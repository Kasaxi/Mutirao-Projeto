use thiserror::Error;

/// Erro do núcleo. A borda (Tauri) converte isto em algo serializável
/// para o front — ver `src-tauri/src/erro.rs`.
#[derive(Debug, Error)]
pub enum Erro {
    #[error("banco: {0}")]
    Banco(#[from] rusqlite::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// Não achou o que foi pedido. Carrega o tipo e o id para a mensagem
    /// chegar útil na interface.
    #[error("{tipo} não encontrado: {id}")]
    NaoEncontrado { tipo: &'static str, id: String },

    /// Regra de domínio violada. Texto vai direto para o usuário.
    #[error("{0}")]
    Invalido(String),

    /// Caminho fora do escopo do workspace. Nunca vaza o caminho tentado
    /// para o agente — só para o log.
    #[error("caminho fora do escopo do workspace")]
    ForaDoEscopo,
}

pub type Resultado<T> = std::result::Result<T, Erro>;

impl Erro {
    pub fn nao_encontrado(tipo: &'static str, id: impl Into<String>) -> Self {
        Erro::NaoEncontrado { tipo, id: id.into() }
    }

    pub fn invalido(msg: impl Into<String>) -> Self {
        Erro::Invalido(msg.into())
    }

    /// Código estável para o front decidir o que fazer. Nunca mude estes
    /// literais sem atualizar `src/lib/erros.ts`.
    pub fn codigo(&self) -> &'static str {
        match self {
            Erro::Banco(_) => "banco",
            Erro::Json(_) => "json",
            Erro::Io(_) => "io",
            Erro::NaoEncontrado { .. } => "nao_encontrado",
            Erro::Invalido(_) => "invalido",
            Erro::ForaDoEscopo => "fora_do_escopo",
        }
    }
}
