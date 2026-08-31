//! Adaptadores de agente.
//!
//! Um adaptador roda um agente e traduz a saída nativa dele para
//! [`EventoAgente`]. O resto do sistema não sabe que existe Claude, Codex ou
//! roteiro — só conhece a sequência de eventos.
//!
//! O contrato aqui é mais estreito que o esboço do `ARQUITETURA.md §5`, e de
//! propósito: `enviar` e `eventos` viraram um método só, [`AgenteAdapter::turno`],
//! porque um turno é sempre pergunta-e-fluxo-de-resposta e separar os dois só
//! abria espaço para chamar na ordem errada. `iniciar` e `retomar` saíram do
//! trait e viraram trabalho da [`Fabrica`], que recebe o
//! `sessao_externa_id` quando existe e decide entre começar e retomar.

use crate::erro::Resultado;
use crate::modelo::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

/// O que o adaptador precisa saber para rodar um agente.
#[derive(Debug, Clone)]
pub struct ContextoSessao {
    pub session_id: String,
    pub node_id: String,
    /// Pasta do workspace. O agente não enxerga nada fora dela.
    pub pasta: String,
    /// Id de retomada devolvido pelo agente numa sessão anterior. `Some` quer
    /// dizer "continue aquela conversa", não "comece outra".
    pub sessao_externa_id: Option<String>,
    /// Segredo que o servidor MCP usa para descobrir qual nó está chamando.
    /// Vai para a configuração MCP do processo do agente e para lugar nenhum
    /// mais — ver `ESPECIFICACAO.md §4`.
    pub token: String,
}

pub trait AgenteAdapter: Send {
    /// Começa um turno. Os eventos chegam pelo canal até um que
    /// [`EventoAgente::encerra_turno`]; depois disso o canal fecha.
    fn turno(&mut self, texto: &str) -> Resultado<Receiver<EventoAgente>>;

    /// Interrompe o turno em andamento. Precisa ser idempotente: o usuário
    /// clica em "parar" duas vezes quando a primeira não parece ter feito nada.
    fn cancelar(&mut self);
}

/// Cria adaptadores sob demanda. O app registra a de verdade; o teste, a que
/// devolve o falso. É o ponto único onde se decide o que roda de fato.
pub trait Fabrica: Send + Sync {
    fn criar(&self, adaptador: Adaptador, ctx: &ContextoSessao)
        -> Resultado<Box<dyn AgenteAdapter>>;
}

// ------------------------------------------------------------------ falso

/// Um turno inteiro, escrito à mão.
///
/// O adaptador falso não é conveniência de teste: testar orquestração contra a
/// API de verdade é lento, caro e não-determinístico. Com roteiro, o mesmo
/// turno roda mil vezes igual e de graça.
#[derive(Debug, Clone)]
pub struct Roteiro {
    /// Pausa entre eventos. `0` nos testes — a espera não prova nada e só
    /// deixa a suíte lenta. Alguns milissegundos no modo demonstração, para a
    /// resposta chegar em pedaços como chegaria de verdade.
    pub atraso_ms: u64,
    pub eventos: Vec<EventoAgente>,
}

impl Roteiro {
    /// Turno de demonstração: pensa, lê um arquivo, responde e cobra.
    ///
    /// Serve para desenvolver a face conversa sem chave de API e para o teste
    /// de fumaça ter o que exercitar.
    pub fn demonstracao(pergunta: &str) -> Roteiro {
        let resposta = format!(
            "Li o material sobre \"{}\". O ponto que salta é a cláusula de \
             reajuste: ela cita um índice que não existe mais desde 2023.",
            resumo_curto(pergunta)
        );
        // Contagem de fantasia, mas coerente: a resposta abaixo tem mesmo essa
        // ordem de grandeza. Número redondo demais denuncia maquete.
        let uso = Uso {
            tokens_entrada: 1_420,
            tokens_saida: 96,
            custo_usd: custo_do_uso(MODELO_DEMONSTRACAO, 1_420, 96),
        };
        Roteiro {
            atraso_ms: 90,
            eventos: vec![
                EventoAgente::SessaoIniciada {
                    id_externo: format!("falso_{}", &novo_id()[..8]),
                    modelo: MODELO_DEMONSTRACAO.to_string(),
                    ferramentas: vec!["ler_arquivo".into(), "listar_arquivos".into()],
                },
                EventoAgente::Raciocinando {
                    resumo: "Procurando o documento na pasta do workspace.".into(),
                },
                EventoAgente::FerramentaPedida {
                    id: "fer_1".into(),
                    nome: "ler_arquivo".into(),
                    argumentos: serde_json::json!({ "caminho": "contrato-v3.docx" }),
                },
                EventoAgente::FerramentaConcluida {
                    id: "fer_1".into(),
                    resultado: Some(serde_json::json!({ "bytes": 48_213, "truncado": false })),
                    erro: None,
                },
                EventoAgente::TextoParcial { delta: "Li o material".into() },
                EventoAgente::TextoParcial { delta: " sobre o documento.".into() },
                EventoAgente::TurnoConcluido { texto_final: resposta, uso },
            ],
        }
    }
}

