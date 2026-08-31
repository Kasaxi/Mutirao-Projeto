//! Rascunhos: duas versões do mesmo trabalho ao mesmo tempo.
//!
//! O usuário vê "Rascunho 2" e "Publicar". Nunca lê branch, commit ou merge —
//! é a `Decisão 3` do `ARQUITETURA.md`, e ela vale até nas mensagens de erro
//! deste arquivo.
//!
//! ## O perigo que este módulo existe para não cometer
//!
//! O adaptador roda o processo do agente com `current_dir` na pasta de
//! trabalho, e essa pasta é decidida **uma vez**, quando o processo abre.
//! Trocar de rascunho sem derrubar os processos vivos deixaria um agente
//! gravando na pasta antiga — e gravando *com aprovação legítima*, porque o
//! card mostra o conteúdo, não o destino. O usuário aprovaria de boa-fé uma
//! gravação no lugar errado.
//!
//! Por isso [`trocar`] derruba os adaptadores antes de mudar o ponteiro, e é a
//! razão de esta função existir em vez de o front chamar
//! `definir_ensaio_ativo` direto.

use crate::db::Banco;
use crate::erro::{Erro, Resultado};
use crate::git;
use crate::modelo::*;
use crate::orquestrador::Orquestrador;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Onde ficam os worktrees dos rascunhos de um workspace.
///
/// Ao lado do repositório, e portanto fora da pasta do usuário: um rascunho é
/// trabalho pela metade, e trabalho pela metade não deve aparecer na pasta que
/// a pessoa abre no Explorer. É o mesmo motivo de o repositório estar fora —
/// ver o cabeçalho de `git.rs`.
fn pasta_dos_rascunhos(repo: &Path) -> PathBuf {
    repo.with_extension("rascunhos")
}

/// Prepara o histórico de um workspace, se ainda não existir.
///
/// Chamado ao abrir o workspace. Sem git na máquina devolve `Ok(false)` em vez
/// de erro: o app inteiro continua servindo, só não tem rascunho, e quem
/// chama diz isso na tela.
pub fn preparar(banco: &Banco, workspace_id: &str) -> Resultado<bool> {
    if !git::existe() {
        return Ok(false);
    }
    let ws = banco.obter_workspace(workspace_id)?;
    let Some(repo) = ws.repo_ou_erro().ok() else {
        return Ok(false);
    };
    git::preparar(Path::new(&repo), Path::new(&ws.pasta))?;
    Ok(true)
}

/// Abre um rascunho novo a partir do estado atual da pasta.
///
/// O estado da pasta é gravado antes: sem isso, o rascunho nasceria de um
/// commit velho e o trabalho feito à mão desde então apareceria como diferença
/// a publicar — o usuário veria o próprio trabalho listado como mudança do
/// agente.
pub fn criar(banco: &Banco, workspace_id: &str, nome: &str) -> Resultado<Ensaio> {
    let ws = banco.obter_workspace(workspace_id)?;
    let repo = ws.repo_ou_erro()?;
    let (repo, pasta) = (PathBuf::from(&repo), PathBuf::from(&ws.pasta));

    let base = git::gravar_estado(&repo, &pasta, "antes de abrir um rascunho")?;

    // O ramo leva um id, não o nome: o usuário chama o rascunho de "Rascunho
    // 2" ou "com a cláusula nova", e nome de gente vira nome de ramo inválido
    // na primeira barra ou acento.
    let id_curto = novo_id();
    let ramo = format!("rascunho/{}", &id_curto[..8]);
    let destino = pasta_dos_rascunhos(&repo).join(&id_curto[..8]);

    git::abrir_worktree(&repo, &pasta, &ramo, &destino)?;

    banco
        .criar_ensaio(
            workspace_id,
            nome,
            &ramo,
            &destino.to_string_lossy(),
            Some(&base),
        )
        .inspect_err(|_| {
            // O worktree ficou no disco e a linha no banco não existe. Limpar
            // é obrigatório: um worktree órfão faz o `worktree add` seguinte
            // falhar com uma mensagem sobre um caminho que ninguém reconhece.
            let _ = git::fechar_worktree(&repo, &pasta, &ramo, &destino);
        })
}

/// Troca o rascunho em foco. `None` volta para a pasta de verdade.
///
/// **Derruba os adaptadores vivos antes de mudar o ponteiro.** Ver o cabeçalho
/// deste módulo: sem isso, um agente já aberto continuaria gravando na pasta
/// anterior, e o card de aprovação mostraria o conteúdo certo com o destino
/// errado. Os processos morrem; as sessões ficam gravadas e o próximo turno as
/// retoma — o usuário perde o processo, não a conversa.
pub fn trocar(
    banco: &Banco,
    orq: &Arc<Orquestrador>,
    workspace_id: &str,
    ensaio: Option<&str>,
) -> Resultado<()> {
    // Nesta ordem: derrubar primeiro e trocar depois. Ao contrário, existiria
    // uma janela — curta, mas real — em que um turno começaria já com o
    // ponteiro novo e a pasta velha.
    orq.encerrar_tudo();
    banco.definir_ensaio_ativo(workspace_id, ensaio)
}

