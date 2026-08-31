use serde::Serialize;

/// Erro que atravessa a fronteira IPC. O front recebe sempre este formato —
/// nunca uma string solta — para poder decidir pelo `codigo` e mostrar a
/// `mensagem`. Contrato espelhado em `src/lib/tipos.ts`.
#[derive(Debug, Serialize)]
pub struct ErroIpc {
    pub codigo: String,
    pub mensagem: String,
}

impl From<nucleo::Erro> for ErroIpc {
    fn from(e: nucleo::Erro) -> Self {
        // Erro de banco e de io não vão crus para a interface: o usuário não
        // tem o que fazer com "UNIQUE constraint failed" e isso vaza esquema.
        let mensagem = match &e {
            nucleo::Erro::Banco(_) | nucleo::Erro::Io(_) | nucleo::Erro::Json(_) => {
                eprintln!("[mutirao] erro interno: {e}");
                "Algo falhou aqui dentro. Se repetir, feche e abra o app.".to_string()
            }
            outro => outro.to_string(),
        };
        ErroIpc { codigo: e.codigo().to_string(), mensagem }
    }
}

pub type ResultadoIpc<T> = std::result::Result<T, ErroIpc>;
