//! Como o núcleo abre um processo — e as duas coisas que o Windows cobra.
//!
//! O núcleo abre processo em dois lugares: a CLI do Claude Code
//! (`claude.rs`) e o git (`git.rs`). Os dois passam por aqui, e não por
//! `Command::new` direto, porque no Windows um `Command::new` cru erra duas
//! vezes — e as duas só aparecem na máquina do usuário, nunca aqui.
//!
//! ## 1. Janela de console
//!
//! O app é GUI (`windows_subsystem = "windows"` no `src-tauri/src/main.rs`).
//! Quando um processo GUI abre um programa de console, o Windows cria um
//! console **novo** para ele — uma janela preta que aparece e some. Um time de
//! quatro agentes pisca quatro janelas por turno, e cada `git` da publicação
//! pisca mais uma. Não quebra nada; faz o app parecer quebrado, que dá no
//! mesmo para quem está olhando.
//!
//! `CREATE_NO_WINDOW` resolve, e é a única razão de [`novo`] existir em vez de
//! `Command::new` espalhado.
//!
//! ## 2. `claude` no Windows costuma ser `claude.cmd`
//!
//! Instalada pelo npm, a CLI vira um `claude.cmd` numa pasta do npm — não um
//! `claude.exe`. E o `Command::new("claude")` do Rust chega no `CreateProcess`
//! do Windows, que procura no PATH acrescentando `.exe`, **não** `.cmd`. Quem
//! resolve `.cmd` é o `cmd.exe`, que não está no caminho.
//!
//! O resultado é a pior mensagem de erro possível: "não encontrei o Claude
//! Code" com a CLI instalada e funcionando no terminal ao lado. Por isso
//! [`achar`] procura no PATH como o console procura, respeitando `PATHEXT`.
//!
//! **A ordem importa.** As extensões vêm antes do nome pelado, e o `.EXE` vem
//! antes do `.CMD` porque é a ordem do `PATHEXT` — quem tem a instalação
//! nativa do Claude Code pega o executável de verdade, e não o atalho do npm.
//!
//! ## O que não dá para medir daqui
//!
//! Este arquivo foi escrito no Linux, então as duas correções acima estão
//! testadas pela lógica ([`procurar`] recebe as extensões de fora justamente
//! para o teste poder simular o Windows), e não pela plataforma. O primeiro
//! `npm run app` no Windows é a primeira prova de verdade.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Abre um `Command` que não pisca janela de console no Windows.
///
/// Use este em vez de `Command::new`. É o motivo de o módulo existir.
pub fn novo(programa: impl AsRef<OsStr>) -> Command {
    let mut cmd = Command::new(programa);
    sem_janela(&mut cmd);
    cmd
}

#[cfg(windows)]
fn sem_janela(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    /// `CREATE_NO_WINDOW` da API do Windows. Constante literal para não
    /// arrastar a dependência `windows-sys` inteira por um número.
    const SEM_JANELA: u32 = 0x0800_0000;
    cmd.creation_flags(SEM_JANELA);
}

#[cfg(not(windows))]
fn sem_janela(_cmd: &mut Command) {}

/// Acha um executável do jeito que o console acharia.
///
/// Devolve `None` quando não existe — quem chama decide se isso é erro ou se
/// dá para seguir sem. Um caminho que já venha com barra não é procurado no
/// PATH, só conferido: quem escreveu um caminho inteiro sabe onde quer.
pub fn achar(nome: &str) -> Option<PathBuf> {
    let caminhos: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    procurar(nome, &caminhos, &extensoes())
}

/// As extensões que contam como executável nesta plataforma.
///
/// No Windows sai do `PATHEXT`, com o padrão do sistema como reserva — a
/// variável pode não existir num ambiente enxuto, e aí `claude.cmd` sumiria de
/// novo. Fora do Windows a lista é vazia: extensão não decide nada, o bit de
/// execução decide.
fn extensoes() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let bruto = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    bruto
        .split(';')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(|e| if e.starts_with('.') { e.to_string() } else { format!(".{e}") })
        .collect()
}

/// A busca em si, com o PATH e as extensões vindos de fora.
///
/// Separada de [`achar`] por um motivo só: assim o teste roda o caminho do
/// Windows numa máquina Linux, passando `[".exe", ".cmd"]` na mão. Sem isso, a
/// correção que este módulo existe para fazer não teria teste nenhum até
/// alguém abrir o app no Windows.
fn procurar(nome: &str, caminhos: &[PathBuf], extensoes: &[String]) -> Option<PathBuf> {
    // Nome com barra é caminho, não nome de programa: o PATH não entra.
    if nome.contains('/') || nome.contains('\\') {
        return candidatos(Path::new(nome), extensoes).into_iter().find(|c| serve(c));
    }

    // Pasta por pasta, e dentro de cada uma todas as extensões — é a ordem do
    // console. O contrário faria uma pasta lá do fim do PATH ganhar da
    // primeira só por ter a extensão "melhor".
    for pasta in caminhos {
        let base = pasta.join(nome);
        if let Some(achado) = candidatos(&base, extensoes).into_iter().find(|c| serve(c)) {
            return Some(achado);
        }
    }
    None
}

