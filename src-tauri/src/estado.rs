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
    /// Qual adaptador a fábrica vai construir. Fica aqui, e não no front,
    /// porque quem decide é quem descobriu se a CLI existe na máquina —
    /// e uma interface que adivinha isso acaba mentindo.
    adaptador: nucleo::Adaptador,
    /// Uma linha para o usuário ler: versão da CLI, ou por que caiu no falso.
    detalhe: String,
}

impl EstadoApp {
    pub fn novo(
        banco: Arc<Mutex<Banco>>,
        orquestrador: Arc<Orquestrador>,
        adaptador: nucleo::Adaptador,
        detalhe: String,
    ) -> Self {
        EstadoApp { banco, orquestrador, adaptador, detalhe }
    }

    pub fn adaptador(&self) -> nucleo::Adaptador {
        self.adaptador
    }

    pub fn detalhe_do_adaptador(&self) -> &str {
        &self.detalhe
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
