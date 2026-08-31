//! As ferramentas do barramento — o §6 do `ARQUITETURA.md` virando código.
//!
//! O que um agente enxerga daqui é decidido pelos cabos, e só por eles. Um nó
//! ligado a outro por `fala_com` pode mandar recado; ligado por `le_nota` pode
//! ler aquela nota; sem cabo, o outro nó **não existe**.
//!
//! Essa última palavra é literal e importa: quando o nome não resolve, a
//! resposta é "esse nó não existe", nunca "existe mas você não pode". A
//! segunda forma transforma cada tentativa numa sonda que mapeia o canvas
//! inteiro. `ESPECIFICACAO.md §4`.
//!
//! ## Por que escrever também passa pelo card
//!
//! `escrever_nota` e `escrever_arquivo` gravam no disco de alguém. Se elas
//! escapassem da aprovação do M2, o barramento seria uma porta dos fundos para
//! exatamente o que o card existe para impedir. Medido na CLI 2.1.251: o hook
//! `PreToolUse` **dispara para ferramenta MCP também**, com o nome completo
//! (`mcp__mutirao__escrever_nota`), e negar impede a execução. Por isso
//! [`FERRAMENTAS_QUE_GRAVAM`] entra no matcher do hook — um gate só, todos os
//! caminhos de escrita.

use crate::arquivos;
use crate::db::Banco;
use crate::erro::{Erro, Resultado};
use crate::modelo::*;
use crate::orquestrador::Orquestrador;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

/// O nome do servidor MCP. Vira prefixo do nome que o modelo vê:
/// `mcp__mutirao__enviar_para`.
pub const SERVIDOR: &str = "mutirao";

/// As que gravam no disco. Entram no matcher do hook de aprovação com o nome
/// completo — ver o cabeçalho deste módulo.
pub const FERRAMENTAS_QUE_GRAVAM: &[&str] = &["escrever_nota", "escrever_arquivo"];

pub fn nome_completo(ferramenta: &str) -> String {
    format!("mcp__{SERVIDOR}__{ferramenta}")
}

/// O catálogo que o `tools/list` devolve.
///
/// As descrições são escritas para o modelo, não para o log: elas são a única
/// instrução que ele recebe sobre como a ponte funciona. Dizer "o nome do nó
/// vizinho, como aparece no canvas" evita metade dos erros de endereçamento.
pub fn catalogo() -> Vec<Value> {
    vec![
        ferramenta(
            "enviar_para",
            "Pergunta a outro nó do canvas e ESPERA a resposta dele. Use quando \
             precisar do trabalho do outro para continuar o seu.",
            json!({
                "type": "object",
                "properties": {
                    "no": { "type": "string", "description": "nome do nó vizinho, como aparece no canvas" },
                    "mensagem": { "type": "string", "description": "o que você precisa dele" },
                    "refs": {
                        "type": "array", "items": { "type": "string" },
                        "description": "notas ou arquivos citados, para ele saber onde olhar",
                    },
                    "prazo_ms": { "type": "integer", "description": "quanto esperar; padrão 600000" },
                },
                "required": ["no", "mensagem"],
                "additionalProperties": false,
            }),
        ),
        ferramenta(
            "avisar",
            "Entrega um recado a outro nó e segue em frente, sem esperar resposta.",
            json!({
                "type": "object",
                "properties": {
                    "no": { "type": "string", "description": "nome do nó vizinho" },
                    "mensagem": { "type": "string" },
                },
                "required": ["no", "mensagem"],
                "additionalProperties": false,
            }),
        ),
        ferramenta(
            "listar_nos",
            "Lista os nós com quem você pode falar e as notas que pode ler ou \
             escrever. Se um nó não está aqui, ele não existe para você.",
            json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        ),
        ferramenta(
            "ler_nota",
            "Lê uma nota compartilhada do canvas.",
            json!({
                "type": "object",
                "properties": { "nota": { "type": "string", "description": "nome do nó de nota" } },
                "required": ["nota"],
                "additionalProperties": false,
            }),
        ),
        ferramenta(
            "escrever_nota",
            "Escreve numa nota compartilhada. O usuário precisa aprovar antes.",
            json!({
                "type": "object",
                "properties": {
                    "nota": { "type": "string" },
                    "conteudo": { "type": "string" },
                    "modo": { "type": "string", "enum": ["substituir", "acrescentar"] },
                },
                "required": ["nota", "conteudo"],
                "additionalProperties": false,
            }),
        ),
        ferramenta(
            "listar_arquivos",
            "Lista a pasta do workspace.",
            json!({
                "type": "object",
                "properties": { "caminho": { "type": "string", "description": "subpasta; vazio é a raiz" } },
                "additionalProperties": false,
            }),
        ),
        ferramenta(
            "ler_arquivo",
            "Lê um arquivo de texto da pasta do workspace.",
            json!({
                "type": "object",
                "properties": { "caminho": { "type": "string" } },
                "required": ["caminho"],
                "additionalProperties": false,
            }),
        ),
        ferramenta(
            "escrever_arquivo",
            "Grava um arquivo de texto na pasta do workspace. O usuário precisa \
             aprovar antes.",
            json!({
                "type": "object",
                "properties": {
                    "caminho": { "type": "string" },
                    "conteudo": { "type": "string" },
                },
                "required": ["caminho", "conteudo"],
                "additionalProperties": false,
            }),
        ),
        ferramenta(
            "perguntar_humano",
            "Pergunta ao usuário e espera. Use quando faltar uma decisão que não \
             é sua — não invente a resposta.",
            json!({
                "type": "object",
                "properties": {
                    "pergunta": { "type": "string" },
                    "opcoes": { "type": "array", "items": { "type": "string" } },
                },
                "required": ["pergunta"],
                "additionalProperties": false,
            }),
        ),
        ferramenta(
            "concluir",
            "Marca sua tarefa como entregue, com um resumo do que foi feito.",
            json!({
                "type": "object",
                "properties": { "resumo": { "type": "string" } },
                "required": ["resumo"],
                "additionalProperties": false,
            }),
        ),
    ]
}

