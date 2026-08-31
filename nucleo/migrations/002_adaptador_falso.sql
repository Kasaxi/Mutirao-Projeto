-- Mutirão — 002: o adaptador falso vira cidadão de primeira classe.
--
-- O falso emite os mesmos EventoAgente do adaptador de verdade e existe para
-- que a orquestração seja testável sem gastar token. Gravar a sessão dele como
-- 'claude' seria mentira no log de auditoria — e `tool_call` é append-only,
-- então a mentira ficaria lá para sempre.
--
-- O SQLite não altera CHECK. A única saída é reconstruir a tabela: cria a nova,
-- copia, derruba a velha, renomeia. Fazer isso agora, com as tabelas vazias,
-- custa esta migration; fazer no M3, com sessão e histórico dentro, custa muito
-- mais. O runner desliga foreign_keys durante a migration — sem isso o
-- DROP TABLE abaixo levaria `message` e `tool_call` junto por CASCADE.

CREATE TABLE session_novo (
    id                TEXT PRIMARY KEY,
    node_id           TEXT    NOT NULL REFERENCES node(id) ON DELETE CASCADE,
    adaptador         TEXT    NOT NULL
                      CHECK (adaptador IN ('claude', 'codex', 'pty', 'falso')),
    sessao_externa_id TEXT,
    token             TEXT    NOT NULL UNIQUE,
    estado            TEXT    NOT NULL DEFAULT 'ocioso'
                      CHECK (estado IN ('ocioso', 'pensando', 'aguardando_aprovacao',
                                        'aguardando_humano', 'aguardando_no', 'erro')),
    pid               INTEGER,
    custo_total       REAL    NOT NULL DEFAULT 0,
    iniciada_em       INTEGER NOT NULL,
    ultimo_sinal_em   INTEGER NOT NULL
);

INSERT INTO session_novo
    SELECT id, node_id, adaptador, sessao_externa_id, token, estado, pid,
           custo_total, iniciada_em, ultimo_sinal_em
    FROM session;

DROP TABLE session;
ALTER TABLE session_novo RENAME TO session;

-- Os índices morreram junto com a tabela antiga; recriar é parte do serviço.
CREATE INDEX        idx_session_node  ON session(node_id);
CREATE UNIQUE INDEX idx_session_token ON session(token);

-- Só o turno mais recente interessa para reabrir a conversa; o índice de
-- `message` já cobre (session_id, criado_em). Aqui basta achar rápido a sessão
-- de um nó, que é o caminho de todo turno.
