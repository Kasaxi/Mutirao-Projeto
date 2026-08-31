//! O servidor MCP do Mutirão: a porta por onde o agente usa as ferramentas
//! do `ARQUITETURA.md §6`.
//!
//! Mora dentro do barramento, no caminho `/mcp`, e fala JSON-RPC 2.0 por HTTP.
//! Não é servidor separado de propósito: **é o mesmo processo, a mesma porta e
//! o mesmo token** do hook de aprovação. Um segundo canal significaria um
//! segundo escopo para manter em dia, e escopo que se mantém em dois lugares
//! cedo ou tarde diverge.
//!
//! ## O que foi medido, e não deduzido
//!
//! Contra a CLI 2.1.251, com `--mcp-config` apontando para um servidor de
//! sonda que registrava tudo:
//!
//! - A conversa é, nesta ordem: `server/discover`, `initialize`,
//!   `notifications/initialized`, `tools/list`, `tools/call`.
//! - `server/discover` **vem antes do handshake** e não é do MCP; é sondagem
//!   da própria CLI. O `id` dela é a *string* `"server-discover-probe-1"`, e
//!   não um número — daí o `id` andar por aqui como `Value`, sem conversão.
//! - O `initialize` pede `protocolVersion` `"2025-11-25"`. Devolver a versão
//!   que ele pediu fecha o handshake; o servidor apareceu no `system/init`
//!   como `{"name":"mutirao","status":"connected"}`.
//! - Os cabeçalhos que configuramos chegam intactos, mas **em minúsculas**:
//!   o token aparece como `x-mutirao-token`. Comparar sem normalizar caixa é o
//!   tipo de bug que só aparece na máquina de outra pessoa.
//! - No `tools/call` o nome chega **sem** o prefixo `mcp__mutirao__` — o
//!   prefixo existe só do lado do cliente, onde o modelo o vê. Mesmo assim o
//!   aceitamos com prefixo: custa uma linha e evita um erro incompreensível.
//! - O `params._meta` traz `claudecode/toolUseId`, que é o **mesmo**
//!   `tool_use_id` do hook e do stream. É o que amarra a chamada ao card de
//!   aprovação e ao card de ação, se um dia precisarmos costurar os três.
//! - O hook `PreToolUse` **dispara para ferramenta MCP também**: negado, o
//!   modelo recebe `is_error: true` com o motivo, o `result` traz a chamada em
//!   `permission_denials` — e o `tools/call` **nunca chega aqui**. É a prova
//!   inteira do §8 num teste só: a gravação negada não é desfeita, ela não
//!   acontece. Por isso `escrever_nota` e `escrever_arquivo` gravam direto
//!   quando chegam — a licença já foi dada. Ver
//!   `ferramentas::FERRAMENTAS_QUE_GRAVAM`.

use crate::db::Banco;
use crate::ferramentas;
use crate::orquestrador::Orquestrador;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

/// Versão do protocolo que respondemos quando o cliente não pede uma.
/// Quando ele pede, devolvemos a dele — é o que a especificação do MCP manda
/// para versão que sabemos falar, e evita uma negociação que não temos por que
/// perder.
pub const VERSAO_PROTOCOLO: &str = "2025-06-18";

/// O que o barramento deve devolver por HTTP.
pub struct Resposta {
    pub codigo: u16,
    /// Vazio quando não há corpo — é o caso das notificações.
    pub corpo: String,
}

impl Resposta {
    fn ok(corpo: Value) -> Resposta {
        Resposta { codigo: 200, corpo: corpo.to_string() }
    }

    /// Notificação: o JSON-RPC não prevê resposta, e o transporte HTTP do MCP
    /// pede 202 com corpo vazio.
    fn aceito() -> Resposta {
        Resposta { codigo: 202, corpo: String::new() }
    }

    /// Token que não resolve não recebe explicação. Ver `ESPECIFICACAO.md §4`.
    fn proibido() -> Resposta {
        Resposta { codigo: 403, corpo: "{}".to_string() }
    }
}

