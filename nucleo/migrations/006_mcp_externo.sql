-- Mutirão — 006: servidores MCP externos por papel.
--
-- A decisão do `ARQUITETURA.md §7` — "não escrever nenhuma integração: ser um
-- host MCP". O agente geral precisa de vinte ferramentas, e escrever vinte
-- integrações à mão é o que mata o projeto no terceiro mês.
--
-- Fica no papel, e não no nó, porque quem precisa do CRM é o papel "Vendas",
-- não um nó específico. Pôr no nó faria cada agente novo repetir a
-- configuração, e configuração repetida diverge.
--
-- JSON e não tabela: a forma de um servidor MCP é do protocolo, não nossa, e
-- vai mudar com ele. Uma tabela nossa exigiria migration a cada campo novo do
-- MCP; um JSON validado na borda, não. Mesma regra do `config_json`.

ALTER TABLE role ADD COLUMN mcp_json TEXT NOT NULL DEFAULT '[]';
