//! O orquestrador: quem transforma eventos de adaptador em estado gravado.
//!
//! Regra que governa tudo aqui: **um turno por vez por nó**. Sem ela, duas
//! mensagens chegando ao mesmo agente intercalam contexto e produzem resposta
//! sem sentido. No M1 quem tenta falar durante um turno leva recusa com
//! mensagem clara; a fila de mensagens em ordem é do M3, quando existir mais
//! de um remetente possível.
//!
//! O núcleo não conhece Tauri. Para contar alguma coisa à interface ele chama
//! um [`Sink`] que recebe [`EventoNucleo`] — quem monta esse sink é o
//! `src-tauri`, e a única coisa que ele faz é traduzir variante em nome de
//! evento.

use crate::agente::{AgenteAdapter, ContextoSessao, Fabrica};
use crate::db::Banco;
use crate::erro::{Erro, Resultado};
use crate::modelo::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Por onde o núcleo avisa a interface. Não devolve nada: avisar não pode
/// falhar de um jeito que interesse a quem está gravando no banco.
pub type Sink = Arc<dyn Fn(EventoNucleo) + Send + Sync>;

/// Um sink que joga tudo fora. Usado onde não há interface escutando.
pub fn sink_mudo() -> Sink {
    Arc::new(|_| {})
}

struct SessaoViva {
    adaptador: Box<dyn AgenteAdapter>,
    /// Ligado enquanto o turno atual vale. Cancelar desliga, e a bomba de
    /// eventos passa a ignorar o que ainda estiver a caminho — um adaptador
    /// não para no meio da frase só porque pedimos.
    turno_vivo: Arc<AtomicBool>,
}

pub struct Orquestrador {
    banco: Arc<Mutex<Banco>>,
    fabrica: Arc<dyn Fabrica>,
    sink: Sink,
    vivos: Mutex<HashMap<String, SessaoViva>>,
}

impl Orquestrador {
    pub fn novo(banco: Arc<Mutex<Banco>>, fabrica: Arc<dyn Fabrica>, sink: Sink) -> Self {
        Orquestrador { banco, fabrica, sink, vivos: Mutex::new(HashMap::new()) }
    }

    /// Abre a sessão de um nó, ou devolve a que já existe. Chamado quando a
    /// face conversa aparece — reabrir o app não deve começar conversa nova.
    pub fn abrir_sessao(&self, node_id: &str, adaptador: Adaptador) -> Resultado<Sessao> {
        let banco = self.banco()?;
        if let Some(s) = banco.sessao_do_no(node_id)? {
            return Ok(s);
        }
        banco.criar_sessao(node_id, adaptador)
    }

    /// Manda um turno. Devolve assim que o turno começa — a resposta chega
    /// pelos eventos, não por este retorno.
    pub fn enviar(&self, session_id: &str, texto: &str) -> Resultado<()> {
        let texto = texto.trim();
        if texto.is_empty() {
            return Err(Erro::invalido("não dá para mandar mensagem vazia"));
        }

        let sessao = {
            let banco = self.banco()?;
            let sessao = banco.obter_sessao(session_id)?;

            if !sessao.estado.aceita_turno() {
                return Err(Erro::invalido(
                    "esse nó ainda está no meio de um turno. Espere ou clique em parar.",
                ));
            }
            // Mandar mensagem nova é reconhecer o erro anterior. Poupa o
            // usuário de um botão "ok, entendi" que não serve para nada.
            if sessao.estado == EstadoSessao::Erro {
                banco.mudar_estado_sessao(session_id, EstadoSessao::Ocioso)?;
            }

            banco.gravar_mensagem(session_id, PapelMensagem::Usuario, texto, Uso::default())?;
            banco.mudar_estado_sessao(session_id, EstadoSessao::Pensando)?;
            sessao
        };

        self.avisar_estado(session_id, &sessao.node_id, EstadoSessao::Pensando);

        let turno_vivo = Arc::new(AtomicBool::new(true));
        let receptor = {
            let mut vivos = self.vivos()?;
            if !vivos.contains_key(session_id) {
                let ctx = self.contexto(&sessao)?;
                let adaptador = self.fabrica.criar(sessao.adaptador, &ctx)?;
                vivos.insert(
                    session_id.to_string(),
                    SessaoViva { adaptador, turno_vivo: turno_vivo.clone() },
                );
            }
            let viva = vivos.get_mut(session_id).expect("acabou de ser inserida");
            viva.turno_vivo = turno_vivo.clone();
            viva.adaptador.turno(texto)?
        };

        let banco = self.banco.clone();
        let sink = self.sink.clone();
        let id = session_id.to_string();
        let node_id = sessao.node_id.clone();

        std::thread::spawn(move || {
            bombear(banco, sink, id, node_id, receptor, turno_vivo);
        });

        Ok(())
    }

