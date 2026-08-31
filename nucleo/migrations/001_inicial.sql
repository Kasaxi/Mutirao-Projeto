-- Mutirão — esquema inicial
-- Convenções:
--   ids são TEXT com UUID v4
--   instantes são INTEGER com epoch em milissegundos (UTC)
--   json é TEXT validado na borda, nunca no banco
--   toda FK tem ON DELETE explícito

PRAGMA foreign_keys = ON;

-- ---------------------------------------------------------------- workspace

CREATE TABLE workspace (
    id            TEXT PRIMARY KEY,
    nome          TEXT    NOT NULL,
    pasta         TEXT    NOT NULL UNIQUE,   -- caminho absoluto no disco
    criado_em     INTEGER NOT NULL,
    -- ensaio em foco. Sem FK: workspace e ensaio se referenciam em ciclo,
    -- e o SQLite não resolve isso sem constraint adiada.
    ensaio_ativo  TEXT,
    -- viewport do canvas, para reabrir onde o usuário parou
    vp_x          REAL    NOT NULL DEFAULT 0,
    vp_y          REAL    NOT NULL DEFAULT 0,
    vp_zoom       REAL    NOT NULL DEFAULT 1 CHECK (vp_zoom > 0)
);

-- ------------------------------------------------------------------- ensaio
-- Um worktree git isolado. O usuário chama de "rascunho"; nunca vê "branch".

CREATE TABLE ensaio (
    id               TEXT PRIMARY KEY,
    workspace_id     TEXT    NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
    nome             TEXT    NOT NULL,
    branch           TEXT    NOT NULL,
    caminho_worktree TEXT    NOT NULL,
    base_commit      TEXT,
    estado           TEXT    NOT NULL DEFAULT 'aberto'
                     CHECK (estado IN ('aberto', 'publicado', 'descartado')),
    criado_em        INTEGER NOT NULL,
    UNIQUE (workspace_id, nome)
);

CREATE INDEX idx_ensaio_workspace ON ensaio(workspace_id);

-- --------------------------------------------------------------------- node
-- Tudo que existe no canvas. `tipo` decide o formato de config_json.

CREATE TABLE node (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT    NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
    -- NULL = existe em todos os ensaios (nó do "térreo")
    ensaio_id    TEXT    REFERENCES ensaio(id) ON DELETE CASCADE,
    tipo         TEXT    NOT NULL
                 CHECK (tipo IN ('agente', 'nota', 'arquivos', 'portal', 'forma')),
    nome         TEXT    NOT NULL,
    x            REAL    NOT NULL,
    y            REAL    NOT NULL,
    w            REAL    NOT NULL CHECK (w  > 0),
    h            REAL    NOT NULL CHECK (h  > 0),
    z            INTEGER NOT NULL DEFAULT 0,     -- ordem de empilhamento
    config_json  TEXT    NOT NULL DEFAULT '{}',
    criado_em    INTEGER NOT NULL,
    alterado_em  INTEGER NOT NULL
);

CREATE INDEX idx_node_workspace ON node(workspace_id, ensaio_id);

-- --------------------------------------------------------------------- edge
-- Os cabos. Definem também o escopo de visibilidade de cada agente:
-- um nó só enxerga aquilo a que está ligado.

CREATE TABLE edge (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT    NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
    de_node      TEXT    NOT NULL REFERENCES node(id) ON DELETE CASCADE,
    para_node    TEXT    NOT NULL REFERENCES node(id) ON DELETE CASCADE,
    tipo         TEXT    NOT NULL
                 CHECK (tipo IN ('fala_com', 'le_nota', 'escreve_nota')),
    criado_em    INTEGER NOT NULL,
    CHECK (de_node <> para_node),
    UNIQUE (de_node, para_node, tipo)
);

CREATE INDEX idx_edge_de   ON edge(de_node);
CREATE INDEX idx_edge_para ON edge(para_node);

-- --------------------------------------------------------------------- role
-- Papel = prompt de sistema + ferramentas liberadas + autonomia.

