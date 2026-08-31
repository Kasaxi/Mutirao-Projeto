//! Barramento: o servidor local por onde o agente pede licença.
//!
//! O `ARQUITETURA.md` chama esta camada de barramento e a imaginava como um
//! servidor MCP. No M2 ela nasce com um endpoint só — o de aprovação — porque
//! é o que o marco pede. As ferramentas de trabalho (notas, arquivos, falar
//! com outro nó) entram aqui depois, pela mesma porta e com o mesmo token.
//!
//! ## Como o agente chega até aqui
//!
//! O Claude Code aceita um **hook `PreToolUse` do tipo `http`**: antes de rodar
//! uma ferramenta, ele faz POST do pedido para uma URL e a resposta decide se
//! a ferramenta roda. Medido na CLI 2.1.251: o pedido traz `tool_name`,
//! `tool_input` (com o conteúdo inteiro que seria gravado) e `tool_use_id`, os
//! cabeçalhos que configuramos chegam intactos, e **a CLI espera** — segurar a
//! resposta por oito segundos fez o agente esperar oito segundos.
//!
//! É essa espera que torna o card de aprovação honesto: o arquivo não é
//! gravado e depois desfeito, ele não chega a ser gravado.
//!
//! ## Escopo
//!
//! Só escuta em `127.0.0.1`, e cada pedido precisa do token da sessão
//! (`ESPECIFICACAO.md §4`). O token resolve para uma sessão, que resolve para
//! um nó, que resolve para um workspace. Token que não resolve não recebe
//! explicação — recebe 403 e nada mais.

use crate::db::Banco;
use crate::erro::{Erro, Resultado};
use crate::modelo::*;
use crate::orquestrador::Sink;
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Quanto tempo o agente fica parado esperando alguém clicar.
///
/// O `ESPECIFICACAO.md §6` diz que `perguntar_humano` bloqueia sem prazo. Uma
/// requisição HTTP pendurada para sempre é outra história: o processo do agente
/// fica de pé, o nó fica travado e ninguém sabe por quê. Meia hora é tempo de
/// ir tomar um café e voltar; passou disso, nega e diz que ninguém respondeu.
pub const PRAZO_APROVACAO: Duration = Duration::from_secs(1800);

/// O cabeçalho onde o token viaja. Mesmo nome do `ESPECIFICACAO.md §4`.
pub const CABECALHO_TOKEN: &str = "X-Mutirao-Token";

/// Ferramentas que exigem licença. Ler não pede; escrever e rodar comando,
/// sempre — é o "Padrão" do `ARQUITETURA.md §8`.
pub const FERRAMENTAS_QUE_PEDEM_LICENCA: &[&str] =
    &["Write", "Edit", "NotebookEdit", "Bash", "WebFetch"];

/// Destas o usuário pode dizer "não perguntar de novo nesta pasta". As outras
/// perguntam sempre: liberar `Bash` de uma vez por todas seria entregar a
/// máquina num clique, e ninguém lembra desse clique uma semana depois.
pub const FERRAMENTAS_QUE_ACEITAM_REGRA: &[&str] = &["Write", "Edit", "NotebookEdit"];

// ------------------------------------------------------------- pendências

/// Os pedidos parados esperando gente. Vive fora do banco porque o que está
/// guardado aqui é um canal, não um dado: o banco sabe que há uma pendência,
/// este mapa sabe para onde mandar a resposta.
#[derive(Default)]
pub struct Aprovacoes {
    pendentes: Mutex<HashMap<String, Sender<Decisao>>>,
}

impl Aprovacoes {
    pub fn nova() -> Arc<Aprovacoes> {
        Arc::new(Aprovacoes::default())
    }

    fn registrar(&self, tool_call_id: &str) -> Receiver<Decisao> {
        let (tx, rx) = mpsc::channel();
        if let Ok(mut p) = self.pendentes.lock() {
            p.insert(tool_call_id.to_string(), tx);
        }
        rx
    }

    fn esquecer(&self, tool_call_id: &str) {
        if let Ok(mut p) = self.pendentes.lock() {
            p.remove(tool_call_id);
        }
    }