/// Atende uma chamada JSON-RPC. Bloqueia em `enviar_para` e
/// `perguntar_humano` — quem chama precisa estar numa thread que pode esperar
/// meia hora.
pub fn tratar(
    orq: &Arc<Orquestrador>,
    banco: &Arc<Mutex<Banco>>,
    token: &str,
    corpo: &str,
) -> Resposta {
    if token.trim().is_empty() {
        return Resposta::proibido();
    }
    let Ok(pedido) = serde_json::from_str::<Value>(corpo) else {
        return Resposta::ok(erro_rpc(Value::Null, -32700, "corpo não é json"));
    };

    // Sem `id` é notificação: reconhece e não responde. Responder a uma
    // notificação é erro de protocolo, e alguns clientes fecham a conexão.
    let Some(id) = pedido.get("id").cloned() else {
        return Resposta::aceito();
    };

    let metodo = pedido.get("method").and_then(Value::as_str).unwrap_or("");
    let params = pedido.get("params").cloned().unwrap_or_else(|| json!({}));

    // O token é o escopo inteiro: token → sessão → nó → workspace. Resolver
    // antes de olhar o método é de propósito — nem a lista de ferramentas sai
    // para quem não se identificou.
    let sessao = match banco.lock().ok().and_then(|b| b.sessao_por_token(token).ok()) {
        Some(s) => s,
        None => return Resposta::proibido(),
    };

    match metodo {
        // `server/discover` não é do MCP; é uma sondagem que alguns clientes
        // fazem antes do handshake. Responder o mesmo que o `initialize` custa
        // nada e evita um erro no log de quem sonda.
        "initialize" | "server/discover" => {
            let versao = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(VERSAO_PROTOCOLO);
            Resposta::ok(resultado(
                id,
                json!({
                    "protocolVersion": versao,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": {
                        "name": ferramentas::SERVIDOR,
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                }),
            ))
        }

        "ping" => Resposta::ok(resultado(id, json!({}))),

        "tools/list" => Resposta::ok(resultado(id, json!({ "tools": ferramentas::catalogo() }))),

        "tools/call" => {
            let nome = params.get("name").and_then(Value::as_str).unwrap_or("");
            let nome = sem_prefixo(nome);
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

            // Falha de ferramenta volta como resultado com `isError`, não como
            // erro de JSON-RPC. A diferença importa: erro de protocolo o
            // cliente engole, resultado com erro o **modelo lê** — e "esse nó
            // não existe" é exatamente o que ele precisa ler para corrigir o
            // rumo sozinho.
            let (texto, falhou) = match ferramentas::executar(orq, banco, &sessao, nome, &args) {
                Ok(v) => (como_texto(&v), false),
                Err(e) => (e.to_string(), true),
            };
            Resposta::ok(resultado(
                id,
                json!({
                    "content": [{ "type": "text", "text": texto }],
                    "isError": falhou,
                }),
            ))
        }

        outro => Resposta::ok(erro_rpc(id, -32601, &format!("método desconhecido: {outro}"))),
    }
}

/// O nome como o servidor o conhece. O cliente manda sem prefixo; aceitamos com
/// ele porque um modelo que repete o nome que viu não merece um erro por isso.
fn sem_prefixo(nome: &str) -> &str {
    nome.strip_prefix(&format!("mcp__{}__", ferramentas::SERVIDOR)).unwrap_or(nome)
}

/// O resultado de uma ferramenta como texto para o modelo.
///
/// Uma regra só: objeto com um `conteudo` de texto vira o texto cru — é o caso
/// de `ler_nota` e `ler_arquivo`, onde o payload **é** o documento e embrulhá-lo
/// em JSON só gastaria token com aspas escapadas. Todo o resto vai como JSON
/// compacto, que é previsível e o modelo lê bem.
fn como_texto(v: &Value) -> String {
    match v.get("conteudo").and_then(Value::as_str) {
        Some(t) if v.as_object().map(|o| o.len()) == Some(1) => t.to_string(),
        _ => v.to_string(),
    }
}

fn resultado(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn erro_rpc(id: Value, codigo: i64, mensagem: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": codigo, "message": mensagem } })
}
