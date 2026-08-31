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
}