    /// Entrega a decisão a quem está esperando. `false` quer dizer que ninguém
    /// esperava — o pedido expirou, ou já foi decidido, ou nunca existiu.
    pub fn responder(&self, tool_call_id: &str, decisao: Decisao) -> bool {
        let canal = self.pendentes.lock().ok().and_then(|mut p| p.remove(tool_call_id));
        match canal {
            Some(tx) => tx.send(decisao).is_ok(),
            None => false,
        }
    }

    pub fn quantas_esperando(&self) -> usize {
        self.pendentes.lock().map(|p| p.len()).unwrap_or(0)
    }
}

// --------------------------------------------------------------- veredito

/// O que o hook do Claude Code perguntou.
#[derive(Debug, Clone)]
pub struct PedidoDoHook {
    pub ferramenta: String,
    pub argumentos: serde_json::Value,
    pub id_externo: String,
}

impl PedidoDoHook {
    /// Lê o corpo que a CLI manda. Os nomes de campo são os medidos na 2.1.251.
    pub fn do_json(v: &serde_json::Value) -> Resultado<PedidoDoHook> {
        let ferramenta = v
            .get("tool_name")
            .and_then(|x| x.as_str())
            .ok_or_else(|| Erro::invalido("pedido sem tool_name"))?
            .to_string();
        Ok(PedidoDoHook {
            ferramenta,
            argumentos: v.get("tool_input").cloned().unwrap_or_else(|| serde_json::json!({})),
            // Sem id o pedido ainda dá para decidir; só não dá para casar com
            // o evento do stream. Melhor um id sintético que uma recusa.
            id_externo: v
                .get("tool_use_id")
                .and_then(|x| x.as_str())
                .map(String::from)
                .unwrap_or_else(|| format!("hook_{}", &novo_id()[..8])),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Veredito {
    pub permitir: bool,
    pub motivo: String,
}

impl Veredito {
    fn permitir(motivo: impl Into<String>) -> Veredito {
        Veredito { permitir: true, motivo: motivo.into() }
    }
    fn negar(motivo: impl Into<String>) -> Veredito {
        Veredito { permitir: false, motivo: motivo.into() }
    }

    /// A resposta que o Claude Code espera. Formato medido, não deduzido.
    pub fn como_json(&self) -> serde_json::Value {
        serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": if self.permitir { "allow" } else { "deny" },
                "permissionDecisionReason": self.motivo,
            }
        })
    }
}

/// Decide um pedido de ferramenta. **Não conhece HTTP** — é aqui que mora a
/// regra, e é por isso que dá para testá-la sem subir servidor nenhum.
///
/// Bloqueia enquanto o humano não decide. Quem chama precisa estar numa thread
/// que pode esperar meia hora.
pub fn avaliar(
    banco: &Arc<Mutex<Banco>>,
    aprovacoes: &Arc<Aprovacoes>,
    sink: &Sink,
    token: &str,
    pedido: PedidoDoHook,
    prazo: Duration,
) -> Resultado<Veredito> {
    let (sessao, node_id, workspace_id) = {
        let b = trava(banco)?;
        let sessao = b.sessao_por_token(token)?;
        let no = b.obter_no(&sessao.node_id)?;
        (sessao.clone(), sessao.node_id.clone(), no.workspace_id)
    };

    // Ler não pede licença. Sem isto o agente pararia a cada arquivo aberto e
    // o card viraria ruído — e um card que vira ruído é um card que o usuário
    // aprova sem ler.
    if !FERRAMENTAS_QUE_PEDEM_LICENCA.contains(&pedido.ferramenta.as_str()) {
        return Ok(Veredito::permitir("Leitura não precisa de aprovação."));
    }

    let tool_call_id = format!("{}:{}", sessao.id, pedido.id_externo);

    // Já concedido antes com "não perguntar de novo"?
    //
    // A consulta é uma instrução separada de propósito. Escrita como
    // `if let Some(r) = trava(banco)?.regra_para(...)`, o guard temporário do
    // Mutex vive o bloco inteiro (regra de temporários do Rust 2021) e o
    // `trava(banco)` de dentro trava contra ele mesmo. O sintoma é a suíte
    // pendurada sem mensagem nenhuma.
    let regra = trava(banco)?.regra_para(&workspace_id, &pedido.ferramenta)?;
    if let Some(regra) = regra {
        let b = trava(banco)?;
        b.gravar_ferramenta_pendente(
            &sessao.id,
            &pedido.id_externo,
            &pedido.ferramenta,
            &pedido.argumentos,
        )?;
        b.decidir_ferramenta(&tool_call_id, Decisao::Aprovada, &regra.assinatura())?;
        drop(b);
        sink(EventoNucleo::AprovacaoDecidida {
            tool_call_id,
            node_id,
            decisao: Decisao::Aprovada,
            decidido_por: regra.assinatura(),
        });
        return Ok(Veredito::permitir("Você já tinha liberado isto nesta pasta."));
    }

    let (resumo, detalhe) = descrever_ferramenta(&pedido.ferramenta, &pedido.argumentos);
    let pedido_ui = PedidoAprovacao {
        tool_call_id: tool_call_id.clone(),
        session_id: sessao.id.clone(),
        node_id: node_id.clone(),
        ferramenta: pedido.ferramenta.clone(),
        resumo,
        detalhe,
        previa: previa_do_conteudo(&pedido.argumentos),
        criado_em: agora(),
    };

    // Registra o canal ANTES de avisar a interface. Ao contrário, uma decisão
    // muito rápida — regra de teclado, clique afobado — chegaria a um mapa
    // vazio e o agente ficaria esperando para sempre.
    let espera = aprovacoes.registrar(&tool_call_id);

    {
        let b = trava(banco)?;
        b.gravar_ferramenta_pendente(
            &sessao.id,
            &pedido.id_externo,
            &pedido.ferramenta,
            &pedido.argumentos,
        )?;
        // O nó passa a pedir atenção. Se a transição não for legítima (a
        // sessão já saiu do turno), seguir mesmo assim é melhor que derrubar
        // o pedido: o card ainda é a coisa certa a mostrar.
        let _ = b.mudar_estado_sessao(&sessao.id, EstadoSessao::AguardandoAprovacao);
    }
    sink(EventoNucleo::SessaoEstado {
        session_id: sessao.id.clone(),
        node_id: node_id.clone(),
        estado: EstadoSessao::AguardandoAprovacao,
        pede_atencao: true,
    });
    sink(EventoNucleo::AprovacaoPedida { pedido: pedido_ui });

    let decisao = match espera.recv_timeout(prazo) {
        Ok(d) => d,
        Err(RecvTimeoutError::Timeout) => {
            aprovacoes.esquecer(&tool_call_id);
            let b = trava(banco)?;
            let _ = b.decidir_ferramenta(&tool_call_id, Decisao::Negada, "prazo");
            Decisao::Negada
        }
        Err(RecvTimeoutError::Disconnected) => {
            // O outro lado sumiu (app fechando). Negar é o único desfecho
            // seguro: na dúvida, não grava.
            aprovacoes.esquecer(&tool_call_id);
            Decisao::Negada
        }
    };

    // Voltar para "pensando": o turno continua, com ou sem a ferramenta.
    {
        let b = trava(banco)?;
        let _ = b.mudar_estado_sessao(&sessao.id, EstadoSessao::Pensando);
    }
    sink(EventoNucleo::SessaoEstado {
        session_id: sessao.id,
        node_id,
        estado: EstadoSessao::Pensando,
        pede_atencao: false,
    });

    Ok(match decisao {
        Decisao::Aprovada => Veredito::permitir("Aprovado por você no Mutirão."),
        Decisao::Negada => Veredito::negar(
            "Negado no Mutirão. Não tente de novo por outro caminho; \
             explique o que queria fazer e pergunte.",
        ),
    })
}

/// A prévia que aparece no card. Encolhida porque um `.xlsx` inteiro em base64
/// não cabe na tela nem ajuda ninguém a decidir.
fn previa_do_conteudo(argumentos: &serde_json::Value) -> Option<String> {
    const TETO: usize = 600;
    let bruto = argumentos
        .get("content")
        .or_else(|| argumentos.get("new_string"))
        .or_else(|| argumentos.get("command"))?
        .as_str()?;
    if bruto.chars().count() <= TETO {
        return Some(bruto.to_string());
    }
    Some(bruto.chars().take(TETO).collect::<String>() + "\n…")
}

fn trava(banco: &Arc<Mutex<Banco>>) -> Resultado<std::sync::MutexGuard<'_, Banco>> {
    banco.lock().map_err(|_| Erro::invalido("o banco ficou num estado ruim"))
}