/// Joga um rascunho fora. O trabalho dele some junto — é o que "descartar"
/// quer dizer, e a tela precisa ter perguntado antes.
pub fn descartar(
    banco: &Banco,
    orq: &Arc<Orquestrador>,
    ensaio_id: &str,
) -> Resultado<()> {
    let e = banco.obter_ensaio(ensaio_id)?;
    let ws = banco.obter_workspace(&e.workspace_id)?;

    // Descartar o rascunho em foco tira o foco dele antes de apagar a pasta —
    // senão o trabalho seguinte aconteceria num caminho que não existe mais.
    if ws.ensaio_ativo.as_deref() == Some(ensaio_id) {
        trocar(banco, orq, &e.workspace_id, None)?;
    }
    if let Ok(repo) = ws.repo_ou_erro() {
        let _ = git::fechar_worktree(
            Path::new(&repo),
            Path::new(&ws.pasta),
            &e.branch,
            Path::new(&e.caminho_worktree),
        );
    }
    banco.mudar_estado_ensaio(ensaio_id, EstadoEnsaio::Descartado)
}

/// O que muda ao publicar. **Não escreve nada.**
///
/// Grava o estado do rascunho antes de comparar, porque o agente escreveu
/// arquivos e ninguém commitou nada — sem isto a prévia mostraria "nenhuma
/// alteração" para um rascunho cheio de trabalho, que é a mentira mais cara
/// que esta tela poderia contar.
pub fn prever(banco: &Banco, ensaio_id: &str) -> Resultado<PreviaPublicacao> {
    let e = banco.obter_ensaio(ensaio_id)?;
    let ws = banco.obter_workspace(&e.workspace_id)?;
    let repo = ws.repo_ou_erro()?;

    git::gravar_worktree(Path::new(&e.caminho_worktree), "trabalho do rascunho")?;
    let previa = git::prever_merge(Path::new(&repo), Path::new(&ws.pasta), &e.branch)?;

    Ok(PreviaPublicacao {
        ensaio_id: ensaio_id.to_string(),
        alteracoes: previa
            .alterados
            .into_iter()
            .map(|(letra, caminho)| MudancaArquivo {
                caminho,
                como: TipoMudanca::da_letra(&letra),
            })
            .collect(),
        conflitos: previa.conflitos,
    })
}

/// Leva o rascunho para a pasta de verdade.
///
/// Grava os dois lados antes de mesclar, e essa ordem não é zelo: medido,
/// mesclar numa pasta com alteração não gravada **não é recusado pelo git** —
/// ele mescla e deixa marcador de conflito dentro do arquivo do usuário. O
/// trabalho que a pessoa fez à mão desde a última vez viraria lixo de merge no
/// meio do documento dela.
///
/// Conflito sem escolha aborta tudo e devolve a pasta como estava: publicar
/// pela metade é pior que não publicar.
pub fn publicar(
    banco: &Banco,
    orq: &Arc<Orquestrador>,
    ensaio_id: &str,
    escolhas: &[(String, LadoDoConflito)],
) -> Resultado<PreviaPublicacao> {
    let e = banco.obter_ensaio(ensaio_id)?;
    if e.estado != EstadoEnsaio::Aberto {
        return Err(Erro::invalido(format!(
            "o rascunho \"{}\" já foi {}",
            e.nome,
            e.estado.como_texto()
        )));
    }
    let ws = banco.obter_workspace(&e.workspace_id)?;
    let repo = PathBuf::from(ws.repo_ou_erro()?);
    let pasta = PathBuf::from(&ws.pasta);

    // Nenhum agente pode estar escrevendo enquanto a pasta é reescrita por
    // baixo dele. Este é o mesmo perigo do `trocar`, com outra roupa.
    orq.encerrar_tudo();

    git::gravar_worktree(Path::new(&e.caminho_worktree), "trabalho do rascunho")?;
    git::gravar_estado(&repo, &pasta, "o que estava na pasta antes de publicar")?;

    let convertidas: Vec<(String, git::Lado)> = escolhas
        .iter()
        .map(|(caminho, lado)| {
            let l = match lado {
                LadoDoConflito::Original => git::Lado::Original,
                LadoDoConflito::Rascunho => git::Lado::Rascunho,
            };
            (caminho.clone(), l)
        })
        .collect();

    let feito = git::prever_merge(&repo, &pasta, &e.branch)?;
    if let Err(erro) = git::publicar(&repo, &pasta, &e.branch, &convertidas) {
        git::abortar(&repo, &pasta);
        return Err(erro);
    }

    banco.mudar_estado_ensaio(ensaio_id, EstadoEnsaio::Publicado)?;
    if ws.ensaio_ativo.as_deref() == Some(ensaio_id) {
        banco.definir_ensaio_ativo(&e.workspace_id, None)?;
    }
    // O worktree cumpriu o papel. A linha do rascunho fica: "o que aconteceu
    // com aquele rascunho?" precisa ter resposta depois de publicado.
    let _ = git::fechar_worktree(&repo, &pasta, &e.branch, Path::new(&e.caminho_worktree));

    Ok(PreviaPublicacao {
        ensaio_id: ensaio_id.to_string(),
        alteracoes: feito
            .alterados
            .into_iter()
            .map(|(letra, caminho)| MudancaArquivo {
                caminho,
                como: TipoMudanca::da_letra(&letra),
            })
            .collect(),
        conflitos: Vec::new(),
    })
}

impl Workspace {
    /// O repositório oculto deste workspace, ou um erro em português.
    ///
    /// `None` acontece em dois casos legítimos: workspace criado antes do M5,
    /// e máquina sem git. Os dois merecem a mesma frase — o que o usuário
    /// precisa saber é que não há rascunho, não qual das duas causas foi.
    pub fn repo_ou_erro(&self) -> Resultado<String> {
        self.repo.clone().ok_or_else(|| {
            Erro::invalido(
                "este workspace não tem histórico, então não dá para usar rascunhos. \
                 Isso acontece quando o Git não está instalado na máquina."
                    .to_string(),
            )
        })
    }
}