fn ferramenta(nome: &str, descricao: &str, esquema: Value) -> Value {
    json!({ "name": nome, "description": descricao, "inputSchema": esquema })
}

// ---------------------------------------------------------------- execução

/// Roda uma ferramenta em nome de uma sessão. Devolve o texto que o modelo vê.
///
/// Bloqueia em `enviar_para` e `perguntar_humano`; quem chama precisa estar
/// numa thread que pode esperar.
pub fn executar(
    orq: &Arc<Orquestrador>,
    banco: &Arc<Mutex<Banco>>,
    sessao: &Sessao,
    nome: &str,
    args: &Value,
) -> Resultado<Value> {
    let (node_id, workspace_id, pasta) = {
        let b = trava(banco)?;
        let no = b.obter_no(&sessao.node_id)?;
        let ws = b.obter_workspace(&no.workspace_id)?;
        (no.id, ws.id, ws.pasta)
    };
    let pasta = std::path::PathBuf::from(pasta);

    match nome {
        "enviar_para" | "avisar" => {
            let espera = nome == "enviar_para";
            let alvo = texto(args, "no")?;
            let mensagem = texto(args, "mensagem")?;
            let destino = vizinho_por_nome(banco, &node_id, TipoCabo::FalaCom, &alvo)?;

            let corpo = match args.get("refs").and_then(Value::as_array) {
                Some(refs) if !refs.is_empty() => {
                    let lista: Vec<&str> = refs.iter().filter_map(Value::as_str).collect();
                    format!("{mensagem}\n\n(citado: {})", lista.join(", "))
                }
                _ => mensagem,
            };

            let prazo = prazo_pedido(args);
            let tipo = if espera { TipoMensagem::Pedido } else { TipoMensagem::Aviso };
            match orq.entregar(sessao, &destino, &corpo, tipo, prazo)? {
                Some(resposta) => Ok(json!({ "resposta": resposta, "de": alvo })),
                None => Ok(json!({ "entregue": true })),
            }
        }

        "listar_nos" => {
            let b = trava(banco)?;
            let mut visiveis = Vec::new();
            for (tipo, rotulo) in [
                (TipoCabo::FalaCom, "posso falar"),
                (TipoCabo::LeNota, "posso ler"),
                (TipoCabo::EscreveNota, "posso escrever"),
            ] {
                for id in b.vizinhos(&node_id, tipo)? {
                    if let Ok(no) = b.obter_no(&id) {
                        visiveis.push(json!({
                            "nome": no.nome,
                            "tipo": no.tipo.como_texto(),
                            "relacao": rotulo,
                        }));
                    }
                }
            }
            Ok(json!({ "nos": visiveis }))
        }

        "ler_nota" => {
            let nome_nota = texto(args, "nota")?;
            let nota = vizinho_por_nome(banco, &node_id, TipoCabo::LeNota, &nome_nota)?;
            let arquivo = {
                let b = trava(banco)?;
                arquivos::arquivo_da_nota_do_no(&b.obter_no(&nota)?)
            };
            let conteudo = arquivos::ler_texto(&pasta, &arquivo).unwrap_or_default();
            Ok(json!({ "conteudo": conteudo }))
        }

        "escrever_nota" => {
            let nome_nota = texto(args, "nota")?;
            let conteudo = texto(args, "conteudo")?;
            let acrescentar = args.get("modo").and_then(Value::as_str) == Some("acrescentar");
            let nota = vizinho_por_nome(banco, &node_id, TipoCabo::EscreveNota, &nome_nota)?;
            let arquivo = {
                let b = trava(banco)?;
                arquivos::arquivo_da_nota_do_no(&b.obter_no(&nota)?)
            };
            let final_ = if acrescentar {
                let antes = arquivos::ler_texto(&pasta, &arquivo).unwrap_or_default();
                format!("{antes}{conteudo}")
            } else {
                conteudo
            };
            let bytes = arquivos::escrever_texto(&pasta, &arquivo, &final_)?;
            Ok(json!({ "bytes": bytes }))
        }

        "listar_arquivos" => {
            let sub = args.get("caminho").and_then(Value::as_str).unwrap_or("");
            let itens = arquivos::listar(&pasta, sub)?;
            Ok(json!({ "itens": itens }))
        }

        "ler_arquivo" => {
            let caminho = texto(args, "caminho")?;
            let conteudo = arquivos::ler_texto(&pasta, &caminho)?;
            Ok(json!({ "conteudo": conteudo }))
        }

        "escrever_arquivo" => {
            let caminho = texto(args, "caminho")?;
            let conteudo = texto(args, "conteudo")?;
            let bytes = arquivos::escrever_texto(&pasta, &caminho, &conteudo)?;
            Ok(json!({ "bytes": bytes }))
        }

        "perguntar_humano" => {
            let pergunta = texto(args, "pergunta")?;
            let opcoes: Vec<String> = args
                .get("opcoes")
                .and_then(Value::as_array)
                .map(|v| v.iter().filter_map(Value::as_str).map(String::from).collect())
                .unwrap_or_default();
            let resposta = orq.perguntar_humano(sessao, &pergunta, &opcoes)?;
            Ok(json!({ "resposta": resposta }))
        }

        "concluir" => {
            let resumo = texto(args, "resumo")?;
            orq.concluir(sessao, &resumo)?;
            Ok(json!({ "ok": true }))
        }

        _ => {
            let _ = workspace_id;
            Err(Erro::invalido(format!("não existe ferramenta chamada {nome}")))
        }
    }
}

