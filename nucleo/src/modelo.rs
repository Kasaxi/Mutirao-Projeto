use serde::{Deserialize, Serialize};

/// Instante em epoch de milissegundos, UTC. Tipo próprio para não confundir
/// com segundos em nenhum lugar do código.
pub type Instante = i64;

pub fn agora() -> Instante {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("relógio anterior a 1970")
        .as_millis() as i64
}

pub fn novo_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

// ---------------------------------------------------------------- workspace

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Workspace {
    pub id: String,
    pub nome: String,
    pub pasta: String,
    pub criado_em: Instante,
    pub ensaio_ativo: Option<String>,
    /// Onde mora o histórico oculto deste workspace. `None` = sem histórico:
    /// workspace de antes do M5, ou máquina sem git. Ver `git.rs`.
    pub repo: Option<String>,
    pub viewport: Viewport,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Viewport {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Viewport { x: 0.0, y: 0.0, zoom: 1.0 }
    }
}

// --------------------------------------------------------------------- node

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoNo {
    Agente,
    Nota,
    Arquivos,
    Portal,
    Forma,
}

impl TipoNo {
    pub fn como_texto(&self) -> &'static str {
        match self {
            TipoNo::Agente => "agente",
            TipoNo::Nota => "nota",
            TipoNo::Arquivos => "arquivos",
            TipoNo::Portal => "portal",
            TipoNo::Forma => "forma",
        }
    }

    pub fn do_texto(s: &str) -> Option<TipoNo> {
        Some(match s {
            "agente" => TipoNo::Agente,
            "nota" => TipoNo::Nota,
            "arquivos" => TipoNo::Arquivos,
            "portal" => TipoNo::Portal,
            "forma" => TipoNo::Forma,
            _ => return None,
        })
    }

    /// Tamanho inicial ao soltar no canvas, em unidades de mundo.
    pub fn tamanho_padrao(&self) -> (f64, f64) {
        match self {
            TipoNo::Agente => (420.0, 320.0),
            TipoNo::Nota => (260.0, 200.0),
            TipoNo::Arquivos => (280.0, 360.0),
            TipoNo::Portal => (480.0, 360.0),
            TipoNo::Forma => (200.0, 120.0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct No {
    pub id: String,
    pub workspace_id: String,
    pub ensaio_id: Option<String>,
    pub tipo: TipoNo,
    pub nome: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub z: i64,
    /// Payload específico do tipo. Validado na borda, opaco para o banco.
    pub config: serde_json::Value,
    /// Papel deste agente. `None` = agente sem papel, que é como todo nó
    /// nasceu até o M4: prompt padrão da CLI e o conjunto completo de
    /// ferramentas que o barramento oferece.
    pub role_id: Option<String>,
    /// Quem recrutou este nó. `None` = foi uma pessoa que o criou.
    pub recrutado_por: Option<String>,
    pub criado_em: Instante,
    pub alterado_em: Instante,
}

// --------------------------------------------------------------------- edge

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoCabo {
    FalaCom,
    LeNota,
    EscreveNota,
}

impl TipoCabo {
    pub fn como_texto(&self) -> &'static str {
        match self {
            TipoCabo::FalaCom => "fala_com",
            TipoCabo::LeNota => "le_nota",
            TipoCabo::EscreveNota => "escreve_nota",
        }
    }

    pub fn do_texto(s: &str) -> Option<TipoCabo> {
        Some(match s {
            "fala_com" => TipoCabo::FalaCom,
            "le_nota" => TipoCabo::LeNota,
            "escreve_nota" => TipoCabo::EscreveNota,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Cabo {
    pub id: String,
    pub workspace_id: String,
    pub de_node: String,
    pub para_node: String,
    pub tipo: TipoCabo,
    pub criado_em: Instante,
}

// ------------------------------------------------------------------- estado

/// Tudo que o front precisa para desenhar um workspace. Uma chamada só:
/// abrir um workspace não deve custar três viagens de IPC.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EstadoCanvas {
    pub workspace: Workspace,
    pub nos: Vec<No>,
    pub cabos: Vec<Cabo>,
}

// ------------------------------------------------------------------ sessão

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoSessao {
    Ocioso,
    Pensando,
    AguardandoAprovacao,
    AguardandoHumano,
    AguardandoNo,
    Erro,
}

impl EstadoSessao {
    pub fn como_texto(&self) -> &'static str {
        match self {
            EstadoSessao::Ocioso => "ocioso",
            EstadoSessao::Pensando => "pensando",
            EstadoSessao::AguardandoAprovacao => "aguardando_aprovacao",
            EstadoSessao::AguardandoHumano => "aguardando_humano",
            EstadoSessao::AguardandoNo => "aguardando_no",
            EstadoSessao::Erro => "erro",
        }
    }

    pub fn do_texto(s: &str) -> Option<EstadoSessao> {
        Some(match s {
            "ocioso" => EstadoSessao::Ocioso,
            "pensando" => EstadoSessao::Pensando,
            "aguardando_aprovacao" => EstadoSessao::AguardandoAprovacao,
            "aguardando_humano" => EstadoSessao::AguardandoHumano,
            "aguardando_no" => EstadoSessao::AguardandoNo,
            "erro" => EstadoSessao::Erro,
            _ => return None,
        })
    }

    /// O ponto vermelho do nó: o usuário precisa agir.
    pub fn pede_atencao(&self) -> bool {
        matches!(
            self,
            EstadoSessao::AguardandoAprovacao | EstadoSessao::AguardandoHumano | EstadoSessao::Erro
        )
    }

    /// Transições legítimas. Qualquer outra é bug do orquestrador, não do agente.
    pub fn pode_ir_para(&self, destino: EstadoSessao) -> bool {
        use EstadoSessao::*;
        match (self, destino) {
            (Ocioso, Pensando) => true,
            (Pensando, Ocioso) => true,
            (Pensando, AguardandoAprovacao) => true,
            (Pensando, AguardandoHumano) => true,
            (Pensando, AguardandoNo) => true,
            (Pensando, Erro) => true,
            (AguardandoAprovacao, Pensando) => true,
            (AguardandoAprovacao, Ocioso) => true, // negado e turno encerrado
            (AguardandoHumano, Pensando) => true,
            (AguardandoNo, Pensando) => true,
            (AguardandoNo, Erro) => true,          // prazo estourou
            (Erro, Ocioso) => true,                // usuário reconheceu
            (a, b) => a == &b,                     // permanecer é sempre válido
        }
    }

    /// O nó aceita um turno novo? Só quem está parado aceita — é aqui que mora
    /// a regra "um turno por vez por nó" do `ARQUITETURA.md §5`.
    pub fn aceita_turno(&self) -> bool {
        matches!(self, EstadoSessao::Ocioso | EstadoSessao::Erro)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Adaptador {
    Claude,
    Codex,
    Pty,
    /// Roteiro em vez de modelo. Existe para testar orquestração sem gastar
    /// token, e é gravado como si mesmo — ver `migrations/002`.
    Falso,
}

impl Adaptador {
    pub fn como_texto(&self) -> &'static str {
        match self {
            Adaptador::Claude => "claude",
            Adaptador::Codex => "codex",
            Adaptador::Pty => "pty",
            Adaptador::Falso => "falso",
        }
    }

    pub fn do_texto(s: &str) -> Option<Adaptador> {
        Some(match s {
            "claude" => Adaptador::Claude,
            "codex" => Adaptador::Codex,
            "pty" => Adaptador::Pty,
            "falso" => Adaptador::Falso,
            _ => return None,
        })
    }
}

/// Uma sessão de agente, do jeito que o front pode ver.
///
/// Repare no que **não** está aqui: `session.token`, o segredo que o servidor
/// MCP usa para descobrir qual nó está chamando (`ESPECIFICACAO.md §4`). Ele
/// mora só no banco e no processo do agente. Se um dia alguém precisar dele na
/// interface, a resposta é não — um token que chega ao front chega também a
/// qualquer coisa que rode no front.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Sessao {
    pub id: String,
    pub node_id: String,
    pub adaptador: Adaptador,
    pub sessao_externa_id: Option<String>,
    pub estado: EstadoSessao,
    pub custo_total: f64,
    pub iniciada_em: Instante,
    pub ultimo_sinal_em: Instante,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PapelMensagem {
    Usuario,
    Agente,
    Sistema,
    /// Veio de outro nó pelo cabo. `origem_node` diz de qual. M3 usa; o M1
    /// já grava para o histórico não precisar de migration depois.
    No,
}

impl PapelMensagem {
    pub fn como_texto(&self) -> &'static str {
        match self {
            PapelMensagem::Usuario => "usuario",
            PapelMensagem::Agente => "agente",
            PapelMensagem::Sistema => "sistema",
            PapelMensagem::No => "no",
        }
    }

    pub fn do_texto(s: &str) -> Option<PapelMensagem> {
        Some(match s {
            "usuario" => PapelMensagem::Usuario,
            "agente" => PapelMensagem::Agente,
            "sistema" => PapelMensagem::Sistema,
            "no" => PapelMensagem::No,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Mensagem {
    pub id: String,
    pub session_id: String,
    pub papel: PapelMensagem,
    pub origem_node: Option<String>,
    pub conteudo: String,
    pub tokens: i64,
    pub custo: f64,
    pub trace_id: Option<String>,
    pub criado_em: Instante,
}

/// Uma ação do agente, do jeito que vira card na face conversa.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChamadaFerramenta {
    pub id: String,
    pub session_id: String,
    pub ferramenta: String,
    pub argumentos: serde_json::Value,
    pub resultado: Option<serde_json::Value>,
    pub erro: Option<String>,
    pub aprovacao: Aprovacao,
    pub decidido_por: Option<String>,
    pub criado_em: Instante,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aprovacao {
    Automatica,
    Pendente,
    Aprovada,
    Negada,
}

impl Aprovacao {
    pub fn como_texto(&self) -> &'static str {
        match self {
            Aprovacao::Automatica => "automatica",
            Aprovacao::Pendente => "pendente",
            Aprovacao::Aprovada => "aprovada",
            Aprovacao::Negada => "negada",
        }
    }

    pub fn do_texto(s: &str) -> Option<Aprovacao> {
        Some(match s {
            "automatica" => Aprovacao::Automatica,
            "pendente" => Aprovacao::Pendente,
            "aprovada" => Aprovacao::Aprovada,
            "negada" => Aprovacao::Negada,
            _ => return None,
        })
    }
}

// ------------------------------------------------------------------- custo

/// Consumo de um turno. Entrada e saída separadas porque custam preços
/// diferentes — juntar os dois num número só torna o custo incalculável.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct Uso {
    pub tokens_entrada: i64,
    pub tokens_saida: i64,
    /// Em dólar. É a moeda em que a API cobra; converter para real exige uma
    /// cotação, que é decisão de produto e não do núcleo.
    pub custo_usd: f64,
}

impl Uso {
    pub fn tokens(&self) -> i64 {
        self.tokens_entrada + self.tokens_saida
    }
}

/// Preço por milhão de tokens (entrada, saída), em dólar.
///
/// Tabela pública da Anthropic, conferida em 31/08/2026. Fica no núcleo porque
/// é o número que o usuário vê no canto do nó, e um preço errado é pior que
/// nenhum: some a confiança no painel inteiro. Ao acrescentar modelo, confira
/// a tabela vigente — não deduza pelo nome.
pub fn preco_por_milhao(modelo: &str) -> (f64, f64) {
    match modelo {
        m if m.starts_with("claude-opus-5") => (5.0, 25.0),
        m if m.starts_with("claude-sonnet-5") => (2.0, 10.0),
        m if m.starts_with("claude-haiku-4-5") => (1.0, 5.0),
        // Modelo desconhecido não vira custo zero: zero mente e some do painel.
        // Sem preço, o front mostra "—" e o usuário sabe que não sabemos.
        _ => (f64::NAN, f64::NAN),
    }
}

pub fn custo_do_uso(modelo: &str, tokens_entrada: i64, tokens_saida: i64) -> f64 {
    let (entrada, saida) = preco_por_milhao(modelo);
    (tokens_entrada as f64 * entrada + tokens_saida as f64 * saida) / 1_000_000.0
}

// ---------------------------------------------------------------- eventos

/// O que um adaptador reporta. Todo adaptador — Claude, Codex, falso — traduz
/// a saída nativa para isto, e o resto do sistema só conhece esta forma.
///
/// `tipo` como discriminante: vira união discriminada em TypeScript sem
/// nenhuma tradução manual na fronteira.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum EventoAgente {
    SessaoIniciada {
        id_externo: String,
        modelo: String,
        ferramentas: Vec<String>,
    },
    TextoParcial {
        delta: String,
    },
    Raciocinando {
        resumo: String,
    },
    FerramentaPedida {
        id: String,
        nome: String,
        argumentos: serde_json::Value,
    },
    FerramentaConcluida {
        id: String,
        resultado: Option<serde_json::Value>,
        erro: Option<String>,
    },
    /// O sino. Com stream estruturado o fim do turno é evento explícito, não
    /// adivinhação sobre o texto do terminal — é a razão de ser da Decisão 1.
    TurnoConcluido {
        texto_final: String,
        uso: Uso,
    },
    PrecisaHumano {
        pergunta: String,
    },
    Erro {
        mensagem: String,
        recuperavel: bool,
    },
}

impl EventoAgente {
    /// Eventos que encerram o turno. O bombeamento para de escutar depois de
    /// um destes; sem isso a thread do turno nunca termina.
    pub fn encerra_turno(&self) -> bool {
        matches!(self, EventoAgente::TurnoConcluido { .. } | EventoAgente::Erro { .. })
    }
}

/// O que o núcleo conta para a interface. Nomes de evento e formato em
/// `ESPECIFICACAO.md §3` — o `src-tauri` só mapeia variante para nome.
///
/// Regra: evento **notifica**, não carrega histórico. Quem quiser a conversa
/// inteira pede por comando. Sem isso a fronteira IPC vira mangueira de bombeiro.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum EventoNucleo {
    SessaoEvento {
        session_id: String,
        evento: EventoAgente,
    },
    SessaoEstado {
        session_id: String,
        node_id: String,
        estado: EstadoSessao,
        pede_atencao: bool,
    },
    CustoAtualizado {
        workspace_id: String,
        total: f64,
        por_no: Vec<CustoDoNo>,
    },
    /// Uma ferramenta parou à espera de gente. O nó está em
    /// `aguardando_aprovacao` e o agente, literalmente parado, segurando a
    /// resposta HTTP do hook até alguém clicar.
    AprovacaoPedida {
        pedido: PedidoAprovacao,
    },
    AprovacaoDecidida {
        tool_call_id: String,
        node_id: String,
        decisao: Decisao,
        /// `usuario` ou `regra:<ferramenta>`. Quem decidiu importa tanto
        /// quanto o que foi decidido.
        decidido_por: String,
    },
    /// Um nó falou com outro. A interface anima o cabo — é o que torna a ponte
    /// visível em vez de mágica.
    NoMensagem {
        de_node: String,
        para_node: String,
        trace_id: String,
        /// `tipo_mensagem` e não `tipo`: o `serde(tag = "tipo")` deste enum já
        /// ocupa esse nome no JSON, e um campo homônimo não compila.
        tipo_mensagem: TipoMensagem,
    },
    /// Uma cadeia acabou por limite, não por conclusão. Sempre chega ao
    /// usuário: o `ARQUITETURA.md §6` é explícito em que estourar um limite
    /// "avisa o usuário em vez de queimar crédito em silêncio".
    CadeiaEncerrada {
        trace_id: String,
        node_id: String,
        motivo: String,
    },
    /// A cadeia parou, e quem pode destravá-la é **a pessoa**.
    ///
    /// Acontece quando A está esperando B e B levanta a mão para perguntar
    /// alguma coisa. Nenhum dos limites pega isso, e nem deveria: não é
    /// travamento, é uma pergunta aberta. Mas sem este aviso o canvas mostra
    /// dois nós calados — um "pensando", outro "aguardando" — e a pessoa não
    /// tem como saber que a fila inteira depende de um clique dela.
    CadeiaEsperaPessoa {
        trace_id: String,
        /// Quem está parado esperando.
        node_id: String,
        /// Quem levantou a mão. É neste nó que a resposta tem de ser dada.
        perguntou_node: String,
        perguntou_nome: String,
    },
    /// O canvas mudou por fora da interface — hoje só quando um agente recruta
    /// outro. O front relê o workspace ao ver isto.
    ///
    /// Evento avisa, não carrega: mandar o canvas inteiro por evento seria a
    /// mangueira de bombeiro que a §3 proíbe. `motivo` é para o log e para a
    /// barra, não para o front decidir o que fazer.
    CanvasMudou {
        workspace_id: String,
        motivo: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CustoDoNo {
    pub node_id: String,
    pub custo: f64,
}

// ------------------------------------------------------------- a ponte

/// Uma cadeia de conversa entre nós.
///
/// Nasce quando o usuário fala com um agente e viaja com cada `enviar_para`.
/// É o que amarra "Pesquisador → Redator → Pesquisador" numa coisa só, e é
/// sobre ela que os três limites do `ARQUITETURA.md §6` incidem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trace {
    pub id: String,
    /// Quantas vezes esta cadeia já mudou de nó.
    pub saltos: u32,
}

impl Trace {
    /// Começa uma cadeia. Toda mensagem do usuário abre uma nova.
    pub fn novo() -> Trace {
        Trace { id: format!("tr_{}", &novo_id()[..8]), saltos: 0 }
    }

    /// A cadeia dando mais um passo. `None` quando já andou demais.
    pub fn saltar(&self) -> Option<Trace> {
        if self.saltos + 1 > MAX_SALTOS {
            return None;
        }
        Some(Trace { id: self.id.clone(), saltos: self.saltos + 1 })
    }
}

/// Teto de saltos de uma cadeia (`ARQUITETURA.md §6`).
///
/// Ciclo A→B→A é legítimo e comum — o Pesquisador pergunta, o Redator responde,
/// o Pesquisador confirma. O que mata é o ciclo infinito, e é por isso que o
/// limite é do host, não do agente: um agente convencido de que precisa de
/// mais uma rodada sempre acha um motivo.
pub const MAX_SALTOS: u32 = 6;

/// Prazo padrão de uma mensagem que espera resposta, e o teto que o agente
/// pode pedir. Os dois em `ESPECIFICACAO.md §5`.
pub const PRAZO_MENSAGEM_PADRAO_MS: u64 = 600_000;
pub const PRAZO_MENSAGEM_TETO_MS: u64 = 1_800_000;

/// Quanto uma cadeia pode gastar antes de ser encerrada, em dólar.
///
/// Existe porque o pior desfecho de um ciclo malcomportado não é travar — é
/// não travar, e queimar crédito a noite inteira em silêncio. Fixo no M3;
/// vira configurável no M6, junto com o teto por workspace.
pub const ORCAMENTO_POR_TRACE_USD: f64 = 1.00;

/// O envelope do `ARQUITETURA.md §6`, do jeito que atravessa a ponte.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Envelope {
    pub id: String,
    pub de: String,
    pub para: String,
    pub tipo: TipoMensagem,
    pub corpo: String,
    pub refs: Vec<String>,
    pub trace: String,
    pub saltos: u32,
    pub prazo_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoMensagem {
    /// Espera resposta. O nó que mandou fica em `aguardando_no`.
    Pedido,
    /// Entrega e segue em frente.
    Aviso,
}

// -------------------------------------------------------------- aprovação

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decisao {
    Aprovada,
    Negada,
}

// ============================================================ M4: papéis ===

/// Quanto o papel pode fazer sozinho.
///
/// **A autonomia escolhe o conjunto de ferramentas, nunca se o card aparece.**
/// Essa distinção é o `ARQUITETURA.md §8` inteiro: um nível que dispensasse a
/// aprovação seria o "pular todas as permissões" que a §8 proíbe, com outro
/// nome. Um papel `Solto` grava com card, igual a um `Padrao`; ele só alcança
/// mais coisa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Autonomia {
    /// Lê, conversa e pergunta. Não grava, não roda comando.
    Cauteloso,
    /// Grava nota e arquivo — cada gravação passa pelo card.
    Padrao,
    /// Mais o `Bash`, que pede card **sempre**: liberar comando de uma vez por
    /// todas seria entregar a máquina num clique que ninguém lembra depois.
    Solto,
}

impl Autonomia {
    pub fn como_texto(&self) -> &'static str {
        match self {
            Autonomia::Cauteloso => "cauteloso",
            Autonomia::Padrao => "padrao",
            Autonomia::Solto => "solto",
        }
    }

    pub fn do_texto(s: &str) -> Option<Autonomia> {
        Some(match s {
            "cauteloso" => Autonomia::Cauteloso,
            "padrao" => Autonomia::Padrao,
            "solto" => Autonomia::Solto,
            _ => return None,
        })
    }
}

/// Papel = prompt de sistema + ferramentas + autonomia (+ modelo, se quiser).
///
/// É o que transforma "um agente" em "o Revisor". Sem papel, dois nós lado a
/// lado são o mesmo programa com nomes diferentes — e um time de quatro iguais
/// não é um time, é uma repetição.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Papel {
    pub id: String,
    pub nome: String,
    /// Vai para o `--append-system-prompt` da CLI em **todo** turno. Medido na
    /// 2.1.251: ele sobrevive ao `--resume` sozinho, mas passar de novo é o
    /// que faz uma edição de papel valer para a conversa que já existe.
    pub prompt: String,
    /// Ferramentas do §6 que este papel enxerga, sem o prefixo do servidor.
    /// Vazio = o que a autonomia der.
    pub ferramentas: Vec<String>,
    pub autonomia: Autonomia,
    /// `None` = o que a CLI do usuário estiver configurada para usar.
    pub modelo: Option<String>,
    /// Veio com o app. Não dá para apagar — o usuário duplica e edita a cópia.
    pub embutido: bool,
    pub criado_em: Instante,
    /// Servidores MCP externos deste papel. É a decisão do §7 — ser host MCP
    /// em vez de escrever integração.
    #[serde(default)]
    pub mcp: Vec<ServidorMcp>,
}

/// Um servidor MCP de fora, ligado a um papel.
///
/// Só HTTP por enquanto, e é escolha consciente: é o transporte que o nosso
/// próprio barramento já fala, então o formato do `--mcp-config` já está
/// medido. Servidor por stdio cabe no mesmo lugar quando aparecer o primeiro
/// que importe — e aparecer primeiro é a diferença entre desenhar para um caso
/// real e desenhar para um imaginado.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServidorMcp {
    /// Vira o prefixo que o modelo vê (`mcp__crm__buscar`) e entra no matcher
    /// do hook, que é uma regex — por isso só letras, números e `_`.
    pub nome: String,
    pub url: String,
    /// Cabeçalhos, tipicamente a autenticação.
    ///
    /// **Nunca atravessam a fronteira IPC** — o que chega ao front chega a
    /// tudo que roda no front, e a chave do CRM de alguém merece a mesma
    /// regra do token da sessão. Quem os tira é [`Papel::sem_segredos`], no
    /// comando que devolve papéis ao front.
    ///
    /// A primeira tentativa foi `#[serde(skip_serializing)]` no campo, e ela
    /// estava errada: o mesmo `Serialize` grava no banco, então o segredo
    /// deixava de chegar ao IPC **e** ao disco — o adaptador ficava sem o que
    /// mandar. Um teste pegou. Esconder é trabalho da borda, não do tipo.
    #[serde(default)]
    pub cabecalhos: Vec<(String, String)>,
}

impl Papel {
    /// Uma cópia sem os segredos, para atravessar a fronteira IPC.
    pub fn sem_segredos(mut self) -> Papel {
        for s in &mut self.mcp {
            s.cabecalhos.clear();
        }
        self
    }
}

/// Teto de nós que uma cadeia pode recrutar.
///
/// O `ARQUITETURA.md §6` limita a conversa entre nós, não a criação deles: os
/// três limites do M3 incidem sobre saltos, prazo e gasto de uma cadeia, e
/// nenhum deles impede um Maestro de recrutar cem agentes num turno só. Este é
/// o limite que faltava, e ele existe pelo mesmo motivo dos outros — o pior
/// desfecho não é travar, é não travar.
pub const MAX_RECRUTAS_POR_CADEIA: usize = 6;

/// Teto de nós de agente por workspace, contando os que a pessoa criou.
///
/// Serve ao caso que o limite por cadeia não cobre: um Maestro que recruta
/// três hoje, três amanhã e três depois. Vinte é folgado para trabalho de
/// verdade e apertado o bastante para um laço não passar despercebido.
pub const MAX_AGENTES_POR_WORKSPACE: usize = 20;

/// Um time salvo para reabrir amanhã.
///
/// Guarda **layout e papéis**, não conversas: partitura é a planta do time, não
/// um backup dele. Reabrir monta os mesmos nós com os mesmos papéis e cabos,
/// prontos para trabalhar de novo — não ressuscita o que já foi dito.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Partitura {
    pub id: String,
    pub workspace_id: String,
    pub nome: String,
    pub snapshot: Snapshot,
    pub criado_em: Instante,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub nos: Vec<NoSalvo>,
    pub cabos: Vec<CaboSalvo>,
}