    /// Interrompe o turno atual. Não é erro: o nó volta a ficar ocioso e a
    /// conversa registra que foi o usuário quem parou.
    pub fn cancelar(&self, session_id: &str) -> Resultado<()> {
        if let Some(viva) = self.vivos()?.get_mut(session_id) {
            viva.turno_vivo.store(false, Ordering::SeqCst);
            viva.adaptador.cancelar();
        }

        let node_id = {
            let banco = self.banco()?;
            let sessao = banco.obter_sessao(session_id)?;
            if sessao.estado == EstadoSessao::Ocioso {
                return Ok(()); // nada em andamento: cancelar de novo é no-op
            }
            banco.gravar_mensagem(
                session_id,
                PapelMensagem::Sistema,
                "Turno interrompido por você.",
                Uso::default(),
            )?;
            banco.mudar_estado_sessao(session_id, EstadoSessao::Ocioso)?;
            sessao.node_id
        };

        self.avisar_estado(session_id, &node_id, EstadoSessao::Ocioso);
        Ok(())
    }

    /// Esquece os adaptadores vivos sem tocar no banco. Serve ao fechar o app:
    /// as sessões continuam gravadas e retomáveis, os processos é que morrem.
    pub fn encerrar_tudo(&self) {
        if let Ok(mut vivos) = self.vivos() {
            for (_, mut viva) in vivos.drain() {
                viva.turno_vivo.store(false, Ordering::SeqCst);
                viva.adaptador.cancelar();
            }
        }
    }

    // ------------------------------------------------------------- internos

    fn contexto(&self, sessao: &Sessao) -> Resultado<ContextoSessao> {
        let banco = self.banco()?;
        let no = banco.obter_no(&sessao.node_id)?;
        let workspace = banco.obter_workspace(&no.workspace_id)?;
        Ok(ContextoSessao {
            session_id: sessao.id.clone(),
            node_id: sessao.node_id.clone(),
            pasta: workspace.pasta,
            sessao_externa_id: sessao.sessao_externa_id.clone(),
            token: banco.token_da_sessao(&sessao.id)?,
        })
    }

    fn avisar_estado(&self, session_id: &str, node_id: &str, estado: EstadoSessao) {
        (self.sink)(EventoNucleo::SessaoEstado {
            session_id: session_id.to_string(),
            node_id: node_id.to_string(),
            estado,
            pede_atencao: estado.pede_atencao(),
        });
    }

    fn banco(&self) -> Resultado<std::sync::MutexGuard<'_, Banco>> {
        self.banco.lock().map_err(|_| Erro::invalido("o banco ficou num estado ruim"))
    }

    fn vivos(&self) -> Resultado<std::sync::MutexGuard<'_, HashMap<String, SessaoViva>>> {
        self.vivos.lock().map_err(|_| Erro::invalido("as sessões ficaram num estado ruim"))
    }
}

// ------------------------------------------------------------------- bomba

/// Consome os eventos de um turno até o fim, aplicando cada um ao banco.
///
/// Roda numa thread por turno. É a única coisa no sistema que escreve a
/// resposta do agente, e por isso é onde mora a garantia de que um turno
/// sempre termina em algum estado — inclusive quando o adaptador some.
fn bombear(
    banco: Arc<Mutex<Banco>>,
    sink: Sink,
    session_id: String,
    node_id: String,
    receptor: std::sync::mpsc::Receiver<EventoAgente>,
    turno_vivo: Arc<AtomicBool>,
) {
    let mut acumulado = String::new();
    let mut terminou = false;

    for evento in receptor {
        if !turno_vivo.load(Ordering::SeqCst) {
            return; // cancelado: quem cancelou já acertou o estado
        }

        // A ordem aqui não é acidental: o evento sai ANTES de ser aplicado.
        //
        // `aplicar` é quem emite `SessaoEstado`, e a interface usa esse estado
        // para reabilitar o campo de escrita. Aplicando primeiro, o campo
        // voltaria a aceitar texto um instante antes de a resposta aparecer na
        // tela — o usuário veria o turno "terminar" vazio.
        //
        // O outro lado da moeda: quem escuta não pode reagir relendo o banco,
        // porque a gravação ainda não aconteceu. Por isso a face conversa monta
        // a bolha a partir do próprio evento. Ver o comentário em Conversa.tsx.
        sink(EventoNucleo::SessaoEvento {
            session_id: session_id.clone(),
            evento: evento.clone(),
        });

        if let Err(e) = aplicar(&banco, &sink, &session_id, &node_id, &evento, &mut acumulado) {
            eprintln!("[mutirao] falha ao aplicar evento da sessão {session_id}: {e}");
        }

        if evento.encerra_turno() {
            terminou = true;
            break;
        }
    }

    // Canal fechado sem sino. O adaptador morreu, travou ou foi encerrado por
    // fora. Deixar o nó "pensando" para sempre é o pior desfecho possível:
    // não pede atenção, não aceita turno novo e não explica nada.
    if !terminou && turno_vivo.load(Ordering::SeqCst) {
        let msg = "O agente parou de responder no meio do turno.";
        if let Ok(b) = banco.lock() {
            let _ = b.gravar_mensagem(&session_id, PapelMensagem::Sistema, msg, Uso::default());
            let _ = b.mudar_estado_sessao(&session_id, EstadoSessao::Erro);
        }
        sink(EventoNucleo::SessaoEstado {
            session_id: session_id.clone(),
            node_id: node_id.clone(),
            estado: EstadoSessao::Erro,
            pede_atencao: true,
        });
    }
}