// --------------------------------------------------------------- servidor

pub struct Barramento {
    porta: u16,
    aprovacoes: Arc<Aprovacoes>,
}

impl Barramento {
    /// Sobe o servidor numa porta que o sistema escolhe.
    ///
    /// Porta fixa daria conflito entre duas cópias do app e, pior, deixaria
    /// um alvo previsível para qualquer coisa rodando na mesma máquina.
    pub fn subir(
        banco: Arc<Mutex<Banco>>,
        aprovacoes: Arc<Aprovacoes>,
        sink: Sink,
    ) -> Resultado<Barramento> {
        let servidor = tiny_http::Server::http("127.0.0.1:0")
            .map_err(|e| Erro::invalido(format!("não consegui subir o barramento: {e}")))?;
        let porta = servidor
            .server_addr()
            .to_ip()
            .ok_or_else(|| Erro::invalido("barramento sem porta"))?
            .port();

        let devolve = Barramento { porta, aprovacoes: aprovacoes.clone() };

        std::thread::spawn(move || {
            for pedido in servidor.incoming_requests() {
                // Uma thread por pedido: cada um pode ficar meia hora parado
                // esperando um clique, e um laço único travaria o próximo.
                let banco = banco.clone();
                let aprovacoes = aprovacoes.clone();
                let sink = sink.clone();
                std::thread::spawn(move || atender(pedido, banco, aprovacoes, sink));
            }
        });

        Ok(devolve)
    }

