//! O substrato: a pasta do workspace, e nada além dela.
//!
//! O `ARQUITETURA.md §8` é explícito: *"Escopo de arquivos é verificado no
//! núcleo com caminho canônico, não confiando no agente: qualquer caminho que
//! escape da pasta é negado antes de chegar ao disco."* Este módulo é essa
//! frase virando código, e [`dentro_do_escopo`] é a única porta.
//!
//! Canônico importa mais do que parece. Recusar `..` por texto deixa passar um
//! link simbólico apontando para fora — o caminho não tem `..` nenhum e ainda
//! assim sai da pasta. Só resolver o caminho de verdade pega os dois casos.

use crate::erro::{Erro, Resultado};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// Um item da árvore de arquivos, do jeito que o nó desenha.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemArquivo {
    /// Caminho relativo à pasta do workspace, sempre com `/`, mesmo no Windows.
    pub caminho: String,
    pub nome: String,
    pub pasta: bool,
    pub tamanho: u64,
}

/// Teto de leitura. Um `.xlsx` de 40 MB não cabe na tela nem no IPC, e tentar
/// mandá-lo inteiro trava a interface em vez de dizer que é grande demais.
pub const TETO_LEITURA: u64 = 2 * 1024 * 1024;

/// Resolve um caminho relativo dentro da pasta do workspace, ou recusa.
///
/// Recusa: caminho absoluto, `..`, prefixo de volume (`C:\`), e qualquer coisa
/// que, depois de resolvida de verdade, caia fora da pasta — link simbólico
/// incluído.
///
/// O arquivo pode não existir ainda (é o caso de gravar uma nota nova). Aí o
/// que se resolve é o ancestral mais próximo que exista: se a pasta onde ele
/// vai nascer está dentro do escopo, o arquivo também estará.
pub fn dentro_do_escopo(base: &Path, relativo: &str) -> Resultado<PathBuf> {
    let pedido = Path::new(relativo);

    // Barreira barata primeiro: erra rápido e com mensagem clara.
    for parte in pedido.components() {
        match parte {
            Component::Normal(_) | Component::CurDir => {}
            // ParentDir, RootDir e Prefix são as três formas de sair da pasta
            // sem link simbólico nenhum.
            _ => return Err(Erro::ForaDoEscopo),
        }
    }

    let base_real = base.canonicalize().map_err(|_| Erro::ForaDoEscopo)?;
    let alvo = base_real.join(pedido);

    // Resolve o que existir. Um link simbólico dentro da pasta apontando para
    // fora só aparece aqui.
    let existente = ancestral_existente(&alvo);
    let real = existente.canonicalize().map_err(|_| Erro::ForaDoEscopo)?;
    if !real.starts_with(&base_real) {
        return Err(Erro::ForaDoEscopo);
    }

    Ok(alvo)
}

fn ancestral_existente(caminho: &Path) -> PathBuf {
    let mut atual = caminho.to_path_buf();
    while !atual.exists() {
        match atual.parent() {
            Some(p) => atual = p.to_path_buf(),
            None => break,
        }
    }
    atual
}

/// Lista uma pasta. `sub` vazio é a raiz do workspace.
///
/// Pastas primeiro, depois arquivos, ambos em ordem alfabética — a mesma
/// ordem que qualquer gerenciador de arquivos usa, porque é a que o olho
/// espera.
pub fn listar(base: &Path, sub: &str) -> Resultado<Vec<ItemArquivo>> {
    let pasta = dentro_do_escopo(base, sub)?;
    let mut itens = Vec::new();

    for entrada in std::fs::read_dir(&pasta)?.flatten() {
        let nome = entrada.file_name().to_string_lossy().to_string();
        // O Git oculto do §3 é detalhe de implementação: o usuário vê
        // "Rascunho 2", nunca `.mutirao`.
        if nome.starts_with('.') {
            continue;
        }
        let meta = match entrada.metadata() {
            Ok(m) => m,
            Err(_) => continue, // arquivo sumiu entre listar e olhar
        };
        let relativo = if sub.trim().is_empty() {
            nome.clone()
        } else {
            format!("{}/{}", sub.trim_end_matches('/'), nome)
        };
        itens.push(ItemArquivo {
            caminho: relativo,
            nome,
            pasta: meta.is_dir(),
            tamanho: if meta.is_dir() { 0 } else { meta.len() },
        });
    }

    itens.sort_by(|a, b| b.pasta.cmp(&a.pasta).then_with(|| a.nome.to_lowercase().cmp(&b.nome.to_lowercase())));
    Ok(itens)
}

pub fn ler_texto(base: &Path, relativo: &str) -> Resultado<String> {
    let caminho = dentro_do_escopo(base, relativo)?;
    let tamanho = std::fs::metadata(&caminho)?.len();
    if tamanho > TETO_LEITURA {
        return Err(Erro::invalido(format!(
            "esse arquivo tem {:.1} MB — grande demais para abrir aqui",
            tamanho as f64 / 1_048_576.0
        )));
    }
    // Arquivo binário lido como texto vira um monte de caractere de
    // substituição. Dizer que não é texto é mais honesto.
    let bruto = std::fs::read(&caminho)?;
    String::from_utf8(bruto).map_err(|_| Erro::invalido("esse arquivo não é texto"))
}

pub fn escrever_texto(base: &Path, relativo: &str, conteudo: &str) -> Resultado<u64> {
    let caminho = dentro_do_escopo(base, relativo)?;
    if let Some(pai) = caminho.parent() {
        std::fs::create_dir_all(pai)?;
    }
    std::fs::write(&caminho, conteudo)?;
    Ok(conteudo.len() as u64)
}

/// Em qual arquivo esta nota mora.
///
/// O nome sai de `config.arquivo` quando já foi decidido, e do nome do nó na
/// primeira vez. Depois de decidido ele **não muda com o nome do nó**: renomear
/// "Briefing" para "Briefing v2" no canvas não pode deixar um `Briefing.md`
/// órfão na pasta do usuário.
pub fn arquivo_da_nota_do_no(no: &crate::modelo::No) -> String {
    match no.config.get("arquivo").and_then(|v| v.as_str()) {
        Some(a) if !a.trim().is_empty() => a.to_string(),
        _ => arquivo_da_nota(&no.nome),
    }
}

/// O nome de arquivo de uma nota, a partir do nome que o usuário deu ao nó.
///
/// A nota é um `.md` na pasta do workspace — o usuário abre no editor dele,
/// manda por e-mail, versiona. É a diferença entre "memória do app" e
/// "arquivo meu".
pub fn arquivo_da_nota(nome_do_no: &str) -> String {
    let limpo: String = nome_do_no
        .chars()
        .map(|c| match c {
            // O que o Windows recusa em nome de arquivo, mais o que confunde
            // caminho. Acento e espaço passam: o nome é do usuário.
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let limpo = limpo.trim().trim_matches('.').trim();
    if limpo.is_empty() {
        return "nota.md".to_string();
    }
    format!("{limpo}.md")
}