fn aplicar(
    banco: &Arc<Mutex<Banco>>,
    sink: &Sink,
    session_id: &str,
    node_id: &str,
    evento: &EventoAgente,
    acumulado: &mut String,
) -> Resultado<()> {
    // O guard é solto antes de qualquer chamada ao sink: avisar a interface
    // não deve acontecer com o banco travado.
    let banco = banco.lock().map_err(|_| Erro::invalido("banco travado"))?;
    banco.marcar_sinal(session_id)?;

    match evento {
        EventoAgente::SessaoIniciada { id_externo, .. } => {
            // É isto que permite fechar o app e continuar amanhã.
            banco.definir_sessao_externa(session_id, id_externo)?;
        }

        EventoAgente::TextoParcial { delta } => {
            acumulado.push_str(delta);
        }

        EventoAgente::Raciocinando { .. } => {}

        EventoAgente::FerramentaPedida { id, nome, argumentos } => {
            // No M1 tudo passa como automática: não existe aprovação ainda.
            // A linha é gravada mesmo assim, porque o log de auditoria começa
            // no primeiro turno, não no marco em que ele fica bonito.
            banco.gravar_ferramenta_pedida(
                session_id,
                id,
                nome,
                argumentos,
                Aprovacao::Automatica,
            )?;
        }

        EventoAgente::FerramentaConcluida { id, resultado, erro } => {
            banco.concluir_ferramenta(session_id, id, resultado.as_ref(), erro.as_deref())?;
        }

        EventoAgente::TurnoConcluido { texto_final, uso } => {
            // Adaptador que só manda pedaços não perde a resposta: o texto
            // final vale, e o acumulado é a rede de proteção.
            let texto = if texto_final.trim().is_empty() { acumulado.as_str() } else { texto_final };
            banco.gravar_mensagem(session_id, PapelMensagem::Agente, texto, *uso)?;
            banco.somar_custo(session_id, uso.custo_usd)?;
            banco.mudar_estado_sessao(session_id, EstadoSessao::Ocioso)?;

            let no = banco.obter_no(node_id)?;
            let (total, por_no) = banco.custo_do_workspace(&no.workspace_id)?;
            drop(banco);

            sink(EventoNucleo::SessaoEstado {
                session_id: session_id.to_string(),
                node_id: node_id.to_string(),
                estado: EstadoSessao::Ocioso,
                pede_atencao: false,
            });
            sink(EventoNucleo::CustoAtualizado {
                workspace_id: no.workspace_id,
                total,
                por_no,
            });
            return Ok(());
        }

        EventoAgente::PrecisaHumano { pergunta } => {
            banco.gravar_mensagem(session_id, PapelMensagem::Sistema, pergunta, Uso::default())?;
            banco.mudar_estado_sessao(session_id, EstadoSessao::AguardandoHumano)?;
            drop(banco);
            sink(EventoNucleo::SessaoEstado {
                session_id: session_id.to_string(),
                node_id: node_id.to_string(),
                estado: EstadoSessao::AguardandoHumano,
                pede_atencao: true,
            });
            return Ok(());
        }

        EventoAgente::Erro { mensagem, .. } => {
            banco.gravar_mensagem(session_id, PapelMensagem::Sistema, mensagem, Uso::default())?;
            banco.mudar_estado_sessao(session_id, EstadoSessao::Erro)?;
            drop(banco);
            sink(EventoNucleo::SessaoEstado {
                session_id: session_id.to_string(),
                node_id: node_id.to_string(),
                estado: EstadoSessao::Erro,
                pede_atencao: true,
            });
            return Ok(());
        }
    }

    Ok(())
}