/// Um nó dentro de uma partitura.
///
/// Sem id: o id de um nó pertence ao canvas onde ele vive, e reabrir uma
/// partitura cria nós **novos**. Guardar o id antigo convidaria a "restaurar
/// por cima", que é backup — e partitura não é backup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NoSalvo {
    pub tipo: TipoNo,
    pub nome: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub config: serde_json::Value,
    /// Papel pelo **nome**, não pelo id: uma partitura precisa poder abrir
    /// noutra máquina, onde o mesmo papel tem outro id.
    pub papel: Option<String>,
}

/// Cabo dentro de uma partitura, pelos índices em [`Snapshot::nos`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct CaboSalvo {
    pub de: usize,
    pub para: usize,
    pub tipo: TipoCabo,
}

// =========================================================== M5: ensaios ===

/// Em que pé está um rascunho.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoEnsaio {
    /// Em uso. Só um rascunho aberto pode ser o ativo de um workspace.
    Aberto,
    /// Já foi para a pasta de verdade.
    Publicado,
    /// Jogado fora. A linha fica, para "o que aconteceu com aquele rascunho?"
    /// ter resposta.
    Descartado,
}

impl EstadoEnsaio {
    pub fn como_texto(&self) -> &'static str {
        match self {
            EstadoEnsaio::Aberto => "aberto",
            EstadoEnsaio::Publicado => "publicado",
            EstadoEnsaio::Descartado => "descartado",
        }
    }

    pub fn do_texto(s: &str) -> Option<EstadoEnsaio> {
        Some(match s {
            "aberto" => EstadoEnsaio::Aberto,
            "publicado" => EstadoEnsaio::Publicado,
            "descartado" => EstadoEnsaio::Descartado,
            _ => return None,
        })
    }
}

