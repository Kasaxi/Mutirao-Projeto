//! Adaptador do Claude Code.
//!
//! Roda a CLI em modo headless (`claude --print --output-format stream-json`)
//! e traduz o JSONL da saída para [`EventoAgente`]. É a Decisão 1 do
//! `ARQUITETURA.md` cumprindo o que prometia: com stream estruturado, o fim do
//! turno é evento explícito — não adivinhação sobre o texto de um terminal.
//!
//! ## Por que a CLI e não um sidecar Node com o Agent SDK
//!
//! O `ARQUITETURA.md §9` escolheu o sidecar com uma justificativa: "entrada em
//! streaming e `canUseTool` não existem na CLI pura". Medido na CLI 2.1.251,
//! isso não se sustenta mais — `--input-format stream-json` existe, e a
//! aprovação de ferramenta sai por `--permission-prompt-tool` apontando para o
//! servidor MCP do próprio app, que a `ESPECIFICACAO.md §4` já projeta.
//!
//! Sem a justificativa, sobra o custo: um runtime Node dentro do instalador do
//! Windows, uma árvore de `node_modules` para manter e mais um processo entre
//! o núcleo e o agente. A CLI dá o mesmo com um `Command::spawn`.
//!
//! E dá uma coisa que o cálculo próprio não daria: **o custo certo**. Ver
//! [`traduzir`].
//!
//! ## Segurança no M1: somente leitura
//!
//! Não existe fluxo de aprovação até o M2, e um agente que escreve sem pedir
//! licença é exatamente o que o `ARQUITETURA.md §8` proíbe. Enquanto o card de
//! aprovação não existe, o turno roda com `--restricted` e um allowlist de
//! leitura. Escrita chega junto com a permissão, não antes dela.

use crate::agente::{AgenteAdapter, ContextoSessao, Fabrica};
use crate::erro::{Erro, Resultado};
use crate::modelo::*;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Prazo de um turno. Bate com o `prazo_ms` padrão do protocolo entre nós
/// (`ESPECIFICACAO.md §5`). Sem ele, uma CLI travada deixa o nó pensando para
/// sempre — o pior desfecho possível, e o que o teste do orquestrador cobre.
pub const PRAZO_PADRAO: Duration = Duration::from_secs(600);

/// Leitura, liberada sem card. Confinada à pasta do workspace pelo
/// `--restricted` mais o diretório de trabalho do processo.
///
/// Escrita **não** está aqui de propósito. A única porta para gravar é o hook
/// de aprovação: se ele deixar de disparar por qualquer motivo, a gravação é
/// recusada em vez de passar batida. Errar para o lado de não gravar é a
/// escolha certa quando o assunto é o disco de outra pessoa.
const FERRAMENTAS_SEM_CARD: &[&str] = &["Read", "Glob", "Grep"];

/// Só leitura, para quando não há barramento no ar.
const FERRAMENTAS_SO_LEITURA: &str = "Read,Glob,Grep";

/// Variáveis de ambiente que o processo do agente **não** herda.
///
/// Achadas medindo: rodando uma sonda de dentro de um Claude Code, a CLI
/// aninhada reportou o `session_id` da sessão de fora. Neste contêiner o id
/// continuou pinado mesmo com as variáveis removidas — então o que se prova
/// aqui é a intenção, não o efeito. Vale assim mesmo: herdar identidade de
/// sessão de quem abriu o app não é comportamento que se queira em nenhum
/// caso, e a limpeza custa uma linha.
const VARIAVEIS_QUE_NAO_HERDAM: &[&str] = &[
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_SESSION_ID",
];

// ------------------------------------------------------------------ tradução

/// O que a tradução precisa lembrar entre uma linha e a seguinte.
#[derive(Debug, Default)]
pub struct Traducao {
    /// `tool_use_id` → nome da ferramenta. O resultado chega numa linha
    /// separada e só traz o id; sem este mapa o card de ação fica sem verbo.
    ferramentas: HashMap<String, String>,
    /// Id de retomada, assim que o `system/init` o anuncia.
    pub id_externo: Option<String>,
    /// Ficou `true` quando o turno terminou com sino (`result`). Se o processo
    /// morrer sem isso, quem chamou sabe que precisa fabricar um erro.
    pub concluiu: bool,
    /// O `result` de erro veio sem texto nenhum.
    ///
    /// Não é hipótese: nos dois erros capturados da CLI 2.1.251 — retomada de
    /// sessão inexistente e estouro de `--max-turns` — o campo `result` nem
    /// existe, e a frase que o usuário precisa ler ("No conversation found with
    /// session ID: …") sai pelo **stderr**. Quem chama usa esta marca para
    /// esperar o stderr antes de mostrar um erro genérico e inútil.
    pub erro_sem_texto: bool,
}

