-- Mutirão — 005: onde mora o histórico oculto.
--
-- A tabela `ensaio` existe desde a 001 e nunca foi usada; `workspace.ensaio_ativo`
-- e `node.ensaio_id` também. O que faltava era o workspace saber ONDE está o
-- repositório dele.
--
-- ## Por que o repositório fica fora da pasta do usuário
--
-- O `ARQUITETURA.md` dizia `.mutirao/` dentro da pasta, com atributo hidden. A
-- mudança tem um motivo específico do Windows 11: a pasta de trabalho de
-- alguém quase sempre está em `Documentos`, que quase sempre está sincronizada
-- com o OneDrive — e um diretório Git dentro de pasta sincronizada é uma forma
-- conhecida de corromper o repositório, porque o sincronizador mexe em
-- arquivos que o Git presume só seus.
--
-- Fora, a pasta do usuário fica literalmente limpa: nenhum `.git`, nenhum
-- `.mutirao`, nada para o Explorer mostrar. Medido: depois de `init` e um
-- commit, `ls -a` lista só os arquivos do trabalho.
--
-- O caminho é absoluto e quem o escolhe é a casca, porque onde ficam os dados
-- de um app é pergunta do sistema operacional e o núcleo não conhece nenhum.
-- NULL quer dizer "sem histórico" — workspace do M0 ao M4, ou máquina sem git
-- instalado. Sem histórico o app inteiro continua servindo; só não tem
-- rascunho.

ALTER TABLE workspace ADD COLUMN repo TEXT;

-- O `estado` do ensaio já vinha com CHECK na 001 ('aberto', 'publicado',
-- 'descartado'). O que faltava era saber quando ele mudou de estado — sem
-- isso, "publicado quando?" não tem resposta, e a lista de rascunhos não
-- consegue ordenar o que interessa primeiro.
ALTER TABLE ensaio ADD COLUMN alterado_em INTEGER NOT NULL DEFAULT 0;