/// Quanto esperar por uma resposta, com o teto aplicado.
///
/// O teto existe porque sem ele um agente pediria um prazo de dias e prenderia
/// o nó — o pior desfecho possível, porque um nó preso não pede atenção e não
/// explica nada. Função à parte para o teto ser testável sem esperar meia hora.
pub fn prazo_pedido(args: &Value) -> u64 {
    args.get("prazo_ms")
        .and_then(Value::as_u64)
        .filter(|p| *p > 0)
        .unwrap_or(PRAZO_MENSAGEM_PADRAO_MS)
        .min(PRAZO_MENSAGEM_TETO_MS)
}

/// Resolve o nome de um vizinho dentro do que os cabos deixam ver.
///
/// Nome que não resolve dá `nao_encontrado` com a mesma frase, esteja o nó
/// desligado ou inexistente. Duas mensagens diferentes fariam de cada tentativa
/// uma sonda: "esse nó não existe" versus "existe mas você não pode" revela o
/// canvas inteiro para quem insistir.
fn vizinho_por_nome(
    banco: &Arc<Mutex<Banco>>,
    node_id: &str,
    tipo: TipoCabo,
    nome: &str,
) -> Resultado<String> {
    let procurado = nome.trim().to_lowercase();
    let b = trava(banco)?;
    let mut achados: Vec<String> = Vec::new();
    for id in b.vizinhos(node_id, tipo)? {
        if let Ok(no) = b.obter_no(&id) {
            if no.nome.trim().to_lowercase() == procurado {
                achados.push(id);
            }
        }
    }
    match achados.len() {
        1 => Ok(achados.remove(0)),
        0 => Err(Erro::nao_encontrado("nó", nome)),
        // Ambiguidade é erro explícito, não um chute: escolher um dos dois
        // mandaria o recado para o lugar errado sem ninguém perceber.
        _ => Err(Erro::invalido(format!(
            "há mais de um nó chamado \"{nome}\" ligado a você; renomeie um deles"
        ))),
    }
}

fn texto(args: &Value, campo: &str) -> Resultado<String> {
    args.get(campo)
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| Erro::invalido(format!("falta o campo \"{campo}\"")))
}

fn trava(banco: &Arc<Mutex<Banco>>) -> Resultado<std::sync::MutexGuard<'_, Banco>> {
    banco.lock().map_err(|_| Erro::invalido("o banco ficou num estado ruim"))
}