/// Um rascunho: uma cópia isolada da pasta em que o time trabalha sem mexer no
/// que está valendo.
///
/// O usuário nunca lê "branch" nem "worktree" — ele vê "Rascunho 2" e
/// "Publicar". Os dois campos técnicos existem porque alguém precisa saber
/// deles, e esse alguém é o `git.rs`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ensaio {
    pub id: String,
    pub workspace_id: String,
    pub nome: String,
    pub branch: String,
    pub caminho_worktree: String,
    /// Commit da pasta de verdade quando este rascunho nasceu.
    pub base_commit: Option<String>,
    pub estado: EstadoEnsaio,
    pub criado_em: Instante,
    pub alterado_em: Instante,
}

/// O que a tela de publicar mostra **antes** do clique.
///
/// Nenhuma palavra de Git: "6 arquivos alterados, 1 conflito", e o conflito
/// com os dois lados para escolher. Ver `ESPECIFICACAO.md §7`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreviaPublicacao {
    pub ensaio_id: String,
    pub alteracoes: Vec<MudancaArquivo>,
    /// Arquivos que mudaram dos dois lados. Cada um precisa de uma escolha
    /// antes de publicar — publicar pela metade é pior que não publicar.
    pub conflitos: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MudancaArquivo {
    pub caminho: String,
    pub como: TipoMudanca,
}