/// Os nomes a tentar para um caminho-base, na ordem de preferência.
fn candidatos(base: &Path, extensoes: &[String]) -> Vec<PathBuf> {
    let mut lista: Vec<PathBuf> = extensoes
        .iter()
        .map(|e| {
            let mut nome = base.as_os_str().to_os_string();
            nome.push(e);
            PathBuf::from(nome)
        })
        .collect();
    // O pelado por último: no Windows ele quase nunca serve, e fora do Windows
    // a lista de extensões é vazia e ele é o único.
    lista.push(base.to_path_buf());
    lista
}

/// Este arquivo dá para executar?
///
/// No Unix, existir não basta — um `claude` sem bit de execução numa pasta do
/// PATH faria a busca parar num arquivo que não roda. No Windows quem decide é
/// a extensão, que a busca já tratou.
fn serve(caminho: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(caminho) else {
        return false;
    };
    meta.is_file() && da_para_executar(&meta)
}

#[cfg(unix)]
fn da_para_executar(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn da_para_executar(_meta: &std::fs::Metadata) -> bool {
    true
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Cria um arquivo executável e devolve o caminho.
    fn executavel(pasta: &Path, nome: &str) -> PathBuf {
        let caminho = pasta.join(nome);
        std::fs::write(&caminho, b"#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&caminho, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        caminho
    }

    /// Uma pasta vazia, sempre.
    ///
    /// Apaga antes de criar de propósito: teste que procura arquivo no disco e
    /// herda sobra da rodada anterior passa ou falha pelo motivo errado — o pid
    /// do processo, que entra no nome, o sistema recicla.
    fn temp(sufixo: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("mutirao-proc-{}-{sufixo}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// O conserto inteiro em um teste: no Windows a CLI do npm é `claude.cmd`,
    /// e procurar por `claude` tem de achá-la.
    #[test]
    fn no_windows_claude_do_npm_e_um_cmd_e_a_busca_acha() {
        let pasta = temp("cmd");
        let esperado = executavel(&pasta, "claude.cmd");

        // As extensões vão em minúscula porque este teste roda no Linux, onde
        // o sistema de arquivos diferencia maiúscula. No Windows, que é onde
        // isto vale, `PATHEXT` vem em maiúscula e dá no mesmo.
        let exts = vec![".exe".to_string(), ".cmd".to_string()];
        let achado = procurar("claude", std::slice::from_ref(&pasta), &exts);

        assert_eq!(achado, Some(esperado), "não achou o .cmd — é o bug do Windows");
        // E a prova de que o problema era real: sem extensão nenhuma, que é o
        // que o `CreateProcess` faz sozinho, não acha nada.
        assert_eq!(procurar("claude", &[pasta], &[]), None);
    }

    /// Quem tem a instalação nativa e o atalho do npm fica com a nativa.
    #[test]
    fn o_exe_ganha_do_cmd_porque_e_a_ordem_do_pathext() {
        let pasta = temp("ordem");
        let exe = executavel(&pasta, "claude.exe");
        executavel(&pasta, "claude.cmd");

        let exts = vec![".exe".to_string(), ".cmd".to_string()];
        assert_eq!(procurar("claude", &[pasta], &exts), Some(exe));
    }

    /// A primeira pasta do PATH ganha, mesmo que a segunda tenha a extensão
    /// preferida. É a ordem do console, e trocá-la faria o app rodar um
    /// programa diferente do que o usuário roda no terminal.
    #[test]
    fn a_pasta_manda_mais_que_a_extensao() {
        let primeira = temp("pasta-1");
        let segunda = temp("pasta-2");
        let cmd = executavel(&primeira, "claude.cmd");
        executavel(&segunda, "claude.exe");

        let exts = vec![".exe".to_string(), ".cmd".to_string()];
        assert_eq!(procurar("claude", &[primeira, segunda], &exts), Some(cmd));
    }

    /// `MUTIRAO_CLAUDE_BIN` apontando para um caminho inteiro continua valendo,
    /// e ainda ganha a extensão de brinde.
    #[test]
    fn caminho_inteiro_nao_passa_pelo_path() {
        let pasta = temp("inteiro");
        let esperado = executavel(&pasta, "claude.cmd");
        let pedido = pasta.join("claude");

        let exts = vec![".cmd".to_string()];
        let achado = procurar(&pedido.to_string_lossy(), &[], &exts);
        assert_eq!(achado, Some(esperado));
    }

    #[test]
    fn o_que_nao_existe_devolve_nada_em_vez_de_chutar() {
        let pasta = temp("vazio");
        assert_eq!(procurar("programa-que-nao-existe", &[pasta], &[]), None);
    }

    /// No Unix, arquivo sem bit de execução não conta. Se contasse, a busca
    /// pararia num arquivo que não roda e o erro sairia lá na frente, sem
    /// dizer o porquê.
    #[cfg(unix)]
    #[test]
    fn arquivo_sem_permissao_de_execucao_nao_conta() {
        let pasta = temp("sem-x");
        std::fs::write(pasta.join("claude"), b"nao sou executavel").unwrap();
        assert_eq!(procurar("claude", &[pasta], &[]), None);
    }

    /// O git existe nesta máquina; a busca de verdade tem de achá-lo.
    #[test]
    fn achar_encontra_o_que_esta_no_path_de_verdade() {
        let programa = if cfg!(windows) { "cmd" } else { "sh" };
        assert!(achar(programa).is_some(), "não achei `{programa}` no PATH");
    }
}
