//! O Git oculto: histórico e trabalho paralelo sem o usuário saber que existe.
//!
//! A `Decisão 3` do `ARQUITETURA.md` promete "Git existe, mas o usuário nunca
//! fica sabendo". Este módulo é a promessa em código, e ele fala **git de
//! linha de comando**, não uma biblioteca.
//!
//! ## Por que a CLI e não libgit2
//!
//! Uma razão só, mas decisiva: `git merge-tree --write-tree --name-only` faz
//! uma mesclagem **seca** e devolve os arquivos que conflitariam, sem tocar em
//! nada no disco. É exatamente a tela de publicar — "6 arquivos alterados, 1
//! conflito" — e reimplementar merge de três vias com libgit2 para chegar ao
//! mesmo lugar é trabalho de meses com muito mais chance de errar.
//!
//! O preço é uma dependência externa. Ela é honesta: [`existe`] responde antes
//! de qualquer promessa, e sem git o app continua servindo — só não tem
//! rascunho. Perder o recurso é aceitável; fingir que ele funciona não é.
//!
//! ## Onde o repositório mora
//!
//! **Fora da pasta do usuário**, e o `ARQUITETURA.md` dizia `.mutirao/` dentro
//! dela. A mudança tem motivo, e ele é específico do Windows 11: a pasta de
//! trabalho de alguém quase sempre está em `Documentos`, que quase sempre está
//! sincronizada com o OneDrive. Um diretório Git dentro de uma pasta
//! sincronizada é uma forma conhecida de corromper o repositório — o
//! sincronizador mexe em arquivos que o Git presume só seus.
//!
//! Com o repositório fora, a pasta do usuário fica **literalmente limpa**:
//! nenhum `.git`, nenhum `.mutirao`, nada para o Explorer mostrar ou para
//! outra ferramenta se confundir. Medido: depois de `init` e um commit, `ls -a`
//! na pasta lista só os arquivos do trabalho.
//!
//! O custo: mover a pasta do workspace desliga o histórico. Já era assim — o
//! caminho absoluto está gravado em `workspace.pasta` desde o M0 — então isto
//! não piora nada que já não estivesse quebrado.
//!
//! ## Todo git sai por `processo::novo`
//!
//! Nenhum `Command::new` direto aqui. Publicar dispara vários gits em sequência
//! e, num app GUI do Windows, cada um deles pisca uma janela de console preta —
//! o usuário veria a tela de publicar respondendo com um estrobo. O porquê
//! está em `processo.rs`.

use crate::erro::{Erro, Resultado};
use crate::processo;
use std::path::Path;
use std::process::Stdio;

/// Identidade dos commits que o app faz sozinho.
///
/// Não usa a identidade do usuário de propósito: estes commits não são dele,
/// são do programa. E `user.name` global pode nem existir na máquina — o que
/// faria o primeiro commit falhar com uma mensagem sobre configurar o Git, que
/// é justamente o que o usuário nunca deveria ver.
const AUTOR: &[&str] = &[
    "-c",
    "user.name=Mutirão",
    "-c",
    "user.email=mutirao@localhost",
    // A assinatura do usuário, se configurada, pediria senha da chave no meio
    // de um "publicar". Desligada explicitamente.
    "-c",
    "commit.gpgsign=false",
];