    pub fn porta(&self) -> u16 {
        self.porta
    }

    pub fn url_de_aprovacao(&self) -> String {
        format!("http://127.0.0.1:{}/aprovacao", self.porta)
    }

    pub fn aprovacoes(&self) -> &Arc<Aprovacoes> {
        &self.aprovacoes
    }
}

fn atender(
    mut pedido: tiny_http::Request,
    banco: Arc<Mutex<Banco>>,
    aprovacoes: Arc<Aprovacoes>,
    sink: Sink,
) {
    let token = pedido
        .headers()
        .iter()
        .find(|h| h.field.equiv(CABECALHO_TOKEN))
        .map(|h| h.value.as_str().to_string())
        .unwrap_or_default();

    let mut corpo = String::new();
    let _ = std::io::Read::read_to_string(pedido.as_reader(), &mut corpo);

    let resposta = tratar(&banco, &aprovacoes, &sink, &token, &corpo, PRAZO_APROVACAO);
    let (codigo, texto) = match resposta {
        Ok(v) => (200, v.como_json().to_string()),
        // 403 sem explicação: quem mandou um token ruim não descobre por aqui
        // se ele existia, nem o que existe do outro lado.
        Err(_) => (403, "{}".to_string()),
    };

    let cabecalho = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("cabeçalho constante");
    let _ = pedido.respond(
        tiny_http::Response::from_string(texto)
            .with_status_code(codigo)
            .with_header(cabecalho),
    );
}

/// A ponte entre o corpo cru e [`avaliar`]. Separada para os testes poderem
/// exercitar o caminho inteiro sem abrir socket.
pub fn tratar(
    banco: &Arc<Mutex<Banco>>,
    aprovacoes: &Arc<Aprovacoes>,
    sink: &Sink,
    token: &str,
    corpo: &str,
    prazo: Duration,
) -> Resultado<Veredito> {
    if token.trim().is_empty() {
        return Err(Erro::invalido("sem token"));
    }
    let v: serde_json::Value =
        serde_json::from_str(corpo).map_err(|_| Erro::invalido("corpo não é json"))?;
    let pedido = PedidoDoHook::do_json(&v)?;
    avaliar(banco, aprovacoes, sink, token, pedido, prazo)
}
