//! O orquestrador: quem transforma eventos de adaptador em estado gravado, e
//! quem carrega recado de um nó para outro.
//!
//! Regra que governa tudo aqui: **um turno por vez por nó**. Sem ela, duas
//! mensagens chegando ao mesmo agente intercalam contexto e produzem resposta
//! sem sentido. Desde o M3 quem chega durante um turno entra na fila em ordem,
//! em vez de levar recusa — porque agora existe mais de um remetente possível,
//! e recusar o recado de outro nó seria perder trabalho.
//!
//! O núcleo não conhece Tauri. Para contar alguma coisa à interface ele chama
//! um [`Sink`] que recebe [`EventoNucleo`] — quem monta esse sink é o
//! `src-tauri`, e a única coisa que ele faz é traduzir variante em nome de
//! evento.

use crate::agente::{AgenteAdapter, ContextoSessao, Fabrica};
use crate::db::Banco;
use crate::erro::{Erro, Resultado};
use crate::modelo::*;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

/// Uma mensagem esperando a vez do nó.
struct Pendencia {
    texto: String,
    papel: PapelMensagem,
    origem_node: Option<String>,
    trace: Trace,
    /// Quem espera a resposta deste turno. `None` quando é o usuário falando —
    /// ele lê a resposta na tela, não por retorno de função.
    responder: Option<Sender<Resultado<String>>>,
}

pub struct Orquestrador {
    banco: Arc<Mutex<Banco>>,
    fabrica: Arc<dyn Fabrica>,
    sink: Sink,
    vivos: Mutex<HashMap<String, SessaoViva>>,
    aprovacoes: Arc<crate::barramento::Aprovacoes>,
    /// Preenchido depois que o barramento sobe — ele precisa das `aprovacoes`
    /// daqui, então nasce depois. Enquanto for `None`, as sessões rodam
    /// somente leitura e sem ponte.
    url_barramento: Mutex<Option<String>>,
    /// Mensagens esperando a vez de cada sessão.
    filas: Mutex<HashMap<String, VecDeque<Pendencia>>>,
    /// A cadeia em que cada sessão está agora. É daqui que `enviar_para` tira
    /// o trace para passar adiante.
    traces: Mutex<HashMap<String, Trace>>,
    /// Quem espera o fim do turno de uma sessão (o outro lado de um `enviar_para`).
    esperando: Mutex<HashMap<String, Sender<Resultado<String>>>>,
    /// Quem está parado esperando quem: `bloqueados[A] = B` quer dizer "A está
    /// em `aguardando_no` por causa de B". É sobre este mapa que roda a
    /// detecção de ciclo — ver [`Orquestrador::fecharia_ciclo`].
    bloqueados: Mutex<HashMap<String, String>>,
    /// Perguntas ao humano abertas, por sessão. A resposta é a próxima
    /// mensagem que o usuário mandar àquele nó.
    perguntas: Mutex<HashMap<String, Sender<String>>>,
}

impl Orquestrador {
    pub fn novo(banco: Arc<Mutex<Banco>>, fabrica: Arc<dyn Fabrica>, sink: Sink) -> Self {
        Orquestrador {
            banco,
            fabrica,
            sink,
            vivos: Mutex::new(HashMap::new()),
            aprovacoes: crate::barramento::Aprovacoes::nova(),
            url_barramento: Mutex::new(None),
            filas: Mutex::new(HashMap::new()),
            traces: Mutex::new(HashMap::new()),
            esperando: Mutex::new(HashMap::new()),
            bloqueados: Mutex::new(HashMap::new()),
            perguntas: Mutex::new(HashMap::new()),
        }
    }

    /// Diz ao orquestrador onde o barramento subiu. Até isto acontecer, nenhum
    /// agente pode gravar nem falar com outro nó — não por precaução vaga, mas
    /// porque sem barramento não existe quem aprove nem por onde falar.
    pub fn ligar_barramento(&self, url_base: impl Into<String>) {
        if let Ok(mut u) = self.url_barramento.lock() {
            *u = Some(url_base.into());
        }
    }

    /// A mesma fila de pendências que o barramento usa. O app passa isto para
    /// `Barramento::subir` — os dois lados precisam do mesmo mapa, senão a
    /// decisão do usuário não chega a quem está esperando.
    pub fn aprovacoes(&self) -> Arc<crate::barramento::Aprovacoes> {
        self.aprovacoes.clone()
    }