/// O git está instalado?
///
/// Chamado antes de prometer rascunho a alguém. Sem git o app continua
/// servindo — canvas, agentes, aprovação, times — e só o trabalho paralelo
/// fica de fora, dito em voz alta.
pub fn existe() -> bool {
    processo::novo("git")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Roda um git com o diretório de repositório e a árvore de trabalho dados.
///
/// Os dois sempre explícitos: sem eles, o git procura um repositório subindo a
/// árvore de diretórios e pode achar **outro** — o do projeto que por acaso
/// contém a pasta do usuário. Achar o repositório errado e commitar nele é uma
/// falha que só aparece na máquina de outra pessoa.
fn git(git_dir: &Path, work_tree: &Path, args: &[&str]) -> Resultado<String> {
    let saida = processo::novo("git")
        .arg("--git-dir")
        .arg(git_dir)
        .arg("--work-tree")
        .arg(work_tree)
        .args(AUTOR)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| Erro::invalido(format!("não consegui rodar o git: {e}")))?;

    if !saida.status.success() {
        return Err(Erro::invalido(format!(
            "git {} falhou: {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&saida.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&saida.stdout).to_string())
}

/// Git dentro de um worktree, sem `--git-dir`. Um worktree criado por
/// `worktree add` tem um `.git` que aponta para o repositório principal, então
/// aqui o git se acha sozinho.
fn git_em(pasta: &Path, args: &[&str]) -> Resultado<String> {
    let saida = processo::novo("git")
        .arg("-C")
        .arg(pasta)
        .args(AUTOR)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| Erro::invalido(format!("não consegui rodar o git: {e}")))?;
    if !saida.status.success() {
        return Err(Erro::invalido(format!(
            "git {} falhou: {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&saida.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&saida.stdout).to_string())
}

/// A linha do ramo principal. Fixa, e não `git config init.defaultBranch`: o
/// repositório é nosso, e depender da configuração da máquina faria o mesmo
/// app se comportar diferente em cada computador.
pub const RAMO_PRINCIPAL: &str = "principal";

/// Prepara o repositório de um workspace e grava o estado atual da pasta.
///
/// Idempotente: chamar de novo num repositório que já existe não faz nada.
/// É o que permite chamar isto na abertura do workspace sem checar antes.
pub fn preparar(git_dir: &Path, pasta: &Path) -> Resultado<()> {
    if git_dir.join("HEAD").exists() {
        return Ok(());
    }
    if let Some(pai) = git_dir.parent() {
        std::fs::create_dir_all(pai)?;
    }

    let saida = processo::novo("git")
        .args(["init", "--bare", "--quiet", "--initial-branch", RAMO_PRINCIPAL])
        .arg(git_dir)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| Erro::invalido(format!("não consegui rodar o git: {e}")))?;
    if !saida.status.success() {
        return Err(Erro::invalido(format!(
            "não consegui preparar o histórico: {}",
            String::from_utf8_lossy(&saida.stderr).trim()
        )));
    }

    // `--bare` cria o repositório sem árvore; desligamos isso para ele aceitar
    // `--work-tree` apontando para a pasta do usuário.
    git(git_dir, pasta, &["config", "core.bare", "false"])?;
    // A pasta do usuário não é um checkout comum: ninguém vai fazer `git
    // status` nela. Marcar como não-suja evita que o git tente arrumar
    // permissões de arquivo vindas do Windows.
    git(git_dir, pasta, &["config", "core.fileMode", "false"])?;

    gravar_estado(git_dir, pasta, "o trabalho como estava")?;
    Ok(())
}

/// Grava tudo que está na pasta agora, e devolve o commit.
///
/// **`add -A`, e não `add -u`.** Medido: `-u` só pega arquivo já rastreado, e o
/// que um agente faz o tempo todo é **criar** arquivo. Com `-u`, o parecer que
/// o Redator acabou de escrever ficaria de fora do rascunho e sumiria na
/// publicação — perda de trabalho silenciosa, que é a pior espécie.
///
/// Nada para gravar não é erro: devolve o commit que já era o topo.
pub fn gravar_estado(git_dir: &Path, pasta: &Path, motivo: &str) -> Resultado<String> {
    git(git_dir, pasta, &["add", "-A"])?;
    let limpo = git(git_dir, pasta, &["status", "--porcelain"])?.trim().is_empty();
    if !limpo || topo(git_dir, pasta).is_err() {
        // `--allow-empty` cobre o primeiro commit de uma pasta vazia: sem ele,
        // um workspace novo e sem arquivos não teria commit nenhum, e todo o
        // resto (ramo, worktree, merge) precisa de um.
        git(git_dir, pasta, &["commit", "--quiet", "--allow-empty", "-m", motivo])?;
    }
    topo(git_dir, pasta)
}

/// O commit no topo do ramo atual.
pub fn topo(git_dir: &Path, pasta: &Path) -> Resultado<String> {
    Ok(git(git_dir, pasta, &["rev-parse", "HEAD"])?.trim().to_string())
}

/// Abre um worktree novo num ramo novo. É um rascunho nascendo.
pub fn abrir_worktree(
    git_dir: &Path,
    pasta: &Path,
    ramo: &str,
    destino: &Path,
) -> Resultado<()> {
    if let Some(pai) = destino.parent() {
        std::fs::create_dir_all(pai)?;
    }
    let destino_txt = destino.to_string_lossy().to_string();
    git(git_dir, pasta, &["worktree", "add", "--quiet", "-b", ramo, &destino_txt])?;
    Ok(())
}

/// Fecha um worktree e apaga o ramo. É um rascunho sendo descartado.
///
/// `--force` porque o rascunho quase sempre tem alteração não gravada — é o
/// estado normal de um rascunho —, e `-D` porque um ramo não publicado nunca
/// foi mesclado, então o `-d` educado recusaria sempre. Quem chega aqui já
/// decidiu jogar fora; recusar seria só um obstáculo sem informação nova.
pub fn fechar_worktree(git_dir: &Path, pasta: &Path, ramo: &str, destino: &Path) -> Resultado<()> {
    let destino_txt = destino.to_string_lossy().to_string();
    // Cada passo tolera falha: um worktree já removido à mão não pode impedir
    // o resto da limpeza.
    let _ = git(git_dir, pasta, &["worktree", "remove", "--force", &destino_txt]);
    let _ = git(git_dir, pasta, &["worktree", "prune"]);
    let _ = git(git_dir, pasta, &["branch", "-D", ramo]);
    if destino.exists() {
        let _ = std::fs::remove_dir_all(destino);
    }
    Ok(())
}

/// Grava o estado de um worktree de rascunho.
pub fn gravar_worktree(worktree: &Path, motivo: &str) -> Resultado<String> {
    git_em(worktree, &["add", "-A"])?;
    let limpo = git_em(worktree, &["status", "--porcelain"])?.trim().is_empty();
    if !limpo {
        git_em(worktree, &["commit", "--quiet", "-m", motivo])?;
    }
    Ok(git_em(worktree, &["rev-parse", "HEAD"])?.trim().to_string())
}

/// O que muda ao publicar, sem tocar em nada.
#[derive(Debug, Clone, PartialEq)]
pub struct Previa {
    /// Arquivos que mudam, com a letra de estado do git (`A`, `M`, `D`, `R`).
    pub alterados: Vec<(String, String)>,
    /// Arquivos que os dois lados mexeram de formas que não se juntam.
    pub conflitos: Vec<String>,
}

/// Simula a publicação. **Não escreve nada em lugar nenhum.**
///
/// É o que a tela de publicar mostra antes do clique, e é o motivo de este
/// módulo falar com a CLI: `merge-tree --write-tree` faz a mesclagem em
/// memória e nomeia o que conflitaria.
pub fn prever_merge(git_dir: &Path, pasta: &Path, ramo: &str) -> Resultado<Previa> {
    let bruto = git(
        git_dir,
        pasta,
        &["diff", "--name-status", &format!("{RAMO_PRINCIPAL}...{ramo}")],
    )?;
    let alterados = bruto
        .lines()
        .filter_map(|l| {
            let mut partes = l.split('\t');
            let estado = partes.next()?.trim().to_string();
            let caminho = partes.next()?.trim().to_string();
            Some((estado, caminho))
        })
        .collect();

    // Este é o único lugar em que a falha do git **não** é erro: código 1 quer
    // dizer "conflitaria", que é resposta legítima e o que a tela precisa.
    let saida = processo::novo("git")
        .arg("--git-dir")
        .arg(git_dir)
        .arg("--work-tree")
        .arg(pasta)
        .args(AUTOR)
        .args(["merge-tree", "--write-tree", "--name-only", RAMO_PRINCIPAL, ramo])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| Erro::invalido(format!("não consegui rodar o git: {e}")))?;

    let texto = String::from_utf8_lossy(&saida.stdout);
    // Formato medido: primeira linha é o oid da árvore; depois, um por linha,
    // os caminhos em conflito; depois uma linha em branco e a explicação em
    // inglês, que não interessa a ninguém aqui.
    let conflitos: Vec<String> = if saida.status.success() {
        Vec::new()
    } else {
        texto
            .lines()
            .skip(1)
            .take_while(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
            .collect()
    };

    Ok(Previa { alterados, conflitos })
}

/// De qual lado ficar quando os dois mexeram no mesmo arquivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lado {
    /// O que já estava na pasta.
    Original,
    /// O que o rascunho fez.
    Rascunho,
}

/// Publica o rascunho na pasta de verdade.
///
/// Quem chama precisa ter gravado os dois lados antes — é
/// `ensaios::publicar` que faz isso, e o motivo está lá: mesclar numa pasta
/// com alteração não gravada **não é recusado pelo git**, ele mescla e deixa
/// marcador de conflito dentro do arquivo do usuário. Medido.
///
/// `escolhas` diz o que fazer com cada conflito. Um conflito sem escolha
/// aborta a publicação inteira e devolve a pasta como estava: publicar pela
/// metade é pior que não publicar.
pub fn publicar(
    git_dir: &Path,
    pasta: &Path,
    ramo: &str,
    escolhas: &[(String, Lado)],
) -> Resultado<()> {
    let saida = processo::novo("git")
        .arg("--git-dir")
        .arg(git_dir)
        .arg("--work-tree")
        .arg(pasta)
        .args(AUTOR)
        .args(["merge", "--no-edit", "--no-ff", ramo])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| Erro::invalido(format!("não consegui rodar o git: {e}")))?;

    if saida.status.success() {
        return Ok(());
    }

    // Conflito. Resolve o que foi escolhido; o que sobrar derruba tudo.
    let pendentes = git(git_dir, pasta, &["diff", "--name-only", "--diff-filter=U"])?;
    for arquivo in pendentes.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let escolha = escolhas.iter().find(|(a, _)| a == arquivo).map(|(_, l)| *l);
        let Some(lado) = escolha else {
            let _ = git(git_dir, pasta, &["merge", "--abort"]);
            return Err(Erro::invalido(format!(
                "\"{arquivo}\" mudou dos dois lados e ninguém escolheu qual fica. \
                 Nada foi publicado."
            )));
        };
        // `--ours` é o lado da pasta, `--theirs` é o do rascunho: durante um
        // merge, "nosso" é para onde estamos mesclando.
        let qual = match lado {
            Lado::Original => "--ours",
            Lado::Rascunho => "--theirs",
        };
        git(git_dir, pasta, &["checkout", qual, "--", arquivo])?;
        git(git_dir, pasta, &["add", "--", arquivo])?;
    }

    git(git_dir, pasta, &["commit", "--quiet", "--no-edit"])?;
    Ok(())
}

/// Desfaz uma publicação em andamento e devolve a pasta ao estado anterior.
pub fn abortar(git_dir: &Path, pasta: &Path) {
    let _ = git(git_dir, pasta, &["merge", "--abort"]);
}
