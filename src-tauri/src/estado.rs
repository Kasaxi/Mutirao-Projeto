use crate::erro::ErroIpc;
use nucleo::{Banco, Orquestrador};
use std::sync::{Arc, Mutex, MutexGuard};

/// Estado global do app. Uma conexão só, protegida por Mutex.
///
/// Por que um Mutex e não um pool: SQLite em WAL aguenta leitura concorrente,
/// mas o volume aqui é de dezenas de operações por minuto, não milhares por
/// segundo. Pool seria complexidade sem ganho. Se um dia a escrita de mensagens
/// de agente virar gargalo, a troca é local a este arquivo.
///
/// O banco é `Arc` porque o orquestrador escreve nele de outra thread: cada
/// turno tem uma bomba de eventos própria, e é ela que grava a resposta do
/// agente enquanto a interface segue respondendo a comando.
pub struct EstadoApp {
    banco: Arc<Mutex<Banco>>,
    orquestrador: Arc<Orquestrador>,
}

impl EstadoApp {
    pub fn novo(banco: Arc<Mutex<Banco>>, orquestrador: Arc<Orquestrador>) -> Self {
        EstadoApp { banco, orquestrador }
    }

    pub fn banco(&self) -> Result<MutexGuard<'_, Banco>, ErroIpc> {
        // Um panic segurando o lock envenena o Mutex. Em vez de propagar o
        // panic para todo comando seguinte, devolvemos erro tratável.
        self.banco.lock().map_err(|_| ErroIpc {
            codigo: "banco".into(),
            mensagem: "O banco ficou num estado ruim. Feche e abra o app.".into(),
        })
    }

    pub fn orquestrador(&self) -> &Orquestrador {
        &self.orquestrador
    }
}