    /// A mesma coisa sem clonar o `Arc`. Existe para o laço do barramento, que
    /// chama isto uma vez por requisição.
    pub fn aprovacoes_ref(&self) -> &Arc<crate::barramento::Aprovacoes> {
        &self.aprovacoes
    }

    /// O usuário clicou. Grava no log de auditoria, solta o agente e avisa a
    /// interface — nessa ordem: soltar antes de gravar deixaria o arquivo no
    /// disco antes de a linha que o autoriza existir.
    pub fn decidir_aprovacao(
        &self,
        tool_call_id: &str,
        decisao: Decisao,
        lembrar: bool,
    ) -> Resultado<()> {
        let node_id = {
            let banco = self.banco()?;
            let chamada = banco.obter_ferramenta(tool_call_id)?;
            let sessao = banco.obter_sessao(&chamada.session_id)?;
            let no = banco.obter_no(&sessao.node_id)?;

            // "Não perguntar de novo" só faz sentido para o que foi aprovado.
            // Guardar um "não" permanente esconderia a ferramenta sem o
            // usuário nunca mais ser lembrado disso.
            if lembrar && decisao == Decisao::Aprovada {
                banco.conceder_regra(&no.workspace_id, &chamada.ferramenta)?;
            }
            banco.decidir_ferramenta(tool_call_id, decisao, "usuario")?;
            sessao.node_id
        };

        // Só agora o agente sai do lugar.
        self.aprovacoes.responder(tool_call_id, decisao);

        (self.sink)(EventoNucleo::AprovacaoDecidida {
            tool_call_id: tool_call_id.to_string(),
            node_id,
            decisao,
            decidido_por: "usuario".to_string(),
        });
        Ok(())
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

    // ------------------------------------------------------------- entrada

    /// O usuário falou com um nó. Devolve assim que a mensagem entra na fila —
    /// a resposta chega pelos eventos, não por este retorno.
    pub fn enviar(self: &Arc<Self>, session_id: &str, texto: &str) -> Resultado<()> {
        let texto = texto.trim();
        if texto.is_empty() {
            return Err(Erro::invalido("não dá para mandar mensagem vazia"));
        }

        // Se o nó perguntou alguma coisa e está esperando, esta mensagem é a
        // resposta — não um turno novo. Sem isto o usuário responderia à
        // pergunta e o agente continuaria parado esperando por ela.
        let esperando_resposta = self
            .perguntas
            .lock()
            .ok()
            .and_then(|mut p| p.remove(session_id));
        if let Some(canal) = esperando_resposta {
            let node_id = {
                let banco = self.banco()?;
                let sessao = banco.obter_sessao(session_id)?;
                banco.gravar_mensagem(session_id, PapelMensagem::Usuario, texto, Uso::default())?;
                // O turno nunca parou: ele estava parado DENTRO da ferramenta.
                // Sem esta linha o nó ficaria em `aguardando_humano` no banco
                // para sempre — pedindo atenção que ninguém mais deve.
                banco.mudar_estado_sessao(session_id, EstadoSessao::Pensando)?;
                sessao.node_id
            };
            let _ = canal.send(texto.to_string());
            self.avisar_estado(session_id, &node_id, EstadoSessao::Pensando);
            return Ok(());
        }

        // Sessão inexistente vira erro aqui, e não lá dentro da fila: quem
        // chamou merece saber na hora que o nó não existe.
        self.banco()?.obter_sessao(session_id)?;

        // Mandar mensagem nova é reconhecer o erro anterior — quem faz isso é
        // `iniciar_turno`, para valer também para o recado que vem de outro nó.
        self.enfileirar(
            session_id,
            Pendencia {
                texto: texto.to_string(),
                papel: PapelMensagem::Usuario,
                origem_node: None,
                // Cada coisa que o usuário pede abre uma cadeia própria: é
                // sobre ela que o orçamento e o limite de saltos incidem.
                trace: Trace::novo(),
                responder: None,
            },
        )
    }

    /// Um nó falando com outro. É a ponte do M3.
    ///
    /// Com `TipoMensagem::Pedido`, **bloqueia** até o destinatário terminar o
    /// turno — quem chamou fica em `aguardando_no` e recebe o texto final dele.
    /// Com `Aviso`, entrega e volta na hora.
    pub fn entregar(
        self: &Arc<Self>,
        de: &Sessao,
        para_node: &str,
        corpo: &str,
        tipo: TipoMensagem,
        prazo_ms: u64,
    ) -> Resultado<Option<String>> {
        let trace_atual = self
            .traces
            .lock()
            .ok()
            .and_then(|t| t.get(&de.id).cloned())
            .unwrap_or_else(Trace::novo);

        // Limite de saltos. É do host, não do agente: um agente convencido de
        // que precisa de mais uma rodada sempre acha um motivo.
        let Some(trace) = trace_atual.saltar() else {
            self.encerrar_cadeia(
                &trace_atual.id,
                &de.node_id,
                &format!("a conversa entre nós passou de {MAX_SALTOS} saltos"),
            );
            return Err(Erro::invalido(format!(
                "esta cadeia já deu {MAX_SALTOS} saltos entre nós e foi encerrada. \
                 Responda com o que você já tem."
            )));
        };

        // Orçamento por cadeia. O pior desfecho de um ciclo malcomportado não é
        // travar — é não travar, e queimar crédito a noite inteira em silêncio.
        let gasto = self.banco()?.custo_do_trace(&trace.id)?;
        if gasto >= ORCAMENTO_POR_TRACE_USD {
            self.encerrar_cadeia(
                &trace.id,
                &de.node_id,
                &format!("a conversa entre nós passou de US$ {ORCAMENTO_POR_TRACE_USD:.2}"),
            );
            return Err(Erro::invalido(
                "esta cadeia já gastou o orçamento dela e foi encerrada. \
                 Responda com o que você já tem."
                    .to_string(),
            ));
        }

        let destino = self.abrir_sessao(para_node, de.adaptador)?;
        if destino.id == de.id {
            return Err(Erro::invalido("um nó não fala consigo mesmo"));
        }

        // Espera cruzada. Não é o mesmo que o ciclo A→B→A do §6, que é
        // legítimo e comum: lá o Redator **termina o turno** e só então o
        // Pesquisador volta a falar. Aqui o destinatário está parado esperando
        // justamente quem está mandando, e nenhum dos dois pode andar.
        //
        // O limite de saltos não pega isto — os saltos só contam quando alguém
        // consegue andar — e o prazo pega tarde demais: dez minutos com dois nós
        // congelados é exatamente o travamento que o M3 promete não ter.
        if tipo == TipoMensagem::Pedido && self.fecharia_ciclo(&de.id, &destino.id) {
            let nome = self.banco()?.obter_no(para_node).map(|n| n.nome).unwrap_or_default();
            return Err(Erro::invalido(format!(
                "o nó \"{nome}\" está parado esperando a SUA resposta — perguntar \
                 de volta agora trava os dois. Responda com o que você já tem."
            )));
        }

        (self.sink)(EventoNucleo::NoMensagem {
            de_node: de.node_id.clone(),
            para_node: para_node.to_string(),
            trace_id: trace.id.clone(),
            tipo_mensagem: tipo,
        });

        let (responder, espera) = match tipo {
            TipoMensagem::Pedido => {
                let (tx, rx) = mpsc::channel();
                (Some(tx), Some(rx))
            }
            TipoMensagem::Aviso => (None, None),
        };

        self.enfileirar(
            &destino.id,
            Pendencia {
                texto: corpo.to_string(),
                papel: PapelMensagem::No,
                origem_node: Some(de.node_id.clone()),
                trace: trace.clone(),
                responder,
            },
        )?;

        let Some(espera) = espera else {
            return Ok(None);
        };

        // Quem perguntou fica explicitamente esperando — sem isso o nó ficaria
        // "pensando" sem ninguém saber por quê.
        {
            let banco = self.banco()?;
            let _ = banco.mudar_estado_sessao(&de.id, EstadoSessao::AguardandoNo);
        }
        if let Ok(mut b) = self.bloqueados.lock() {
            b.insert(de.id.clone(), destino.id.clone());
        }
        self.avisar_estado(&de.id, &de.node_id, EstadoSessao::AguardandoNo);

        let resposta = match espera.recv_timeout(Duration::from_millis(prazo_ms)) {
            Ok(r) => r,
            Err(RecvTimeoutError::Timeout) => Err(Erro::invalido(format!(
                "o nó não respondeu em {} minutos",
                prazo_ms / 60_000
            ))),
            Err(RecvTimeoutError::Disconnected) => {
                Err(Erro::invalido("o nó encerrou sem responder"))
            }
        };

        if let Ok(mut b) = self.bloqueados.lock() {
            b.remove(&de.id);
        }

        {
            let banco = self.banco()?;
            let _ = banco.mudar_estado_sessao(&de.id, EstadoSessao::Pensando);
        }
        self.avisar_estado(&de.id, &de.node_id, EstadoSessao::Pensando);

        resposta.map(Some)
    }

    /// O agente perguntou alguma coisa a quem está na frente da tela.
    ///
    /// Bloqueia até a próxima mensagem que o usuário mandar àquele nó. Prazo
    /// existe apesar do `ESPECIFICACAO.md §6` dizer "sem prazo": uma chamada
    /// pendurada para sempre deixa o processo do agente de pé e o nó travado
    /// sem explicação. Meia hora é tempo de sair para almoçar e voltar.
    pub fn perguntar_humano(
        self: &Arc<Self>,
        sessao: &Sessao,
        pergunta: &str,
        opcoes: &[String],
    ) -> Resultado<String> {
        let texto = if opcoes.is_empty() {
            pergunta.to_string()
        } else {
            format!("{pergunta}\n\n({})", opcoes.join(" · "))
        };

        let (tx, rx) = mpsc::channel();
        // Registra antes de avisar: uma resposta rápida demais chegaria a um
        // mapa vazio e o agente ficaria esperando para sempre.
        if let Ok(mut p) = self.perguntas.lock() {
            p.insert(sessao.id.clone(), tx);
        }

        {
            let banco = self.banco()?;
            banco.gravar_mensagem(&sessao.id, PapelMensagem::Sistema, &texto, Uso::default())?;
            let _ = banco.mudar_estado_sessao(&sessao.id, EstadoSessao::AguardandoHumano);
        }
        self.avisar_estado(&sessao.id, &sessao.node_id, EstadoSessao::AguardandoHumano);

        match rx.recv_timeout(Duration::from_millis(PRAZO_MENSAGEM_TETO_MS)) {
            Ok(r) => Ok(r),
            Err(_) => {
                if let Ok(mut p) = self.perguntas.lock() {
                    p.remove(&sessao.id);
                }
                Err(Erro::invalido("ninguém respondeu a tempo"))
            }
        }
    }

    /// O agente diz que terminou. Vira uma linha na conversa; o turno segue
    /// até o fim normalmente.
    pub fn concluir(&self, sessao: &Sessao, resumo: &str) -> Resultado<()> {
        let banco = self.banco()?;
        banco.gravar_mensagem(
            &sessao.id,
            PapelMensagem::Sistema,
            &format!("Entregue: {resumo}"),
            Uso::default(),
        )?;
        Ok(())
    }

    /// Interrompe o turno atual. Não é erro: o nó volta a ficar ocioso e a
    /// conversa registra que foi o usuário quem parou.
    pub fn cancelar(&self, session_id: &str) -> Resultado<()> {
        if let Some(viva) = self.vivos()?.get_mut(session_id) {
            viva.turno_vivo.store(false, Ordering::SeqCst);
            viva.adaptador.cancelar();
        }

        // Um card aberto segura uma requisição HTTP e, com ela, o processo do
        // agente. Cancelar sem fechá-lo deixaria os dois pendurados até o
        // prazo de meia hora — e o usuário achando que tinha parado.
        self.negar_pendentes(session_id);
        // Idem para quem estava esperando a resposta deste nó, e para a fila:
        // cancelar é cancelar o que estava por vir, não só o que estava em curso.
        self.responder_espera(session_id, Err(Erro::invalido("o turno foi interrompido")));
        // E se era ESTE nó que estava esperando outro, solta a espera dele
        // também. Sem isto o `enviar_para` continuaria pendurado até o prazo,
        // segurando a requisição HTTP de um agente que já foi morto.
        let esperado = self.bloqueados.lock().ok().and_then(|b| b.get(session_id).cloned());
        if let Some(outro) = esperado {
            self.responder_espera(&outro, Err(Erro::invalido("o turno foi interrompido")));
        }
        if let Ok(mut filas) = self.filas.lock() {
            filas.remove(session_id);
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
            // De `aguardando_no` não se vai direto para ocioso na tabela do §6;
            // passar por erro seria mentir sobre o que houve. Força o estado.
            banco.forcar_estado_sessao(session_id, EstadoSessao::Ocioso)?;
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

    // -------------------------------------------------------------- fila

    fn enfileirar(self: &Arc<Self>, session_id: &str, pendencia: Pendencia) -> Resultado<()> {
        {
            let mut filas = self
                .filas
                .lock()
                .map_err(|_| Erro::invalido("as filas ficaram num estado ruim"))?;
            filas.entry(session_id.to_string()).or_default().push_back(pendencia);
        }
        self.puxar_da_fila(session_id);
        Ok(())
    }

    /// Começa o próximo da fila, se o nó estiver livre.
    ///
    /// A leitura do estado e a mudança para `pensando` acontecem sob a mesma
    /// trava do banco: é esse par que serializa duas threads tentando começar
    /// um turno ao mesmo tempo.
    fn puxar_da_fila(self: &Arc<Self>, session_id: &str) {
        let sessao = {
            let banco = match self.banco() {
                Ok(b) => b,
                Err(_) => return,
            };
            let sessao = match banco.obter_sessao(session_id) {
                Ok(s) => s,
                Err(_) => return,
            };
            if !sessao.estado.aceita_turno() {
                return; // ocupado: quem terminar o turno atual puxa o próximo
            }
            sessao
        };

        let pendencia = {
            let mut filas = match self.filas.lock() {
                Ok(f) => f,
                Err(_) => return,
            };
            match filas.get_mut(session_id).and_then(|f| f.pop_front()) {
                Some(p) => p,
                None => return,
            }
        };

        if let Err(e) = self.iniciar_turno(&sessao, pendencia) {
            eprintln!("[mutirao] não consegui começar o turno de {session_id}: {e}");
        }
    }

    fn iniciar_turno(self: &Arc<Self>, sessao: &Sessao, pendencia: Pendencia) -> Resultado<()> {
        let Pendencia { texto, papel, origem_node, trace, responder } = pendencia;

        {
            let banco = self.banco()?;
            banco.gravar_mensagem_completa(
                &sessao.id,
                papel,
                &texto,
                Uso::default(),
                Some(&trace.id),
                origem_node.as_deref(),
            )?;
            // Um nó em `erro` aceita turno novo, mas a tabela do §6 só o deixa
            // sair por `ocioso`. Mandar mensagem já é reconhecer o erro — vale
            // para o usuário e vale para o recado que veio de outro nó.
            if banco.obter_sessao(&sessao.id)?.estado == EstadoSessao::Erro {
                banco.mudar_estado_sessao(&sessao.id, EstadoSessao::Ocioso)?;
            }
            banco.mudar_estado_sessao(&sessao.id, EstadoSessao::Pensando)?;
        }

        if let Ok(mut t) = self.traces.lock() {
            t.insert(sessao.id.clone(), trace);
        }
        if let Some(r) = responder {
            if let Ok(mut e) = self.esperando.lock() {
                e.insert(sessao.id.clone(), r);
            }
        }

        self.avisar_estado(&sessao.id, &sessao.node_id, EstadoSessao::Pensando);

        let turno_vivo = Arc::new(AtomicBool::new(true));
        let receptor = {
            let mut vivos = self.vivos()?;
            if !vivos.contains_key(&sessao.id) {
                let ctx = self.contexto(sessao)?;
                let adaptador = self.fabrica.criar(sessao.adaptador, &ctx)?;
                vivos.insert(
                    sessao.id.clone(),
                    SessaoViva { adaptador, turno_vivo: turno_vivo.clone() },
                );
            }
            let viva = vivos.get_mut(&sessao.id).expect("acabou de ser inserida");
            viva.turno_vivo = turno_vivo.clone();
            viva.adaptador.turno(&texto)?
        };

        let orq = self.clone();
        let id = sessao.id.clone();
        let node_id = sessao.node_id.clone();
        std::thread::spawn(move || {
            bombear(orq, id, node_id, receptor, turno_vivo);
        });

        Ok(())
    }

    /// Mandar de `de` para `destino` fecharia uma espera circular?
    ///
    /// Segue a corrente de quem-espera-quem a partir do destinatário. Se ela
    /// chegar de volta a quem está mandando, os dois (ou os cinco) ficariam
    /// parados um pelo outro até o prazo estourar.
    fn fecharia_ciclo(&self, de: &str, destino: &str) -> bool {
        let Ok(bloqueados) = self.bloqueados.lock() else {
            // Sem conseguir olhar o mapa, recusar é o desfecho seguro: o custo
            // de um "não" injusto é uma frase para o modelo; o de um "sim"
            // errado é o app travado.
            return true;
        };
        let mut atual = destino;
        // A corrente não pode ser maior que o número de sessões bloqueadas.
        // O contador é só cinto de segurança contra um mapa inconsistente.
        for _ in 0..=bloqueados.len() {
            match bloqueados.get(atual) {
                Some(proximo) if proximo == de => return true,
                Some(proximo) => atual = proximo,
                None => return false,
            }
        }
        true
    }

    /// Entrega o resultado do turno a quem estava esperando, e só a ele.
    fn responder_espera(&self, session_id: &str, resultado: Resultado<String>) {
        let canal = self.esperando.lock().ok().and_then(|mut e| e.remove(session_id));
        if let Some(tx) = canal {
            let _ = tx.send(resultado);
        }
    }

    fn encerrar_cadeia(&self, trace_id: &str, node_id: &str, motivo: &str) {
        // Nunca em silêncio: o §6 é explícito em que estourar um limite avisa
        // o usuário em vez de queimar crédito calado.
        (self.sink)(EventoNucleo::CadeiaEncerrada {
            trace_id: trace_id.to_string(),
            node_id: node_id.to_string(),
            motivo: motivo.to_string(),
        });
    }

    /// Fecha como negados os cards abertos de uma sessão. Sem barulho: quem
    /// cancelou o turno já sabe o que fez.
    fn negar_pendentes(&self, session_id: &str) {
        let pendentes: Vec<String> = match self.banco() {
            Ok(banco) => banco
                .ferramentas_da_sessao(session_id)
                .unwrap_or_default()
                .into_iter()
                .filter(|c| c.aprovacao == Aprovacao::Pendente)
                .map(|c| c.id)
                .collect(),
            Err(_) => return,
        };
        for id in pendentes {
            if let Ok(banco) = self.banco() {
                let _ = banco.decidir_ferramenta(&id, Decisao::Negada, "turno cancelado");
            }
            self.aprovacoes.responder(&id, Decisao::Negada);
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
            url_barramento: self.url_barramento.lock().ok().and_then(|u| u.clone()),
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
    orq: Arc<Orquestrador>,
    session_id: String,
    node_id: String,
    receptor: std::sync::mpsc::Receiver<EventoAgente>,
    turno_vivo: Arc<AtomicBool>,
) {
    let banco = orq.banco.clone();
    let sink = orq.sink.clone();
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

        if let Err(e) = aplicar(&banco, &sink, &orq, &session_id, &node_id, &evento, &mut acumulado)
        {
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
            let _ = b.forcar_estado_sessao(&session_id, EstadoSessao::Erro);
        }
        sink(EventoNucleo::SessaoEstado {
            session_id: session_id.clone(),
            node_id: node_id.clone(),
            estado: EstadoSessao::Erro,
            pede_atencao: true,
        });
        orq.responder_espera(&session_id, Err(Erro::invalido(msg)));
    }

    if turno_vivo.load(Ordering::SeqCst) {
        // A cadeia acabou aqui; a próxima começa do zero.
        if let Ok(mut t) = orq.traces.lock() {
            t.remove(&session_id);
        }
        // E quem estava na fila deste nó entra agora. Sem esta linha, uma
        // mensagem enfileirada durante o turno esperaria até alguém falar de
        // novo com o nó — que é como um recado se perde sem dar erro.
        orq.puxar_da_fila(&session_id);
    }
}

#[allow(clippy::too_many_arguments)]
fn aplicar(
    banco: &Arc<Mutex<Banco>>,
    sink: &Sink,
    orq: &Arc<Orquestrador>,
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
            // Passa como automática; se ela precisar de licença, o barramento
            // reescreve a linha para pendente quando o hook chegar. A linha é
            // gravada mesmo assim, porque o log de auditoria começa no primeiro
            // turno, não no marco em que ele fica bonito.
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
            let texto =
                if texto_final.trim().is_empty() { acumulado.as_str() } else { texto_final };
            let trace = orq.traces.lock().ok().and_then(|t| t.get(session_id).cloned());
            banco.gravar_mensagem_completa(
                session_id,
                PapelMensagem::Agente,
                texto,
                *uso,
                trace.as_ref().map(|t| t.id.as_str()),
                None,
            )?;
            banco.somar_custo(session_id, uso.custo_usd)?;
            banco.mudar_estado_sessao(session_id, EstadoSessao::Ocioso)?;

            let no = banco.obter_no(node_id)?;
            let (total, por_no) = banco.custo_do_workspace(&no.workspace_id)?;
            drop(banco);

            // Quem pediu por `enviar_para` recebe agora, antes de a interface
            // saber que o nó ficou ocioso: quem espera esperou mais.
            orq.responder_espera(session_id, Ok(texto.to_string()));

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
            banco.forcar_estado_sessao(session_id, EstadoSessao::Erro)?;
            drop(banco);
            orq.responder_espera(session_id, Err(Erro::invalido(mensagem.clone())));
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