CREATE TABLE role (
    id               TEXT PRIMARY KEY,
    nome             TEXT    NOT NULL UNIQUE,
    prompt           TEXT    NOT NULL,
    ferramentas_json TEXT    NOT NULL DEFAULT '[]',
    autonomia        TEXT    NOT NULL DEFAULT 'padrao'
                     CHECK (autonomia IN ('cauteloso', 'padrao', 'solto')),
    embutido         INTEGER NOT NULL DEFAULT 0,  -- 1 = veio com o app
    criado_em        INTEGER NOT NULL
);

-- ------------------------------------------------------------------ session
-- Uma sessão viva de agente. `token` é o segredo que o servidor MCP usa
-- para descobrir QUEM está chamando — sem ele não há escopo por nó.

CREATE TABLE session (
    id                TEXT PRIMARY KEY,
    node_id           TEXT    NOT NULL REFERENCES node(id) ON DELETE CASCADE,
    adaptador         TEXT    NOT NULL
                      CHECK (adaptador IN ('claude', 'codex', 'pty')),
    sessao_externa_id TEXT,                       -- id de retomada do agente
    token             TEXT    NOT NULL UNIQUE,    -- injetado na config MCP
    estado            TEXT    NOT NULL DEFAULT 'ocioso'
                      CHECK (estado IN ('ocioso', 'pensando', 'aguardando_aprovacao',
                                        'aguardando_humano', 'aguardando_no', 'erro')),
    pid               INTEGER,
    custo_total       REAL    NOT NULL DEFAULT 0,
    iniciada_em       INTEGER NOT NULL,
    ultimo_sinal_em   INTEGER NOT NULL            -- heartbeat: detecta travamento
);

CREATE INDEX        idx_session_node  ON session(node_id);
CREATE UNIQUE INDEX idx_session_token ON session(token);

-- ------------------------------------------------------------------ message
-- Histórico da conversa. É o que alimenta a face conversa.

CREATE TABLE message (
    id         TEXT PRIMARY KEY,
    session_id TEXT    NOT NULL REFERENCES session(id) ON DELETE CASCADE,
    papel      TEXT    NOT NULL CHECK (papel IN ('usuario', 'agente', 'sistema', 'no')),
    -- quando papel = 'no', de onde veio a mensagem
    origem_node TEXT   REFERENCES node(id) ON DELETE SET NULL,
    conteudo   TEXT    NOT NULL,
    tokens     INTEGER NOT NULL DEFAULT 0,
    custo      REAL    NOT NULL DEFAULT 0,
    trace_id   TEXT,                              -- amarra uma cadeia entre nós
    criado_em  INTEGER NOT NULL
);

CREATE INDEX idx_message_session ON message(session_id, criado_em);
CREATE INDEX idx_message_trace   ON message(trace_id);

-- ---------------------------------------------------------------- tool_call
-- Cada ação de agente vira uma linha. Append-only: é o log de auditoria.

CREATE TABLE tool_call (
    id              TEXT PRIMARY KEY,
    session_id      TEXT    NOT NULL REFERENCES session(id) ON DELETE CASCADE,
    ferramenta      TEXT    NOT NULL,
    argumentos_json TEXT    NOT NULL,
    resultado_json  TEXT,
    erro            TEXT,
    aprovacao       TEXT    NOT NULL DEFAULT 'automatica'
                    CHECK (aprovacao IN ('automatica', 'pendente', 'aprovada', 'negada')),
    decidido_por    TEXT,                          -- 'usuario' | 'regra:<nome>'
    decidido_em     INTEGER,
    criado_em       INTEGER NOT NULL
);

CREATE INDEX idx_toolcall_session   ON tool_call(session_id, criado_em);
CREATE INDEX idx_toolcall_pendentes ON tool_call(aprovacao) WHERE aprovacao = 'pendente';

-- ---------------------------------------------------------------- partitura
-- O time inteiro salvo para reabrir amanhã.

CREATE TABLE partitura (
    id            TEXT PRIMARY KEY,
    workspace_id  TEXT    NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
    nome          TEXT    NOT NULL,
    snapshot_json TEXT    NOT NULL,
    criado_em     INTEGER NOT NULL,
    UNIQUE (workspace_id, nome)
);