/// A letra do git virada palavra de gente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoMudanca {
    Criado,
    Alterado,
    Apagado,
    Renomeado,
}

impl TipoMudanca {
    /// Traduz a letra de `git diff --name-status`. Desconhecida vira
    /// `Alterado`: dizer "alterado" para uma cópia é impreciso; inventar uma
    /// categoria nova na tela é pior.
    pub fn da_letra(letra: &str) -> TipoMudanca {
        match letra.chars().next() {
            Some('A') => TipoMudanca::Criado,
            Some('D') => TipoMudanca::Apagado,
            Some('R') => TipoMudanca::Renomeado,
            _ => TipoMudanca::Alterado,
        }
    }

    pub fn como_texto(&self) -> &'static str {
        match self {
            TipoMudanca::Criado => "criado",
            TipoMudanca::Alterado => "alterado",
            TipoMudanca::Apagado => "apagado",
            TipoMudanca::Renomeado => "renomeado",
        }
    }
}

/// De qual lado ficar num conflito. Espelha `git::Lado`, mas com nome de
/// produto: o usuário escolhe entre "o que já estava" e "o que o rascunho fez".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LadoDoConflito {
    Original,
    Rascunho,
}

/// Uma permissão concedida com "não perguntar de novo".
///
/// Escopo (workspace, ferramenta) — "gravar nesta pasta", não "gravar neste
/// arquivo". Regra por arquivo vira lista que ninguém audita.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegraAprovacao {
    pub id: String,
    pub workspace_id: String,
    pub ferramenta: String,
    pub criado_em: Instante,
}