/// Modelo que o roteiro de demonstração diz estar usando. Fica aqui para o
/// custo falso sair da mesma tabela que o de verdade — se o preço mudar, os
/// dois mudam juntos.
pub const MODELO_DEMONSTRACAO: &str = "claude-opus-5";

type GeradorRoteiro = Arc<dyn Fn(&str) -> Roteiro + Send + Sync>;

pub struct AdaptadorFalso {
    gerador: GeradorRoteiro,
    cancelado: Arc<AtomicBool>,
}

impl AdaptadorFalso {
    /// Sempre o mesmo roteiro, seja qual for a pergunta. É o que os testes usam.
    pub fn com_roteiro(roteiro: Roteiro) -> Self {
        AdaptadorFalso::com(move |_| roteiro.clone())
    }

    /// Roteiro que depende da pergunta.
    pub fn com(f: impl Fn(&str) -> Roteiro + Send + Sync + 'static) -> Self {
        AdaptadorFalso { gerador: Arc::new(f), cancelado: Arc::new(AtomicBool::new(false)) }
    }

    pub fn demonstracao() -> Self {
        AdaptadorFalso::com(Roteiro::demonstracao)
    }
}

impl AgenteAdapter for AdaptadorFalso {
    fn turno(&mut self, texto: &str) -> Resultado<Receiver<EventoAgente>> {
        // Um cancelamento do turno anterior não pode calar o próximo.
        self.cancelado.store(false, Ordering::SeqCst);

        let roteiro = (self.gerador)(texto);
        let cancelado = self.cancelado.clone();
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            for evento in roteiro.eventos {
                if cancelado.load(Ordering::SeqCst) {
                    // Sai calado: quem cancelou já sabe, e o orquestrador é
                    // que decide o estado do nó depois de um cancelamento.
                    return;
                }
                if roteiro.atraso_ms > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(roteiro.atraso_ms));
                }
                // Erro no envio quer dizer que o outro lado desistiu de escutar.
                // Continuar seria falar sozinho.
                if tx.send(evento).is_err() {
                    return;
                }
            }
        });

        Ok(rx)
    }

    fn cancelar(&mut self) {
        self.cancelado.store(true, Ordering::SeqCst);
    }
}

/// Fábrica que devolve o falso para qualquer adaptador pedido. Usada nos
/// testes e no `npm run app` de desenvolvimento, onde gastar token a cada
/// recarregamento de tela seria absurdo.
pub struct FabricaFalsa {
    gerador: GeradorRoteiro,
}

impl FabricaFalsa {
    pub fn demonstracao() -> Self {
        FabricaFalsa { gerador: Arc::new(Roteiro::demonstracao) }
    }

    pub fn com_roteiro(roteiro: Roteiro) -> Self {
        FabricaFalsa { gerador: Arc::new(move |_: &str| roteiro.clone()) }
    }
}

impl Fabrica for FabricaFalsa {
    fn criar(
        &self,
        _adaptador: Adaptador,
        _ctx: &ContextoSessao,
    ) -> Resultado<Box<dyn AgenteAdapter>> {
        let g = self.gerador.clone();
        Ok(Box::new(AdaptadorFalso::com(move |t| g(t))))
    }
}

// ------------------------------------------------------------------ ajudas

/// Primeira linha da pergunta, encurtada. Só para o roteiro de demonstração
/// parecer que leu o que foi perguntado.
fn resumo_curto(texto: &str) -> String {
    let limpo = texto.trim().lines().next().unwrap_or("").trim();
    if limpo.chars().count() <= 40 {
        return limpo.to_string();
    }
    let corte: String = limpo.chars().take(40).collect();
    format!("{}…", corte.trim_end())
}
