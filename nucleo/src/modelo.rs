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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CustoDoNo {
    pub node_id: String,
    pub custo: f64,
}