impl RegraAprovacao {
    /// O que gravar em `tool_call.decidido_por` quando esta regra decide.
    /// O formato `regra:<nome>` é o do `ESPECIFICACAO.md §7`.
    pub fn assinatura(&self) -> String {
        format!("regra:{}", self.ferramenta)
    }
}

/// O que a interface precisa para desenhar o card de aprovação.
///
/// Traz `resumo` e `detalhe` já mastigados porque quem sabe traduzir
/// "Write com file_path=orçamento.xlsx" para "Gravar orçamento.xlsx" é o
/// núcleo, não o front — e assim os dois adaptadores contam a mesma história.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PedidoAprovacao {
    pub tool_call_id: String,
    pub session_id: String,
    pub node_id: String,
    pub ferramenta: String,
    /// Uma linha: "Gravar orçamento.xlsx".
    pub resumo: String,
    /// A segunda linha: "14 linhas · 2,3 kB", ou o comando que vai rodar.
    pub detalhe: String,
    /// Prévia do conteúdo, encolhida. `None` quando a ferramenta não escreve.
    pub previa: Option<String>,
    pub criado_em: Instante,
}

/// Descreve um pedido de ferramenta em português de gente.
///
/// Os nomes vêm do Claude Code (`Write`, `Edit`, `Bash`…). Quando aparecer um
/// desconhecido, mostrar o nome cru é melhor que inventar um verbo errado — o
/// usuário está prestes a autorizar isso.
pub fn descrever_ferramenta(ferramenta: &str, argumentos: &serde_json::Value) -> (String, String) {
    let campo = |c: &str| argumentos.get(c).and_then(|v| v.as_str()).unwrap_or("");
    let arquivo = |c: &str| {
        let bruto = campo(c);
        bruto.rsplit(['/', '\\']).next().unwrap_or(bruto).to_string()
    };

    match ferramenta {
        "Write" => {
            let conteudo = campo("content");
            (
                format!("Gravar {}", arquivo("file_path")),
                format!("{} linhas · {}", conteudo.lines().count(), tamanho(conteudo.len())),
            )
        }
        "Edit" | "NotebookEdit" => (
            format!("Alterar {}", arquivo("file_path")),
            "trecho substituído no arquivo".to_string(),
        ),
        "Bash" => (
            "Rodar um comando".to_string(),
            campo("command").chars().take(120).collect(),
        ),
        // As duas do nosso servidor MCP que gravam. Os literais estão escritos
        // à mão porque `match` não aceita expressão — o teste
        // `os_nomes_mcp_do_card_batem_com_o_catalogo` é quem garante que eles
        // continuem iguais a `ferramentas::nome_completo`.
        "mcp__mutirao__escrever_nota" => {
            let conteudo = campo("conteudo");
            let modo = if campo("modo") == "acrescentar" { "acrescentar em" } else { "gravar" };
            (
                format!("Escrever na nota {}", campo("nota")),
                format!(
                    "{modo} · {} linhas · {}",
                    conteudo.lines().count(),
                    tamanho(conteudo.len())
                ),
            )
        }
        "mcp__mutirao__escrever_arquivo" => {
            let conteudo = campo("conteudo");
            (
                format!("Gravar {}", arquivo("caminho")),
                format!("{} linhas · {}", conteudo.lines().count(), tamanho(conteudo.len())),
            )
        }
        outro => (
            format!("Usar {outro}"),
            argumentos.to_string().chars().take(120).collect(),
        ),
    }
}

fn tamanho(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else {
        format!("{:.1} kB", bytes as f64 / 1024.0)
    }
}
