-- Mutirão — 003: a caixa "não perguntar de novo".
--
-- A `ESPECIFICACAO.md §7` diz que essa caixa grava uma regra em
-- `role.ferramentas_json`. Papel é do M4 e ainda não existe, e inventar um
-- papel só para pendurar a regra seria pior que uma tabela honesta.
--
-- Escopo (workspace, ferramenta): "não perguntar de novo para gravar nesta
-- pasta". Não é por caminho de arquivo de propósito — uma regra por arquivo
-- vira uma lista que ninguém audita, e o usuário concedeu pensando na pasta.
--
-- O UNIQUE existe para conceder duas vezes não criar duas linhas: revogar
-- precisa apagar tudo o que foi concedido, não a metade.

CREATE TABLE regra_aprovacao (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT    NOT NULL REFERENCES workspace(id) ON DELETE CASCADE,
    ferramenta   TEXT    NOT NULL,
    -- Quem concedeu e quando. Uma permissão sem data não dá para auditar.
    criado_em    INTEGER NOT NULL,
    UNIQUE (workspace_id, ferramenta)
);

CREATE INDEX idx_regra_workspace ON regra_aprovacao(workspace_id);
