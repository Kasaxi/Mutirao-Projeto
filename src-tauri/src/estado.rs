use crate::erro::ErroIpc;
use nucleo::Banco;
use std::sync::{Mutex, MutexGuard};

/// Estado global do app. Uma conexão só, protegida por Mutex.
///
/// Por que um Mutex e não um pool: SQLite em WAL aguenta leitura concorrente,
/// mas o volume aqui é de dezenas de operações por minuto, não milhares por
/// segundo. Pool seria complexidade sem ganho. Se um dia a escrita de mensagens
/// de agente virar gargalo (M1 em diante), a troca é local a este arquivo.
pub struct EstadoApp {
    banco: Mutex<Banco>,
}

impl EstadoApp {
    pub fn novo(banco: Banco) -> Self {
        EstadoApp { banco: Mutex::new(banco) }
    }

    pub fn banco(&self) -> Result<MutexGuard<'_, Banco>, ErroIpc> {
        // Um panic segurando o lock envenena o Mutex. Em vez de propagar o
        // panic para todo comando seguinte, devolvemos erro tratável.
        self.banco.lock().map_err(|_| ErroIpc {
            codigo: "banco".into(),
            mensagem: "O banco ficou num estado ruim. Feche e abra o app.".into(),
        })
    }
}
