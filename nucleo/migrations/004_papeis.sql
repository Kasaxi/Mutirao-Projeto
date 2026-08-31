-- Mutirão — 004: papel no nó, e modelo por papel.
--
-- A tabela `role` existe desde a 001 e nunca foi usada. Ela nasceu certa; o
-- que faltava era o nó saber a qual papel pertence, e o papel poder dizer em
-- que modelo ele roda.
--
-- ## Por que `role_id` no nó e não em `config_json`
--
-- `config_json` é o payload do TIPO do nó — o arquivo de uma nota, a URL de um
-- portal. Papel não é isso: é uma relação, e é a pergunta "quais nós usam este
-- papel?" que decide se dá para renomear ou apagar um. Enterrada em JSON, essa
-- pergunta vira varredura de tabela e a integridade vira convenção.
--
-- `ON DELETE SET NULL`, e não CASCADE: apagar um papel não pode levar junto o
-- nó e a conversa dele. O nó volta a ser um agente sem papel, que é o que ele
-- era antes de existir papel nenhum.

ALTER TABLE node ADD COLUMN role_id TEXT REFERENCES role(id) ON DELETE SET NULL;

CREATE INDEX idx_node_role ON node(role_id);

-- NULL = "o que a CLI do usuário estiver configurada para usar". É o
-- comportamento de hoje, e continua sendo o padrão: escolher modelo por papel
-- é otimização, e otimização chumbada envelhece mal.
ALTER TABLE role ADD COLUMN modelo TEXT;

-- Quem recrutou quem. Preenchido só por `recrutar`; NULL quer dizer "foi uma
-- pessoa que criou este nó".
--
-- Existe por dois motivos, e o segundo é o que importa: `dispensar` só pode
-- encerrar quem o próprio nó recrutou — sem esta coluna, um agente dispensaria
-- qualquer vizinho, inclusive um que a pessoa criou à mão.
ALTER TABLE node ADD COLUMN recrutado_por TEXT REFERENCES node(id) ON DELETE SET NULL;

CREATE INDEX idx_node_recrutador ON node(recrutado_por);