/// Traduz uma linha do JSONL da CLI para zero ou mais eventos do Mutirão.
///
/// Função pura de propósito: é o miolo do adaptador e a única parte que dá
/// para testar sem gastar dinheiro. Os testes rodam contra
/// `nucleo/testes/claude_stream.jsonl`, que é saída **de verdade** capturada da
/// CLI 2.1.251 — não uma invenção do que ela deveria devolver.
///
/// Linha que não interessa vira vetor vazio, nunca erro: a CLI acrescenta tipos
/// de evento a cada versão, e um adaptador que quebra ao ver um evento novo
/// quebraria no dia da atualização.
pub fn traduzir(linha: &str, t: &mut Traducao) -> Vec<EventoAgente> {
    let Ok(o) = serde_json::from_str::<Value>(linha) else {
        return vec![];
    };
    let tipo = o.get("type").and_then(Value::as_str).unwrap_or("");
    let subtipo = o.get("subtype").and_then(Value::as_str).unwrap_or("");

    match (tipo, subtipo) {
        ("system", "init") => {
            let id = texto(&o, "session_id");
            t.id_externo = Some(id.clone());
            vec![EventoAgente::SessaoIniciada {
                id_externo: id,
                modelo: texto(&o, "model"),
                ferramentas: o
                    .get("tools")
                    .and_then(Value::as_array)
                    .map(|v| v.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
            }]
        }

        // A linha de atividade que a interface do Claude Code mostra enquanto
        // trabalha. `detail` nulo é o fim da atividade, não uma atividade vazia.
        ("system", "task_summary") => match o.get("detail").and_then(Value::as_str) {
            Some(d) if !d.trim().is_empty() => {
                vec![EventoAgente::Raciocinando { resumo: d.to_string() }]
            }
            _ => vec![],
        },

        ("stream_event", _) => {
            let delta = o.get("event").and_then(|e| e.get("delta"));
            match delta.and_then(|d| d.get("type")).and_then(Value::as_str) {
                Some("text_delta") => {
                    let texto = delta.and_then(|d| d.get("text")).and_then(Value::as_str);
                    match texto {
                        Some(s) if !s.is_empty() => {
                            vec![EventoAgente::TextoParcial { delta: s.to_string() }]
                        }
                        _ => vec![],
                    }
                }
                // O pensamento vem quase sempre vazio (o texto cru nunca é
                // exposto). Não vira evento: um "Raciocinando" em branco só
                // pisca na tela sem dizer nada. Quem conta o que ele está
                // fazendo é o `task_summary`.
                _ => vec![],
            }
        }

        ("assistant", _) => blocos(&o)
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
            .map(|b| {
                let id = texto(b, "id");
                let nome = texto(b, "name");
                t.ferramentas.insert(id.clone(), nome.clone());
                EventoAgente::FerramentaPedida {
                    id,
                    nome,
                    argumentos: b.get("input").cloned().unwrap_or_else(|| serde_json::json!({})),
                }
            })
            .collect(),

        ("user", _) => blocos(&o)
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
            .map(|b| {
                let id = texto(b, "tool_use_id");
                let conteudo = b.get("content").cloned().unwrap_or(Value::Null);
                // `is_error` ausente quer dizer que deu certo — foi assim que a
                // CLI de verdade devolveu na captura.
                let falhou = b.get("is_error").and_then(Value::as_bool).unwrap_or(false);
                if falhou {
                    EventoAgente::FerramentaConcluida {
                        id,
                        resultado: None,
                        erro: Some(como_texto(&conteudo)),
                    }
                } else {
                    EventoAgente::FerramentaConcluida {
                        id,
                        resultado: Some(resumir(&conteudo)),
                        erro: None,
                    }
                }
            })
            .collect(),

        ("result", _) => {
            t.concluiu = true;
            let deu_erro = o.get("is_error").and_then(Value::as_bool).unwrap_or(false)
                || subtipo != "success";
            let texto_final = texto(&o, "result");
            if deu_erro {
                let mensagem = if texto_final.trim().is_empty() {
                    t.erro_sem_texto = true;
                    format!("O agente terminou com erro ({subtipo}).")
                } else {
                    texto_final
                };
                // Recuperável: o nó volta para `erro`, que aceita turno novo.
                // Mandar outra mensagem é o "tentar de novo".
                return vec![EventoAgente::Erro { mensagem, recuperavel: true }];
            }
            vec![EventoAgente::TurnoConcluido { texto_final, uso: uso_do_resultado(&o) }]
        }

        _ => vec![],
    }
}

/// Consumo e custo de um turno, do jeito que a CLI reporta.
///
/// **O custo vem de `total_cost_usd`, não da nossa tabela de preços.** Não é
/// preguiça: medido num turno real, a tabela de `modelo.rs` diria US$ 0,58 num
/// turno que custou US$ 0,0496. A diferença é cache — leitura de contexto
/// cacheado custa um décimo do preço cheio, e gravação um quarto a mais, e esse
/// turno leu 108 mil tokens de cache. Um painel de custo com erro de 11x é pior
/// que nenhum painel.
///
/// A tabela de preços continua valendo para adaptador que não reporta custo —
/// hoje, o falso.
fn uso_do_resultado(o: &Value) -> Uso {
    let u = o.get("usage").cloned().unwrap_or(Value::Null);
    let n = |chave: &str| u.get(chave).and_then(Value::as_i64).unwrap_or(0);
    Uso {
        // Tudo que entrou conta como entrada, inclusive o que veio do cache:
        // é o tamanho do contexto que o turno leu.
        tokens_entrada: n("input_tokens")
            + n("cache_creation_input_tokens")
            + n("cache_read_input_tokens"),
        tokens_saida: n("output_tokens"),
        custo_usd: o.get("total_cost_usd").and_then(Value::as_f64).unwrap_or(f64::NAN),
    }
}

fn blocos(o: &Value) -> Vec<Value> {
    o.get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn texto(o: &Value, chave: &str) -> String {
    o.get(chave).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Conteúdo de resultado de ferramenta como texto legível. A CLI manda ora uma
/// string, ora um vetor de blocos.
fn como_texto(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(bs) => bs
            .iter()
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        outro => outro.to_string(),
    }
}

/// Resultado de ferramenta encolhido para caber num card.
///
/// O conteúdo bruto pode ser um arquivo inteiro. Guardar isso em
/// `tool_call.resultado_json` para cada leitura infla o banco sem servir a
/// ninguém: o card mostra "leu contrato.docx", não o contrato.
fn resumir(v: &Value) -> Value {
    const TETO: usize = 2_000;
    let t = como_texto(v);
    if t.chars().count() <= TETO {
        return serde_json::json!({ "conteudo": t });
    }
    let corte: String = t.chars().take(TETO).collect();
    serde_json::json!({ "conteudo": corte, "truncado": true, "bytes": t.len() })
}

// ----------------------------------------------------------------- adaptador

pub struct AdaptadorClaude {
    binario: String,
    ctx: ContextoSessao,
    /// Id de retomada. `Arc` porque quem o descobre é a thread que lê o
    /// stdout, no `system/init` do primeiro turno, e quem precisa dele é o
    /// comando do turno seguinte. Sem compartilhar, todo turno começaria uma
    /// conversa nova e o agente não lembraria de nada — o tipo de falha que
    /// não dá erro, só respostas estranhas.
    sessao_externa: Arc<Mutex<Option<String>>>,
    filho: Arc<Mutex<Option<Child>>>,
    cancelado: Arc<AtomicBool>,
    prazo: Duration,
    /// O `--settings` desta sessão, com o hook de aprovação. `None` quando não
    /// há barramento — e aí o adaptador roda somente leitura.
    arquivo_settings: Option<std::path::PathBuf>,
    /// O `--mcp-config` desta sessão, apontando para o servidor MCP do
    /// barramento. Anda junto com o de settings: um sem o outro seria ou uma
    /// ponte sem aprovação, ou uma aprovação sem ponte.
    arquivo_mcp: Option<std::path::PathBuf>,
}

impl AdaptadorClaude {
    pub fn novo(binario: impl Into<String>, ctx: ContextoSessao) -> Resultado<Self> {
        let arquivo_settings = AdaptadorClaude::escrever_settings(&ctx)?;
        let arquivo_mcp = AdaptadorClaude::escrever_mcp(&ctx)?;
        Ok(AdaptadorClaude {
            binario: binario.into(),
            sessao_externa: Arc::new(Mutex::new(ctx.sessao_externa_id.clone())),
            ctx,
            filho: Arc::new(Mutex::new(None)),
            cancelado: Arc::new(AtomicBool::new(false)),
            prazo: PRAZO_PADRAO,
            arquivo_settings,
            arquivo_mcp,
        })
    }

    /// A escrita está liberada nesta sessão? Falso quando não há barramento.
    pub fn pode_escrever(&self) -> bool {
        self.arquivo_settings.is_some()
    }

    /// O id de retomada que este adaptador usará no próximo turno.
    pub fn sessao_externa(&self) -> Option<String> {
        self.sessao_externa.lock().ok().and_then(|g| g.clone())
    }

    pub fn com_prazo(mut self, prazo: Duration) -> Self {
        self.prazo = prazo;
        self
    }

    /// A linha de comando do turno. `pub(crate)` só para o teste poder olhar
    /// se o token vazou para dentro dela — ver
    /// `o_token_da_sessao_nao_aparece_na_linha_de_comando`.
    pub(crate) fn comando(&self, texto: &str) -> Command {
        let mut cmd = Command::new(&self.binario);
        cmd.arg("--print")
            .arg(texto)
            .arg("--output-format")
            .arg("stream-json")
            // Sem isto a resposta só chega inteira no fim, e a face conversa
            // deixa de ser conversa.
            .arg("--include-partial-messages")
            .arg("--verbose")
            // Confina as ferramentas de arquivo ao diretório de trabalho e —
            // o que mais importa — ignora as configurações do usuário e do
            // projeto. Sem isso o comportamento do agente dependeria do que
            // houvesse em `~/.claude` de quem instalou, e o mesmo workspace se
            // comportaria diferente em cada computador.
            .arg("--restricted")
            // `--restricted` remove Bash e as ferramentas que rodam código,
            // "unless --tools names them". Nomeá-las aqui devolve o poder de
            // montar um `.xlsx`, e cada uso passa pelo card.
            .arg("--tools")
            .arg(self.disponiveis())
            .arg("--allowedTools")
            .args(self.sem_card())
            // Só o nosso servidor MCP, nenhum dos que o usuário tenha
            // configurado em `~/.claude`. Pelo mesmo motivo do `--restricted`:
            // o mesmo workspace tem de se comportar igual em cada computador.
            .arg("--strict-mcp-config")
            // A pasta do workspace é o mundo do agente.
            .current_dir(&self.ctx.pasta)
            // stdin fechado: sem isto a CLI espera 3 segundos por dados que
            // nunca vêm, em todo turno. Medido, não suposto.
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // O ambiente de quem abriu o Mutirão não pode virar o ambiente do
        // agente. Se alguém rodar `npm run app` de dentro de um Claude Code —
        // que é exatamente o que se faz ao desenvolver isto — todos os nós
        // herdariam a sessão de fora, e um `--resume` de segundo turno cairia
        // na conversa errada. Quem manda no id de sessão somos nós, pelo
        // `--resume` explícito.
        for herdada in VARIAVEIS_QUE_NAO_HERDAM {
            cmd.env_remove(herdada);
        }

        if let Some(caminho) = &self.arquivo_settings {
            cmd.arg("--settings").arg(caminho);
        }
        if let Some(caminho) = &self.arquivo_mcp {
            cmd.arg("--mcp-config").arg(caminho);
        }

        if let Some(papel) = &self.ctx.papel {
            // Em TODO turno, e não só no primeiro.
            //
            // Medido na 2.1.251: o prompt anexado sobrevive ao `--resume`
            // sozinho — o turno seguinte obedece mesmo sem a flag. Passar de
            // novo é o que faz uma EDIÇÃO do papel valer para a conversa que
            // já existe. Sem isso, mudar o prompt de um papel não teria efeito
            // nenhum sobre os nós que já o usam, e falharia calado.
            cmd.arg("--append-system-prompt").arg(&papel.prompt);
            if let Some(modelo) = &papel.modelo {
                cmd.arg("--model").arg(modelo);
            }
        }

        if let Some(id) = self.sessao_externa() {
            cmd.arg("--resume").arg(id);
        }
        cmd
    }

    /// O que o processo pode enxergar. `--restricted` tira tudo que não estiver
    /// aqui, e isso vale para ferramenta de MCP também — nomeá-las é o que faz
    /// a ponte existir para o modelo.
    ///
    /// Desde o M4 quem decide é o papel. Sem barramento no ar continua sendo
    /// só leitura, seja qual for o papel: escrever sem quem aprove é o que a
    /// §8 proíbe, e um papel não pode revogar isso.
    fn disponiveis(&self) -> String {
        if self.arquivo_settings.is_none() {
            return FERRAMENTAS_SO_LEITURA.to_string();
        }
        let mut v: Vec<String> = crate::papeis::nativas_do_papel(self.ctx.papel.as_ref())
            .iter()
            .map(|s| s.to_string())
            .collect();
        if self.arquivo_mcp.is_some() {
            v.extend(self.ferramentas_mcp());
        }
        v.join(",")
    }

    /// As ferramentas do §6 deste nó, com o prefixo do servidor.
    fn ferramentas_mcp(&self) -> Vec<String> {
        crate::papeis::ferramentas_do_papel(self.ctx.papel.as_ref())
            .iter()
            .map(|f| crate::ferramentas::nome_completo(f))
            .collect()
    }

    /// O que roda sem card. Leitura sempre; e, quando há barramento, as
    /// ferramentas do §6 que **não** gravam — falar com outro nó, listar,
    /// perguntar. Escrita fica de fora de propósito: as duas que gravam estão
    /// no matcher do hook, e é lá que elas param.
    fn sem_card(&self) -> Vec<String> {
        let mut v: Vec<String> = FERRAMENTAS_SEM_CARD.iter().map(|s| s.to_string()).collect();
        if self.arquivo_mcp.is_some() {
            v.extend(
                self.ferramentas_mcp()
                    .into_iter()
                    .filter(|n| !crate::barramento::pede_licenca(n)),
            );
        }
        v
    }

    /// Escreve o `--settings` desta sessão: um hook `PreToolUse` do tipo HTTP
    /// apontando para o barramento, com o token da sessão no cabeçalho.
    ///
    /// Em arquivo, e não como JSON na linha de comando (que `--settings`
    /// também aceita), porque a linha de comando de um processo é legível por
    /// qualquer outro processo do mesmo usuário — e ali dentro vai o token.
    fn escrever_settings(ctx: &ContextoSessao) -> Resultado<Option<std::path::PathBuf>> {
        let Some(url) = ctx.url_de_aprovacao() else {
            return Ok(None);
        };
        let settings = serde_json::json!({
            "hooks": {
                "PreToolUse": [{
                    // Nativas e MCP no mesmo matcher. Medido na CLI 2.1.251: o
                    // hook dispara para `mcp__mutirao__escrever_nota` também, e
                    // negar impede a chamada — é isso que permite um gate só
                    // para todos os caminhos de escrita.
                    "matcher": crate::barramento::matcher_do_hook(),
                    "hooks": [{
                        "type": "http",
                        "url": url,
                        "headers": { crate::barramento::CABECALHO_TOKEN: ctx.token },
                        // Um pouco mais que o prazo do barramento: quem decide
                        // desistir é ele, com uma resposta explicando; a CLI
                        // desistindo antes daria um erro sem sentido para quem
                        // estava lendo o card.
                        "timeout": crate::barramento::PRAZO_APROVACAO.as_secs() + 60,
                        "statusMessage": "esperando você aprovar no Mutirão",
                    }],
                }],
            },
        });

        let caminho =
            std::env::temp_dir().join(format!("mutirao-{}-settings.json", ctx.session_id));
        std::fs::write(&caminho, serde_json::to_vec_pretty(&settings)?)?;
        segredar(&caminho);
        Ok(Some(caminho))
    }

    /// Escreve o `--mcp-config` desta sessão: um servidor HTTP só, o do
    /// barramento, com o token da sessão no cabeçalho.
    ///
    /// Em arquivo pelo mesmo motivo do settings — a linha de comando de um
    /// processo é legível por qualquer outro processo do mesmo usuário, e ali
    /// dentro vai o token.
    fn escrever_mcp(ctx: &ContextoSessao) -> Resultado<Option<std::path::PathBuf>> {
        let Some(url) = ctx.url_do_mcp() else {
            return Ok(None);
        };
        let config = serde_json::json!({
            "mcpServers": {
                crate::ferramentas::SERVIDOR: {
                    "type": "http",
                    "url": url,
                    "headers": { crate::barramento::CABECALHO_TOKEN: ctx.token },
                },
            },
        });

        let caminho = std::env::temp_dir().join(format!("mutirao-{}-mcp.json", ctx.session_id));
        std::fs::write(&caminho, serde_json::to_vec_pretty(&config)?)?;
        segredar(&caminho);
        Ok(Some(caminho))
    }

    /// A CLI está instalada e responde? Chamado no onboarding (M6) e antes do
    /// primeiro turno, para o erro ser "instale o Claude Code" em vez de
    /// "programa não encontrado".
    pub fn detectar(binario: &str) -> Resultado<String> {
        let saida = Command::new(binario)
            .arg("--version")
            .stdin(Stdio::null())
            .output()
            .map_err(|e| {
                Erro::invalido(format!(
                    "não encontrei o Claude Code em `{binario}`: {e}. \
                     Instale-o ou aponte MUTIRAO_CLAUDE_BIN para o executável."
                ))
            })?;
        if !saida.status.success() {
            return Err(Erro::invalido(format!(
                "`{binario} --version` falhou: {}",
                String::from_utf8_lossy(&saida.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&saida.stdout).trim().to_string())
    }
}

impl AgenteAdapter for AdaptadorClaude {
    fn turno(&mut self, texto: &str) -> Resultado<Receiver<EventoAgente>> {
        self.cancelado.store(false, Ordering::SeqCst);

        let mut filho = self.comando(texto).spawn().map_err(|e| {
            Erro::invalido(format!(
                "não consegui rodar o Claude Code (`{}`): {e}",
                self.binario
            ))
        })?;

        let stdout = filho.stdout.take().ok_or_else(|| Erro::invalido("sem stdout"))?;
        let stderr = filho.stderr.take().ok_or_else(|| Erro::invalido("sem stderr"))?;
        *self.filho.lock().map_err(|_| Erro::invalido("processo travado"))? = Some(filho);

        let (tx, rx) = mpsc::channel();

        // O stderr é coletado numa thread própria: se o cano encher e ninguém
        // ler, o processo bloqueia escrevendo nele e o turno nunca termina.
        let erros = Arc::new(Mutex::new(String::new()));
        let coletor = {
            let erros = erros.clone();
            std::thread::spawn(move || {
                for linha in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if let Ok(mut e) = erros.lock() {
                        e.push_str(&linha);
                        e.push('\n');
                    }
                }
            })
        };

        // Cão de guarda: mata o processo se o prazo estourar. Sem ele, uma CLI
        // pendurada deixa o nó "pensando" até alguém reiniciar o app.
        {
            let filho = self.filho.clone();
            let cancelado = self.cancelado.clone();
            let prazo = self.prazo;
            let tx_prazo: Sender<EventoAgente> = tx.clone();
            std::thread::spawn(move || {
                std::thread::sleep(prazo);
                if cancelado.load(Ordering::SeqCst) {
                    return;
                }
                let mut guarda = match filho.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                if let Some(p) = guarda.as_mut() {
                    // `try_wait` diz se ainda está vivo sem bloquear.
                    if matches!(p.try_wait(), Ok(None)) {
                        let _ = p.kill();
                        let _ = tx_prazo.send(EventoAgente::Erro {
                            mensagem: format!(
                                "O agente passou de {} minutos sem terminar o turno.",
                                prazo.as_secs() / 60
                            ),
                            recuperavel: true,
                        });
                    }
                }
            });
        }

        let filho_leitor = self.filho.clone();
        let cancelado = self.cancelado.clone();
        let sessao_externa = self.sessao_externa.clone();
        let mut traducao =
            Traducao { id_externo: self.sessao_externa(), ..Default::default() };

        std::thread::spawn(move || {
            // Um erro sem texto fica esperando aqui. A frase que o usuário
            // precisa ler está no stderr, e o stderr só fecha quando o processo
            // morre — mandar o evento na hora entregaria "erro (subtipo)" e
            // jogaria fora "No conversation found with session ID: …".
            let mut adiado: Option<EventoAgente> = None;

            for linha in BufReader::new(stdout).lines().map_while(Result::ok) {
                if cancelado.load(Ordering::SeqCst) {
                    return;
                }
                for evento in traduzir(&linha, &mut traducao) {
                    // Guarda o id assim que ele aparece: é o que faz o próximo
                    // turno continuar esta conversa em vez de começar outra.
                    if let EventoAgente::SessaoIniciada { id_externo, .. } = &evento {
                        if let Ok(mut g) = sessao_externa.lock() {
                            *g = Some(id_externo.clone());
                        }
                    }
                    if traducao.erro_sem_texto && matches!(evento, EventoAgente::Erro { .. }) {
                        adiado = Some(evento);
                        continue;
                    }
                    if tx.send(evento).is_err() {
                        return; // ninguém escutando
                    }
                }
            }

            // A saída acabou: o processo morreu, então o stderr também fechou.
            let _ = coletor.join();
            let status = filho_leitor
                .lock()
                .ok()
                .and_then(|mut g| g.as_mut().map(|p| p.wait()))
                .and_then(Result::ok);
            // O stderr da CLI é escrito para gente ler. A última linha é a que
            // diz o que houve; passar adiante é mais útil que "algo deu errado".
            let detalhe = erros
                .lock()
                .ok()
                .and_then(|e| e.trim().lines().last().map(str::to_string))
                .unwrap_or_default();

            if cancelado.load(Ordering::SeqCst) {
                return;
            }

            if let Some(EventoAgente::Erro { mensagem, recuperavel }) = adiado {
                let _ = tx.send(EventoAgente::Erro {
                    mensagem: if detalhe.is_empty() { mensagem } else { detalhe },
                    recuperavel,
                });
            } else if !traducao.concluiu {
                // Nem sino, nem erro: o processo morreu por fora. Alguém
                // precisa dizer isso, senão o turno fica pendurado.
                let codigo = status.map(|s| s.to_string()).unwrap_or_else(|| "desconhecido".into());
                let _ = tx.send(EventoAgente::Erro {
                    mensagem: if detalhe.is_empty() {
                        format!("O Claude Code saiu sem terminar o turno ({codigo}).")
                    } else {
                        detalhe
                    },
                    recuperavel: true,
                });
            }
        });

        Ok(rx)
    }

    fn cancelar(&mut self) {
        self.cancelado.store(true, Ordering::SeqCst);
        if let Ok(mut guarda) = self.filho.lock() {
            if let Some(p) = guarda.as_mut() {
                // Matar é o único jeito: a CLI não tem "pare o turno atual".
                // A sessão fica no disco e o próximo turno a retoma.
                let _ = p.kill();
                let _ = p.wait();
            }
        }
    }
}

// ------------------------------------------------------------------- fábrica

pub struct FabricaClaude {
    binario: String,
}

impl FabricaClaude {
    /// `MUTIRAO_CLAUDE_BIN` cobre o caso de a CLI não estar no PATH — comum no
    /// Windows, onde ela costuma virar um `.cmd` numa pasta do npm.
    pub fn nova() -> Self {
        let binario = std::env::var("MUTIRAO_CLAUDE_BIN").unwrap_or_else(|_| "claude".into());
        FabricaClaude { binario }
    }

    pub fn com_binario(binario: impl Into<String>) -> Self {
        FabricaClaude { binario: binario.into() }
    }

    pub fn binario(&self) -> &str {
        &self.binario
    }
}

impl Fabrica for FabricaClaude {
    fn criar(
        &self,
        _adaptador: Adaptador,
        ctx: &ContextoSessao,
    ) -> Resultado<Box<dyn AgenteAdapter>> {
        Ok(Box::new(AdaptadorClaude::novo(self.binario.clone(), ctx.clone())?))
    }
}

impl Drop for AdaptadorClaude {
    fn drop(&mut self) {
        // Os dois arquivos carregam o token. Deixá-los no temp depois que a
        // sessão morreu é deixar um segredo válido para ninguém, mas legível
        // por qualquer coisa.
        for caminho in [&self.arquivo_settings, &self.arquivo_mcp].into_iter().flatten() {
            let _ = std::fs::remove_file(caminho);
        }
    }
}


/// Restringe o arquivo ao dono. No Windows o temp já é por usuário; no Unix,
/// sem isto, qualquer conta na máquina lê o token.
#[cfg(unix)]
fn segredar(caminho: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(caminho, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn segredar(_caminho: &std::path::Path) {}
