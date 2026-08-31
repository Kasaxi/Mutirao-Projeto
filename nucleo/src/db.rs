use crate::erro::{Erro, Resultado};
use crate::modelo::*;
use rusqlite::{params, Connection, Row};
use std::path::Path;

/// Migrations embutidas no binário. Para adicionar uma, some um item aqui —
/// nunca edite um arquivo já publicado, mesmo para corrigir. O índice do
/// vetor + 1 é a versão gravada em `PRAGMA user_version`.
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/001_inicial.sql"),
    include_str!("../migrations/002_adaptador_falso.sql"),
    include_str!("../migrations/003_regras_de_aprovacao.sql"),
    include_str!("../migrations/004_papeis.sql"),
    include_str!("../migrations/005_ensaios.sql"),
    include_str!("../migrations/006_mcp_externo.sql"),
];

pub struct Banco {
    conn: Connection,
}

impl Banco {
    /// Abre (ou cria) o banco no caminho dado e aplica o que faltar de migration.
    pub fn abrir(caminho: &Path) -> Resultado<Banco> {
        if let Some(pai) = caminho.parent() {
            std::fs::create_dir_all(pai)?;
        }
        let conn = Connection::open(caminho)?;
        Banco::preparar(conn)
    }

    /// Banco em memória. Usado pelos testes e pelo modo de demonstração.
    pub fn em_memoria() -> Resultado<Banco> {
        Banco::preparar(Connection::open_in_memory()?)
    }

    fn preparar(conn: Connection) -> Resultado<Banco> {
        // WAL: leitura não bloqueia escrita. Sem isso, o canvas engasga
        // enquanto uma sessão grava mensagem.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let mut banco = Banco { conn };
        banco.migrar()?;
        banco.destravar_sessoes()?;
        // A biblioteca de papéis vive no banco, não em constante: o usuário
        // edita os embutidos e cria os dele. Semear em toda abertura é
        // idempotente pelo nome, e é o que garante que um banco do M3 ganhe a
        // biblioteca ao abrir no M4 — sem migration que insere linha, que
        // envelhece mal quando o texto do prompt muda.
        crate::papeis::semear(&banco)?;
        Ok(banco)
    }

    /// Conserta sessões que ficaram no ar quando o app morreu no meio de um
    /// turno. Sem isto, fechar o Mutirão enquanto um agente pensa deixa o nó
    /// em `pensando` para sempre: ele não pede atenção, não aceita turno novo
    /// e não explica nada. Roda em toda abertura porque esquecer de chamar é
    /// exatamente o tipo de erro que só aparece na máquina do usuário.
    ///
    /// `aguardando_aprovacao` e `aguardando_humano` ficam como estão de
    /// propósito: são perguntas legítimas que sobrevivem ao fechamento, e
    /// apagá-las perderia a pergunta.
    fn destravar_sessoes(&self) -> Resultado<()> {
        let presas: Vec<String> = {
            let mut st = self.conn.prepare(
                "SELECT id FROM session WHERE estado IN ('pensando', 'aguardando_no')",
            )?;
            let linhas = st.query_map([], |r| r.get::<_, String>(0))?;
            linhas.collect::<Result<Vec<_>, _>>()?
        };
        for id in presas {
            self.gravar_mensagem(
                &id,
                PapelMensagem::Sistema,
                "O app foi fechado no meio deste turno. A resposta não chegou.",
                Uso::default(),
            )?;
            self.conn.execute(
                "UPDATE session SET estado = 'erro', ultimo_sinal_em = ?2 WHERE id = ?1",
                params![id, agora()],
            )?;
        }
        Ok(())
    }

    fn migrar(&mut self) -> Resultado<()> {
        let versao: i64 =
            self.conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        let alvo = MIGRATIONS.len() as i64;
        if versao > alvo {
            return Err(Erro::invalido(format!(
                "banco na versão {versao}, app só conhece até {alvo}. Atualize o Mutirão."
            )));
        }
        for i in versao..alvo {
            // FK desligada durante a migration. Mudar um CHECK no SQLite exige
            // reconstruir a tabela, e um `DROP TABLE` com FK ligada leva os
            // filhos junto por CASCADE — a 002 perderia `message` e `tool_call`.
            // Tem de ser FORA da transação: dentro, este PRAGMA é ignorado sem
            // avisar, e aí o estrago acontece calado.
            self.conn.pragma_update(None, "foreign_keys", "OFF")?;
            let tx = self.conn.transaction()?;
            tx.execute_batch(MIGRATIONS[i as usize])?;
            tx.pragma_update(None, "user_version", i + 1)?;
            tx.commit()?;
            self.conn.pragma_update(None, "foreign_keys", "ON")?;

            // Uma migration que deixa referência órfã é bug de quem a escreveu.
            // Falhar aqui, na subida, é muito melhor que descobrir meses depois
            // com um JOIN que devolve menos linha do que devia.
            let orfas: i64 =
                self.conn.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
                    r.get(0)
                })?;
            if orfas > 0 {
                return Err(Erro::invalido(format!(
                    "a migration {} deixou {orfas} referência(s) órfã(s)",
                    i + 1
                )));
            }
        }
        Ok(())
    }

    pub fn versao_esquema(&self) -> Resultado<i64> {
        Ok(self.conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    // ------------------------------------------------------------ workspace

    pub fn criar_workspace(&self, nome: &str, pasta: &str) -> Resultado<Workspace> {
        let nome = nome.trim();
        if nome.is_empty() {
            return Err(Erro::invalido("o workspace precisa de um nome"));
        }
        let ws = Workspace {
            id: novo_id(),
            nome: nome.to_string(),
            pasta: pasta.to_string(),
            criado_em: agora(),
            ensaio_ativo: None,
            repo: None,
            viewport: Viewport::default(),
        };
        self.conn.execute(
            "INSERT INTO workspace (id, nome, pasta, criado_em, ensaio_ativo, vp_x, vp_y, vp_zoom)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7)",
            params![ws.id, ws.nome, ws.pasta, ws.criado_em,
                    ws.viewport.x, ws.viewport.y, ws.viewport.zoom],
        )?;
        Ok(ws)
    }

    pub fn listar_workspaces(&self) -> Resultado<Vec<Workspace>> {
        let mut st = self.conn.prepare(
            "SELECT id, nome, pasta, criado_em, ensaio_ativo, vp_x, vp_y, vp_zoom, repo
             FROM workspace ORDER BY criado_em DESC",
        )?;
        let linhas = st.query_map([], le_workspace)?;
        Ok(linhas.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn obter_workspace(&self, id: &str) -> Resultado<Workspace> {
        let mut st = self.conn.prepare(
            "SELECT id, nome, pasta, criado_em, ensaio_ativo, vp_x, vp_y, vp_zoom, repo
             FROM workspace WHERE id = ?1",
        )?;
        st.query_row(params![id], le_workspace)
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Erro::nao_encontrado("workspace", id),
                outro => Erro::Banco(outro),
            })
    }

    /// **A pasta em que o trabalho acontece agora.**
    ///
    /// Ponto único, e é isso que importa: se o workspace tem um rascunho
    /// aberto, é a pasta dele; senão, é a pasta do usuário. Todo lugar que
    /// precisa saber "onde escrevo" passa por aqui — o contexto do adaptador,
    /// as ferramentas do §6, a árvore de arquivos e as notas.
    ///
    /// Ter dois lugares que respondem isso seria o pior bug que este projeto
    /// pode ter: uma sessão viva apontando para o worktree errado grava no
    /// lugar errado **com aprovação legítima do usuário**. O card diria a
    /// verdade sobre o conteúdo e mentiria sobre o destino.
    pub fn pasta_de_trabalho(&self, workspace_id: &str) -> Resultado<String> {
        let ws = self.obter_workspace(workspace_id)?;
        let Some(id) = &ws.ensaio_ativo else {
            return Ok(ws.pasta);
        };
        match self.obter_ensaio(id) {
            Ok(e) if e.estado == EstadoEnsaio::Aberto => Ok(e.caminho_worktree),
            // Rascunho publicado, descartado ou sumido: o trabalho volta para a
            // pasta de verdade em vez de parar. Um ponteiro velho não pode
            // deixar o workspace inutilizável.
            _ => Ok(ws.pasta),
        }
    }

    /// Onde fica o repositório oculto deste workspace. Quem escolhe é a casca:
    /// onde ficam os dados de um app é pergunta do sistema operacional.
    pub fn definir_repo(&self, workspace_id: &str, repo: &str) -> Resultado<()> {
        let n = self.conn.execute(
            "UPDATE workspace SET repo = ?2 WHERE id = ?1",
            params![workspace_id, repo],
        )?;
        if n == 0 {
            return Err(Erro::nao_encontrado("workspace", workspace_id));
        }
        Ok(())
    }

    pub fn salvar_viewport(&self, workspace_id: &str, vp: Viewport) -> Resultado<()> {
        if !(vp.zoom.is_finite() && vp.zoom > 0.0) {
            return Err(Erro::invalido("zoom inválido"));
        }
        let n = self.conn.execute(
            "UPDATE workspace SET vp_x = ?2, vp_y = ?3, vp_zoom = ?4 WHERE id = ?1",
            params![workspace_id, vp.x, vp.y, vp.zoom],
        )?;
        if n == 0 {
            return Err(Erro::nao_encontrado("workspace", workspace_id));
        }
        Ok(())
    }

    /// Estado completo do canvas em uma viagem só.
    pub fn estado_canvas(&self, workspace_id: &str) -> Resultado<EstadoCanvas> {
        let workspace = self.obter_workspace(workspace_id)?;
        Ok(EstadoCanvas {
            nos: self.listar_nos(workspace_id)?,
            cabos: self.listar_cabos(workspace_id)?,
            workspace,
        })
    }

    // ----------------------------------------------------------------- nós

    pub fn criar_no(
        &self,
        workspace_id: &str,
        tipo: TipoNo,
        nome: &str,
        x: f64,
        y: f64,
    ) -> Resultado<No> {
        // Falha cedo com mensagem boa em vez de estourar FK lá embaixo.
        self.obter_workspace(workspace_id)?;
        let (w, h) = tipo.tamanho_padrao();
        let t = agora();
        let no = No {
            id: novo_id(),
            workspace_id: workspace_id.to_string(),
            ensaio_id: None,
            tipo,
            nome: se_vazio(nome, nome_padrao(tipo)),
            x,
            y,
            w,
            h,
            z: self.proximo_z(workspace_id)?,
            config: serde_json::json!({}),
            role_id: None,
            recrutado_por: None,
            criado_em: t,
            alterado_em: t,
        };
        self.inserir_no(&no)?;
        Ok(no)
    }

    /// Cria um nó já com papel e com quem o recrutou. É o caminho de
    /// `recrutar`; o `criar_no` acima é o da pessoa clicando na barra.
    pub fn criar_no_recrutado(
        &self,
        workspace_id: &str,
        tipo: TipoNo,
        nome: &str,
        x: f64,
        y: f64,
        role_id: Option<&str>,
        recrutado_por: Option<&str>,
    ) -> Resultado<No> {
        let mut no = self.criar_no(workspace_id, tipo, nome, x, y)?;
        if role_id.is_some() || recrutado_por.is_some() {
            self.conn.execute(
                "UPDATE node SET role_id = ?2, recrutado_por = ?3 WHERE id = ?1",
                params![no.id, role_id, recrutado_por],
            )?;
            no.role_id = role_id.map(String::from);
            no.recrutado_por = recrutado_por.map(String::from);
        }
        Ok(no)
    }

    fn inserir_no(&self, no: &No) -> Resultado<()> {
        self.conn.execute(
            "INSERT INTO node (id, workspace_id, ensaio_id, tipo, nome, x, y, w, h, z,
                               config_json, criado_em, alterado_em, role_id, recrutado_por)
             VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![no.id, no.workspace_id, no.tipo.como_texto(), no.nome,
                    no.x, no.y, no.w, no.h, no.z,
                    no.config.to_string(), no.criado_em, no.alterado_em,
                    no.role_id, no.recrutado_por],
        )?;
        Ok(())
    }

    /// Põe (ou tira, com `None`) o papel de um nó.
    pub fn definir_papel_do_no(&self, node_id: &str, role_id: Option<&str>) -> Resultado<()> {
        if let Some(id) = role_id {
            // Falha cedo e com nome, em vez de estourar FK lá embaixo.
            self.obter_papel(id)?;
        }
        let n = self.conn.execute(
            "UPDATE node SET role_id = ?2, alterado_em = ?3 WHERE id = ?1",
            params![node_id, role_id, agora()],
        )?;
        if n == 0 {
            return Err(Erro::nao_encontrado("nó", node_id));
        }
        Ok(())
    }

    /// Quantos nós de agente o workspace tem. É o teto do
    /// [`MAX_AGENTES_POR_WORKSPACE`] que consulta isto.
    pub fn quantos_agentes(&self, workspace_id: &str) -> Resultado<usize> {
        let n: i64 = self.conn.query_row(
            "SELECT count(*) FROM node WHERE workspace_id = ?1 AND tipo = 'agente'",
            params![workspace_id],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    fn proximo_z(&self, workspace_id: &str) -> Resultado<i64> {
        let z: Option<i64> = self.conn.query_row(
            "SELECT MAX(z) FROM node WHERE workspace_id = ?1",
            params![workspace_id],
            |r| r.get(0),
        )?;
        Ok(z.unwrap_or(0) + 1)
    }

    pub fn listar_nos(&self, workspace_id: &str) -> Resultado<Vec<No>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {COLUNAS_NO} FROM node WHERE workspace_id = ?1 ORDER BY z ASC"
        ))?;
        let linhas = st.query_map(params![workspace_id], le_no)?;
        Ok(linhas.collect::<Result<Vec<_>, _>>()?)
    }

    /// Move e/ou redimensiona. O front chama isto no fim do arrasto, não a
    /// cada frame — ver `useArrasto` no lado TypeScript.
    pub fn mover_no(&self, id: &str, x: f64, y: f64, w: f64, h: f64) -> Resultado<()> {
        if !(x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite()) {
            return Err(Erro::invalido("geometria inválida"));
        }
        if w <= 0.0 || h <= 0.0 {
            return Err(Erro::invalido("largura e altura precisam ser positivas"));
        }
        let n = self.conn.execute(
            "UPDATE node SET x = ?2, y = ?3, w = ?4, h = ?5, alterado_em = ?6 WHERE id = ?1",
            params![id, x, y, w, h, agora()],
        )?;
        if n == 0 {
            return Err(Erro::nao_encontrado("nó", id));
        }
        Ok(())
    }

    /// Guarda o payload específico do tipo. É onde a nota lembra em qual
    /// arquivo ela mora — sem isso, renomear o nó órfãozaria o `.md` no disco.
    pub fn definir_config_no(&self, id: &str, config: &serde_json::Value) -> Resultado<()> {
        let n = self.conn.execute(
            "UPDATE node SET config_json = ?2, alterado_em = ?3 WHERE id = ?1",
            params![id, config.to_string(), agora()],
        )?;
        if n == 0 {
            return Err(Erro::nao_encontrado("nó", id));
        }
        Ok(())
    }

    pub fn renomear_no(&self, id: &str, nome: &str) -> Resultado<()> {
        let nome = nome.trim();
        if nome.is_empty() {
            return Err(Erro::invalido("o nó precisa de um nome"));
        }
        let n = self.conn.execute(
            "UPDATE node SET nome = ?2, alterado_em = ?3 WHERE id = ?1",
            params![id, nome, agora()],
        )?;
        if n == 0 {
            return Err(Erro::nao_encontrado("nó", id));
        }
        Ok(())
    }

    pub fn trazer_para_frente(&self, id: &str) -> Resultado<i64> {
        let ws: String = self
            .conn
            .query_row("SELECT workspace_id FROM node WHERE id = ?1", params![id], |r| r.get(0))
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Erro::nao_encontrado("nó", id),
                outro => Erro::Banco(outro),
            })?;
        let z = self.proximo_z(&ws)?;
        self.conn.execute("UPDATE node SET z = ?2 WHERE id = ?1", params![id, z])?;
        Ok(z)
    }

    /// Remove o nó. Os cabos ligados a ele caem por CASCADE.
    pub fn remover_no(&self, id: &str) -> Resultado<()> {
        let n = self.conn.execute("DELETE FROM node WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(Erro::nao_encontrado("nó", id));
        }
        Ok(())
    }

    // --------------------------------------------------------------- cabos

    pub fn criar_cabo(
        &self,
        workspace_id: &str,
        de: &str,
        para: &str,
        tipo: TipoCabo,
    ) -> Resultado<Cabo> {
        if de == para {
            return Err(Erro::invalido("um nó não se conecta a si mesmo"));
        }
        let cabo = Cabo {
            id: novo_id(),
            workspace_id: workspace_id.to_string(),
            de_node: de.to_string(),
            para_node: para.to_string(),
            tipo,
            criado_em: agora(),
        };
        let r = self.conn.execute(
            "INSERT INTO edge (id, workspace_id, de_node, para_node, tipo, criado_em)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![cabo.id, cabo.workspace_id, cabo.de_node, cabo.para_node,
                    cabo.tipo.como_texto(), cabo.criado_em],
        );
        match r {
            Ok(_) => Ok(cabo),
            // UNIQUE(de,para,tipo) — cabo repetido não é erro para o usuário,
            // é no-op. Mas devolvemos aviso para o front não duplicar desenho.
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Err(Erro::invalido("esses nós já estão ligados desse jeito"))
            }
            Err(e) => Err(Erro::Banco(e)),
        }
    }

    pub fn listar_cabos(&self, workspace_id: &str) -> Resultado<Vec<Cabo>> {
        let mut st = self.conn.prepare(
            "SELECT id, workspace_id, de_node, para_node, tipo, criado_em
             FROM edge WHERE workspace_id = ?1",
        )?;
        let linhas = st.query_map(params![workspace_id], le_cabo)?;
        Ok(linhas.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn remover_cabo(&self, id: &str) -> Resultado<()> {
        let n = self.conn.execute("DELETE FROM edge WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(Erro::nao_encontrado("cabo", id));
        }
        Ok(())
    }

    /// Vizinhos alcançáveis a partir de um nó por um tipo de cabo.
    /// É isto que define o escopo do agente no servidor MCP: ele só
    /// enxerga o que está ligado a ele. Sem isto não há segurança.
    pub fn vizinhos(&self, node_id: &str, tipo: TipoCabo) -> Resultado<Vec<String>> {
        let mut st = self.conn.prepare(
            "SELECT para_node FROM edge WHERE de_node = ?1 AND tipo = ?2
             UNION
             SELECT de_node FROM edge WHERE para_node = ?1 AND tipo = ?2",
        )?;
        let linhas = st.query_map(params![node_id, tipo.como_texto()], |r| r.get::<_, String>(0))?;
        Ok(linhas.collect::<Result<Vec<_>, _>>()?)
    }

    // ------------------------------------------------------------- sessões

    /// Abre uma sessão para um nó. O `token` nasce aqui e não sai daqui para
    /// a interface — só para a configuração MCP do processo do agente.
    pub fn criar_sessao(&self, node_id: &str, adaptador: Adaptador) -> Resultado<Sessao> {
        let no = self.obter_no(node_id)?;
        if no.tipo != TipoNo::Agente {
            return Err(Erro::invalido("só nó de agente abre sessão"));
        }
        let t = agora();
        let s = Sessao {
            id: novo_id(),
            node_id: node_id.to_string(),
            adaptador,
            sessao_externa_id: None,
            estado: EstadoSessao::Ocioso,
            custo_total: 0.0,
            iniciada_em: t,
            ultimo_sinal_em: t,
        };
        self.conn.execute(
            "INSERT INTO session (id, node_id, adaptador, sessao_externa_id, token, estado,
                                  pid, custo_total, iniciada_em, ultimo_sinal_em)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5, NULL, 0, ?6, ?7)",
            params![s.id, s.node_id, s.adaptador.como_texto(), novo_token(),
                    s.estado.como_texto(), s.iniciada_em, s.ultimo_sinal_em],
        )?;
        Ok(s)
    }

    pub fn obter_no(&self, id: &str) -> Resultado<No> {
        let mut st =
            self.conn.prepare(&format!("SELECT {COLUNAS_NO} FROM node WHERE id = ?1"))?;
        st.query_row(params![id], le_no).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Erro::nao_encontrado("nó", id),
            outro => Erro::Banco(outro),
        })
    }

    pub fn obter_sessao(&self, id: &str) -> Resultado<Sessao> {
        let mut st = self.conn.prepare(SELECT_SESSAO_POR_ID)?;
        st.query_row(params![id], le_sessao).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Erro::nao_encontrado("sessão", id),
            outro => Erro::Banco(outro),
        })
    }

    /// A sessão mais recente de um nó, se houver. É o que a face conversa pede
    /// ao abrir: um nó de agente reaberto amanhã continua a mesma conversa.
    pub fn sessao_do_no(&self, node_id: &str) -> Resultado<Option<Sessao>> {
        let mut st = self.conn.prepare(
            "SELECT id, node_id, adaptador, sessao_externa_id, estado, custo_total,
                    iniciada_em, ultimo_sinal_em
             FROM session WHERE node_id = ?1 ORDER BY iniciada_em DESC LIMIT 1",
        )?;
        match st.query_row(params![node_id], le_sessao) {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Erro::Banco(e)),
        }
    }

    /// O segredo do MCP. Método separado, e com este nome, para que qualquer
    /// uso apareça na busca — nada além do caminho do adaptador deve chamá-lo.
    pub fn token_da_sessao(&self, id: &str) -> Resultado<String> {
        self.conn
            .query_row("SELECT token FROM session WHERE id = ?1", params![id], |r| r.get(0))
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Erro::nao_encontrado("sessão", id),
                outro => Erro::Banco(outro),
            })
    }

    /// Muda o estado do turno, recusando transição que a máquina não prevê.
    /// A tabela está em `ESPECIFICACAO.md §6`; transição fora dela é bug do
    /// orquestrador, e é melhor ele estourar aqui do que gravar um estado
    /// impossível que só vai confundir alguém daqui a três meses.
    pub fn mudar_estado_sessao(&self, id: &str, destino: EstadoSessao) -> Resultado<()> {
        let atual = self.obter_sessao(id)?.estado;
        if !atual.pode_ir_para(destino) {
            return Err(Erro::invalido(format!(
                "transição inválida: {} para {}",
                atual.como_texto(),
                destino.como_texto()
            )));
        }
        self.conn.execute(
            "UPDATE session SET estado = ?2, ultimo_sinal_em = ?3 WHERE id = ?1",
            params![id, destino.como_texto(), agora()],
        )?;
        Ok(())
    }

    /// Muda o estado sem consultar a tabela de transições.
    ///
    /// Existe para os dois casos em que a tabela do §6 não tem aresta e a
    /// alternativa seria pior que a exceção: cancelar de `aguardando_no`
    /// (o §6 não prevê, e passar por `erro` mentiria sobre o que houve) e cair
    /// em `erro` de onde quer que o adaptador tenha morrido. Fora daí use
    /// [`Banco::mudar_estado_sessao`] — a checagem é o que impede o
    /// orquestrador de gravar um estado impossível.
    pub fn forcar_estado_sessao(&self, id: &str, destino: EstadoSessao) -> Resultado<()> {
        let n = self.conn.execute(
            "UPDATE session SET estado = ?2, ultimo_sinal_em = ?3 WHERE id = ?1",
            params![id, destino.como_texto(), agora()],
        )?;
        if n == 0 {
            return Err(Erro::nao_encontrado("sessão", id));
        }
        Ok(())
    }

    /// Guarda o id de retomada que o agente devolveu. É isto que permite
    /// fechar o app e continuar a conversa amanhã.
    pub fn definir_sessao_externa(&self, id: &str, externa: &str) -> Resultado<()> {
        let n = self.conn.execute(
            "UPDATE session SET sessao_externa_id = ?2, ultimo_sinal_em = ?3 WHERE id = ?1",
            params![id, externa, agora()],
        )?;
        if n == 0 {
            return Err(Erro::nao_encontrado("sessão", id));
        }
        Ok(())
    }

    /// Bate o coração da sessão. O heartbeat é o que distingue "pensando há
    /// 40 segundos" de "travado desde ontem".
    pub fn marcar_sinal(&self, id: &str) -> Resultado<()> {
        self.conn.execute(
            "UPDATE session SET ultimo_sinal_em = ?2 WHERE id = ?1",
            params![id, agora()],
        )?;
        Ok(())
    }

    pub fn somar_custo(&self, id: &str, custo: f64) -> Resultado<f64> {
        // Custo desconhecido (modelo sem preço na tabela) não vira zero nem
        // envenena o total com NaN: simplesmente não soma.
        if !custo.is_finite() || custo == 0.0 {
            return Ok(self.obter_sessao(id)?.custo_total);
        }
        let n = self.conn.execute(
            "UPDATE session SET custo_total = custo_total + ?2 WHERE id = ?1",
            params![id, custo],
        )?;
        if n == 0 {
            return Err(Erro::nao_encontrado("sessão", id));
        }
        Ok(self.obter_sessao(id)?.custo_total)
    }

    /// Custo do workspace inteiro e a fatia de cada nó. Alimenta o painel de
    /// custo e o teto por workspace.
    pub fn custo_do_workspace(&self, workspace_id: &str) -> Resultado<(f64, Vec<CustoDoNo>)> {
        let mut st = self.conn.prepare(
            "SELECT s.node_id, SUM(s.custo_total)
             FROM session s JOIN node n ON n.id = s.node_id
             WHERE n.workspace_id = ?1
             GROUP BY s.node_id",
        )?;
        let linhas = st.query_map(params![workspace_id], |r| {
            Ok(CustoDoNo { node_id: r.get(0)?, custo: r.get(1)? })
        })?;
        let por_no: Vec<CustoDoNo> = linhas.collect::<Result<Vec<_>, _>>()?;
        Ok((por_no.iter().map(|c| c.custo).sum(), por_no))
    }

    // ----------------------------------------------------------- mensagens

    pub fn gravar_mensagem(
        &self,
        session_id: &str,
        papel: PapelMensagem,
        conteudo: &str,
        uso: Uso,
    ) -> Resultado<Mensagem> {
        self.gravar_mensagem_completa(session_id, papel, conteudo, uso, None, None)
    }

    /// A mesma coisa, mas amarrada a uma cadeia e, quando vem de outro nó, com
    /// a origem. É o `trace_id` que o orçamento por cadeia soma depois.
    pub fn gravar_mensagem_completa(
        &self,
        session_id: &str,
        papel: PapelMensagem,
        conteudo: &str,
        uso: Uso,
        trace_id: Option<&str>,
        origem_node: Option<&str>,
    ) -> Resultado<Mensagem> {
        let m = Mensagem {
            id: novo_id(),
            session_id: session_id.to_string(),
            papel,
            origem_node: origem_node.map(String::from),
            conteudo: conteudo.to_string(),
            tokens: uso.tokens(),
            custo: if uso.custo_usd.is_finite() { uso.custo_usd } else { 0.0 },
            trace_id: trace_id.map(String::from),
            criado_em: agora(),
        };
        self.conn.execute(
            "INSERT INTO message (id, session_id, papel, origem_node, conteudo,
                                  tokens, custo, trace_id, criado_em)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![m.id, m.session_id, m.papel.como_texto(), m.origem_node, m.conteudo,
                    m.tokens, m.custo, m.trace_id, m.criado_em],
        )?;
        Ok(m)
    }

    /// Quanto uma cadeia já custou. É a soma que o teto por trace compara —
    /// o índice `idx_message_trace` existe desde a 001 justamente para isto.
    pub fn custo_do_trace(&self, trace_id: &str) -> Resultado<f64> {
        let total: Option<f64> = self.conn.query_row(
            "SELECT SUM(custo) FROM message WHERE trace_id = ?1",
            params![trace_id],
            |r| r.get(0),
        )?;
        Ok(total.unwrap_or(0.0))
    }

    /// Histórico em ordem cronológica. `limite` existe porque uma conversa
    /// longa não cabe — e não precisa caber — numa viagem de IPC só.
    pub fn historico(&self, session_id: &str, limite: i64) -> Resultado<Vec<Mensagem>> {
        let mut st = self.conn.prepare(
            "SELECT id, session_id, papel, origem_node, conteudo, tokens, custo,
                    trace_id, criado_em
             FROM message WHERE session_id = ?1
             ORDER BY criado_em DESC, rowid DESC LIMIT ?2",
        )?;
        let linhas = st.query_map(params![session_id, limite], le_mensagem)?;
        let mut v: Vec<Mensagem> = linhas.collect::<Result<Vec<_>, _>>()?;
        // Buscamos do fim para trás para pegar as N mais recentes; a interface
        // quer ler de cima para baixo.
        v.reverse();
        Ok(v)
    }

    // -------------------------------------------------------- ferramentas

    /// Registra o pedido de ferramenta. Vira linha antes de executar, porque
    /// `tool_call` é o log de auditoria: o que foi tentado conta tanto quanto
    /// o que deu certo.
    pub fn gravar_ferramenta_pedida(
        &self,
        session_id: &str,
        id_externo: &str,
        ferramenta: &str,
        argumentos: &serde_json::Value,
        aprovacao: Aprovacao,
    ) -> Resultado<ChamadaFerramenta> {
        let c = ChamadaFerramenta {
            // O id vem do agente e só é único dentro do turno dele. A chave
            // primária junta sessão e id externo para dois nós não colidirem.
            id: format!("{session_id}:{id_externo}"),
            session_id: session_id.to_string(),
            ferramenta: ferramenta.to_string(),
            argumentos: argumentos.clone(),
            resultado: None,
            erro: None,
            aprovacao,
            decidido_por: None,
            criado_em: agora(),
        };
        // O mesmo pedido chega por dois caminhos: o evento do stream (que o
        // adaptador traduz) e o hook de aprovação (que chega pelo barramento),
        // em ordem que não dá para garantir. Quem chegar primeiro cria a linha;
        // o segundo não pode rebaixar uma pendente para automática — daí o
        // ON CONFLICT dividido entre este método e `gravar_ferramenta_pendente`.
        self.conn.execute(
            "INSERT INTO tool_call (id, session_id, ferramenta, argumentos_json,
                                    resultado_json, erro, aprovacao, decidido_por,
                                    decidido_em, criado_em)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5, NULL, NULL, ?6)
             ON CONFLICT(id) DO NOTHING",
            params![c.id, c.session_id, c.ferramenta, c.argumentos.to_string(),
                    c.aprovacao.como_texto(), c.criado_em],
        )?;
        Ok(c)
    }

    pub fn concluir_ferramenta(
        &self,
        session_id: &str,
        id_externo: &str,
        resultado: Option<&serde_json::Value>,
        erro: Option<&str>,
    ) -> Resultado<()> {
        let id = format!("{session_id}:{id_externo}");
        let n = self.conn.execute(
            "UPDATE tool_call SET resultado_json = ?2, erro = ?3 WHERE id = ?1",
            params![id, resultado.map(|r| r.to_string()), erro],
        )?;
        if n == 0 {
            return Err(Erro::nao_encontrado("chamada de ferramenta", id));
        }
        Ok(())
    }

    /// Resolve o segredo do MCP para a sessão que o carrega.
    ///
    /// É o coração do escopo do `ESPECIFICACAO.md §4`: token → sessão → nó.
    /// Um token que não resolve não recebe "existe mas você não pode" — recebe
    /// nada, e quem chamou não descobre se o token existia.
    pub fn sessao_por_token(&self, token: &str) -> Resultado<Sessao> {
        let mut st = self.conn.prepare(
            "SELECT id, node_id, adaptador, sessao_externa_id, estado, custo_total,
                    iniciada_em, ultimo_sinal_em
             FROM session WHERE token = ?1",
        )?;
        st.query_row(params![token], le_sessao).map_err(|e| match e {
            // Nunca ecoa o token na mensagem: ele vai para log e log vaza.
            rusqlite::Error::QueryReturnedNoRows => Erro::nao_encontrado("sessão", "token"),
            outro => Erro::Banco(outro),
        })
    }

    /// Registra o pedido e o deixa esperando gente.
    ///
    /// Ao contrário de `gravar_ferramenta_pedida`, este **sobrepõe** o que já
    /// estiver lá: se o evento do stream chegou antes e gravou "automatica", a
    /// verdade é que o hook está segurando o agente, e é essa que vale.
    pub fn gravar_ferramenta_pendente(
        &self,
        session_id: &str,
        id_externo: &str,
        ferramenta: &str,
        argumentos: &serde_json::Value,
    ) -> Resultado<ChamadaFerramenta> {
        let c = ChamadaFerramenta {
            id: format!("{session_id}:{id_externo}"),
            session_id: session_id.to_string(),
            ferramenta: ferramenta.to_string(),
            argumentos: argumentos.clone(),
            resultado: None,
            erro: None,
            aprovacao: Aprovacao::Pendente,
            decidido_por: None,
            criado_em: agora(),
        };
        self.conn.execute(
            "INSERT INTO tool_call (id, session_id, ferramenta, argumentos_json,
                                    resultado_json, erro, aprovacao, decidido_por,
                                    decidido_em, criado_em)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, 'pendente', NULL, NULL, ?5)
             ON CONFLICT(id) DO UPDATE SET
                 aprovacao = 'pendente',
                 argumentos_json = excluded.argumentos_json",
            params![c.id, c.session_id, c.ferramenta, c.argumentos.to_string(), c.criado_em],
        )?;
        Ok(c)
    }

    /// Fecha um pedido de aprovação. `decidido_por` é `usuario` ou
    /// `regra:<ferramenta>`, e vai para o log de auditoria junto com a hora.
    pub fn decidir_ferramenta(
        &self,
        tool_call_id: &str,
        decisao: Decisao,
        decidido_por: &str,
    ) -> Resultado<()> {
        let aprovacao = match decisao {
            Decisao::Aprovada => Aprovacao::Aprovada,
            Decisao::Negada => Aprovacao::Negada,
        };
        let n = self.conn.execute(
            "UPDATE tool_call SET aprovacao = ?2, decidido_por = ?3, decidido_em = ?4
             WHERE id = ?1 AND aprovacao = 'pendente'",
            params![tool_call_id, aprovacao.como_texto(), decidido_por, agora()],
        )?;
        if n == 0 {
            // Ou não existe, ou já foi decidido. Os dois casos são a mesma
            // coisa para quem chamou: não há o que decidir agora.
            return Err(Erro::nao_encontrado("aprovação pendente", tool_call_id));
        }
        Ok(())
    }

    pub fn obter_ferramenta(&self, id: &str) -> Resultado<ChamadaFerramenta> {
        let mut st = self.conn.prepare(
            "SELECT id, session_id, ferramenta, argumentos_json, resultado_json, erro,
                    aprovacao, decidido_por, criado_em
             FROM tool_call WHERE id = ?1",
        )?;
        st.query_row(params![id], le_ferramenta).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                Erro::nao_encontrado("chamada de ferramenta", id)
            }
            outro => Erro::Banco(outro),
        })
    }

    // ------------------------------------------------------------- regras

    /// Concede "não perguntar de novo" para uma ferramenta neste workspace.
    /// Conceder duas vezes não cria duas linhas — revogar precisa apagar tudo.
    pub fn conceder_regra(
        &self,
        workspace_id: &str,
        ferramenta: &str,
    ) -> Resultado<RegraAprovacao> {
        // Rodar comando e buscar na web nunca viram permanentes. "Não
        // perguntar de novo" para gravar nesta pasta é uma decisão sobre uma
        // pasta; para `Bash` seria uma decisão sobre a máquina inteira, tomada
        // uma vez com um clique e esquecida no dia seguinte.
        if !crate::barramento::aceita_regra(ferramenta) {
            return Err(Erro::invalido(format!(
                "{ferramenta} pergunta sempre: uma licença permanente para isso \
                 valeria pela máquina toda, não só por esta pasta"
            )));
        }
        self.obter_workspace(workspace_id)?;
        if let Some(ja) = self.regra_para(workspace_id, ferramenta)? {
            return Ok(ja);
        }
        let r = RegraAprovacao {
            id: novo_id(),
            workspace_id: workspace_id.to_string(),
            ferramenta: ferramenta.to_string(),
            criado_em: agora(),
        };
        self.conn.execute(
            "INSERT INTO regra_aprovacao (id, workspace_id, ferramenta, criado_em)
             VALUES (?1, ?2, ?3, ?4)",
            params![r.id, r.workspace_id, r.ferramenta, r.criado_em],
        )?;
        Ok(r)
    }

    pub fn regra_para(
        &self,
        workspace_id: &str,
        ferramenta: &str,
    ) -> Resultado<Option<RegraAprovacao>> {
        let mut st = self.conn.prepare(
            "SELECT id, workspace_id, ferramenta, criado_em
             FROM regra_aprovacao WHERE workspace_id = ?1 AND ferramenta = ?2",
        )?;
        match st.query_row(params![workspace_id, ferramenta], le_regra) {
            Ok(r) => Ok(Some(r)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Erro::Banco(e)),
        }
    }

    pub fn listar_regras(&self, workspace_id: &str) -> Resultado<Vec<RegraAprovacao>> {
        let mut st = self.conn.prepare(
            "SELECT id, workspace_id, ferramenta, criado_em
             FROM regra_aprovacao WHERE workspace_id = ?1 ORDER BY criado_em ASC",
        )?;
        let linhas = st.query_map(params![workspace_id], le_regra)?;
        Ok(linhas.collect::<Result<Vec<_>, _>>()?)
    }

    /// Toda permissão concedida precisa ser revogável — `ARQUITETURA.md §8`.
    pub fn revogar_regra(&self, id: &str) -> Resultado<()> {
        let n = self.conn.execute("DELETE FROM regra_aprovacao WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(Erro::nao_encontrado("regra", id));
        }
        Ok(())
    }

    pub fn ferramentas_da_sessao(&self, session_id: &str) -> Resultado<Vec<ChamadaFerramenta>> {
        let mut st = self.conn.prepare(
            "SELECT id, session_id, ferramenta, argumentos_json, resultado_json, erro,
                    aprovacao, decidido_por, criado_em
             FROM tool_call WHERE session_id = ?1 ORDER BY criado_em ASC, rowid ASC",
        )?;
        let linhas = st.query_map(params![session_id], le_ferramenta)?;
        Ok(linhas.collect::<Result<Vec<_>, _>>()?)
    }

    // --------------------------------------------------------------- papéis

    pub fn criar_papel(
        &self,
        nome: &str,
        prompt: &str,
        ferramentas: &[String],
        autonomia: Autonomia,
        modelo: Option<&str>,
        embutido: bool,
    ) -> Resultado<Papel> {
        let nome = nome.trim();
        if nome.is_empty() {
            return Err(Erro::invalido("o papel precisa de um nome"));
        }
        if prompt.trim().is_empty() {
            return Err(Erro::invalido(
                "um papel sem prompt é um agente sem papel — escreva o que ele é",
            ));
        }
        let p = Papel {
            id: novo_id(),
            nome: nome.to_string(),
            prompt: prompt.trim().to_string(),
            ferramentas: ferramentas.to_vec(),
            autonomia,
            modelo: modelo.map(String::from),
            embutido,
            criado_em: agora(),
            mcp: Vec::new(),
        };
        self.conn
            .execute(
                "INSERT INTO role (id, nome, prompt, ferramentas_json, autonomia, modelo,
                                   embutido, criado_em, mcp_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, '[]')",
                params![p.id, p.nome, p.prompt, serde_json::to_string(&p.ferramentas)?,
                        p.autonomia.como_texto(), p.modelo, p.embutido as i64, p.criado_em],
            )
            .map_err(|e| match e {
                rusqlite::Error::SqliteFailure(f, _)
                    if f.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    Erro::invalido(format!("já existe um papel chamado \"{}\"", p.nome))
                }
                outro => Erro::Banco(outro),
            })?;
        Ok(p)
    }

    pub fn obter_papel(&self, id: &str) -> Resultado<Papel> {
        let mut st =
            self.conn.prepare(&format!("SELECT {COLUNAS_PAPEL} FROM role WHERE id = ?1"))?;
        st.query_row(params![id], le_papel).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Erro::nao_encontrado("papel", id),
            outro => Erro::Banco(outro),
        })
    }

    /// Papel pelo nome, comparando sem caixa. É por aqui que `recrutar` e a
    /// abertura de partitura resolvem "Redator" — nome é o que a pessoa e o
    /// modelo escrevem; id é detalhe interno.
    pub fn papel_por_nome(&self, nome: &str) -> Resultado<Option<Papel>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {COLUNAS_PAPEL} FROM role WHERE lower(nome) = lower(?1)"
        ))?;
        match st.query_row(params![nome.trim()], le_papel) {
            Ok(p) => Ok(Some(p)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Erro::Banco(e)),
        }
    }

    pub fn listar_papeis(&self) -> Resultado<Vec<Papel>> {
        // Embutidos primeiro: a biblioteca que veio com o app é o que o
        // usuário procura na maior parte das vezes.
        let mut st = self.conn.prepare(&format!(
            "SELECT {COLUNAS_PAPEL} FROM role ORDER BY embutido DESC, nome ASC"
        ))?;
        let linhas = st.query_map([], le_papel)?;
        Ok(linhas.collect::<Result<Vec<_>, _>>()?)
    }

    /// Liga (ou desliga, com lista vazia) servidores MCP externos num papel.
    ///
    /// Toda ferramenta vinda deles passa pelo card, sempre e sem "não
    /// perguntar de novo": elas saem da máquina, e o `ARQUITETURA.md §8` é
    /// explícito em que ação externa sempre pede aprovação, em qualquer nível
    /// de autonomia. Ver `barramento::e_de_fora`.
    pub fn definir_mcp_do_papel(&self, id: &str, servidores: &[ServidorMcp]) -> Resultado<Papel> {
        for s in servidores {
            // O nome vira parte do nome da ferramenta que o modelo vê
            // (`mcp__crm__buscar`) **e do matcher do hook, que é uma regex**.
            // Um ponto, um asterisco ou uma barra vertical aqui viraria
            // curinga no matcher e abriria buraco na aprovação sem ninguém
            // perceber. Recusar na entrada é o único lugar em que isso é
            // barato.
            let nome_ok = !s.nome.trim().is_empty()
                && s.nome.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !nome_ok {
                return Err(Erro::invalido(format!(
                    "\"{}\" não serve como nome de servidor: use só letras, números e _",
                    s.nome
                )));
            }
            if s.url.trim().is_empty() {
                return Err(Erro::invalido("o servidor precisa de um endereço"));
            }
        }
        let n = self.conn.execute(
            "UPDATE role SET mcp_json = ?2 WHERE id = ?1",
            params![id, serde_json::to_string(&servidores.to_vec())?],
        )?;
        if n == 0 {
            return Err(Erro::nao_encontrado("papel", id));
        }
        self.obter_papel(id)
    }

    /// Edita um papel. Embutido também se edita — o que não se pode é apagar.
    pub fn editar_papel(
        &self,
        id: &str,
        prompt: &str,
        ferramentas: &[String],
        autonomia: Autonomia,
        modelo: Option<&str>,
    ) -> Resultado<Papel> {
        if prompt.trim().is_empty() {
            return Err(Erro::invalido("um papel sem prompt é um agente sem papel"));
        }
        let n = self.conn.execute(
            "UPDATE role SET prompt = ?2, ferramentas_json = ?3, autonomia = ?4, modelo = ?5
             WHERE id = ?1",
            params![id, prompt.trim(), serde_json::to_string(&ferramentas.to_vec())?,
                    autonomia.como_texto(), modelo],
        )?;
        if n == 0 {
            return Err(Erro::nao_encontrado("papel", id));
        }
        self.obter_papel(id)
    }

    /// Apaga um papel. Os nós que o usavam ficam sem papel (`ON DELETE SET
    /// NULL`), e não somem junto: apagar um papel não pode levar a conversa.
    pub fn remover_papel(&self, id: &str) -> Resultado<()> {
        let p = self.obter_papel(id)?;
        if p.embutido {
            // Recusar em vez de apagar é o certo: o usuário duplica e edita a
            // cópia. Um embutido apagado voltaria na próxima subida do app, e
            // "apaguei e voltou" é pior que "não dá para apagar".
            return Err(Erro::invalido(format!(
                "\"{}\" veio com o app e não dá para apagar. Duplique e edite a cópia.",
                p.nome
            )));
        }
        self.conn.execute("DELETE FROM role WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Quantos nós usam este papel. O front mostra antes de apagar.
    pub fn quantos_usam_o_papel(&self, id: &str) -> Resultado<usize> {
        let n: i64 = self.conn.query_row(
            "SELECT count(*) FROM node WHERE role_id = ?1",
            params![id],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    // -------------------------------------------------------------- ensaios

    pub fn criar_ensaio(
        &self,
        workspace_id: &str,
        nome: &str,
        branch: &str,
        caminho_worktree: &str,
        base_commit: Option<&str>,
    ) -> Resultado<Ensaio> {
        let nome = nome.trim();
        if nome.is_empty() {
            return Err(Erro::invalido("o rascunho precisa de um nome"));
        }
        self.obter_workspace(workspace_id)?;
        let t = agora();
        let e = Ensaio {
            id: novo_id(),
            workspace_id: workspace_id.to_string(),
            nome: nome.to_string(),
            branch: branch.to_string(),
            caminho_worktree: caminho_worktree.to_string(),
            base_commit: base_commit.map(String::from),
            estado: EstadoEnsaio::Aberto,
            criado_em: t,
            alterado_em: t,
        };
        self.conn
            .execute(
                "INSERT INTO ensaio (id, workspace_id, nome, branch, caminho_worktree,
                                     base_commit, estado, criado_em, alterado_em)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![e.id, e.workspace_id, e.nome, e.branch, e.caminho_worktree,
                        e.base_commit, e.estado.como_texto(), e.criado_em, e.alterado_em],
            )
            .map_err(|erro| match erro {
                rusqlite::Error::SqliteFailure(f, _)
                    if f.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    Erro::invalido(format!("já existe um rascunho chamado \"{}\"", e.nome))
                }
                outro => Erro::Banco(outro),
            })?;
        Ok(e)
    }

    pub fn obter_ensaio(&self, id: &str) -> Resultado<Ensaio> {
        let mut st =
            self.conn.prepare(&format!("SELECT {COLUNAS_ENSAIO} FROM ensaio WHERE id = ?1"))?;
        st.query_row(params![id], le_ensaio).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Erro::nao_encontrado("rascunho", id),
            outro => Erro::Banco(outro),
        })
    }

    /// Os rascunhos de um workspace, os abertos primeiro. Publicado e
    /// descartado ficam na lista de propósito: "o que aconteceu com aquele
    /// rascunho?" precisa ter resposta.
    pub fn listar_ensaios(&self, workspace_id: &str) -> Resultado<Vec<Ensaio>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {COLUNAS_ENSAIO} FROM ensaio WHERE workspace_id = ?1
             ORDER BY (estado = 'aberto') DESC, alterado_em DESC"
        ))?;
        let linhas = st.query_map(params![workspace_id], le_ensaio)?;
        Ok(linhas.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn mudar_estado_ensaio(&self, id: &str, estado: EstadoEnsaio) -> Resultado<()> {
        let n = self.conn.execute(
            "UPDATE ensaio SET estado = ?2, alterado_em = ?3 WHERE id = ?1",
            params![id, estado.como_texto(), agora()],
        )?;
        if n == 0 {
            return Err(Erro::nao_encontrado("rascunho", id));
        }
        Ok(())
    }

    /// Põe (ou tira, com `None`) o rascunho em foco.
    ///
    /// Quem chama **precisa** derrubar os adaptadores vivos depois — ver
    /// `ensaios::trocar`. Um processo de agente já aberto guarda a pasta antiga
    /// no diretório de trabalho dele, e ninguém avisa o processo de nada.
    pub fn definir_ensaio_ativo(&self, workspace_id: &str, ensaio: Option<&str>) -> Resultado<()> {
        if let Some(id) = ensaio {
            let e = self.obter_ensaio(id)?;
            if e.workspace_id != workspace_id {
                return Err(Erro::invalido("esse rascunho é de outro workspace"));
            }
            if e.estado != EstadoEnsaio::Aberto {
                return Err(Erro::invalido(format!(
                    "o rascunho \"{}\" já foi {}",
                    e.nome,
                    e.estado.como_texto()
                )));
            }
        }
        let n = self.conn.execute(
            "UPDATE workspace SET ensaio_ativo = ?2 WHERE id = ?1",
            params![workspace_id, ensaio],
        )?;
        if n == 0 {
            return Err(Erro::nao_encontrado("workspace", workspace_id));
        }
        Ok(())
    }

    // ----------------------------------------------------------- partituras

    pub fn salvar_partitura(
        &self,
        workspace_id: &str,
        nome: &str,
        snapshot: &Snapshot,
    ) -> Resultado<Partitura> {
        let nome = nome.trim();
        if nome.is_empty() {
            return Err(Erro::invalido("o time precisa de um nome para você achar depois"));
        }
        if snapshot.nos.is_empty() {
            return Err(Erro::invalido("não dá para salvar um time sem ninguém nele"));
        }
        self.obter_workspace(workspace_id)?;
        let p = Partitura {
            id: novo_id(),
            workspace_id: workspace_id.to_string(),
            nome: nome.to_string(),
            snapshot: snapshot.clone(),
            criado_em: agora(),
        };
        // Salvar duas vezes com o mesmo nome substitui, e não dá erro: o
        // usuário que repete o nome está atualizando o time, não descobrindo
        // um índice único.
        self.conn.execute(
            "INSERT INTO partitura (id, workspace_id, nome, snapshot_json, criado_em)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(workspace_id, nome) DO UPDATE SET
                 snapshot_json = excluded.snapshot_json,
                 criado_em     = excluded.criado_em",
            params![p.id, p.workspace_id, p.nome,
                    serde_json::to_string(&p.snapshot)?, p.criado_em],
        )?;
        // O ON CONFLICT pode ter atualizado a linha antiga, que guarda o id
        // dela. Reler é o que devolve a verdade em vez do que tentamos gravar.
        self.partitura_por_nome(workspace_id, nome)?
            .ok_or_else(|| Erro::invalido("a partitura sumiu logo depois de salva"))
    }

    pub fn partitura_por_nome(
        &self,
        workspace_id: &str,
        nome: &str,
    ) -> Resultado<Option<Partitura>> {
        let mut st = self.conn.prepare(
            "SELECT id, workspace_id, nome, snapshot_json, criado_em
             FROM partitura WHERE workspace_id = ?1 AND nome = ?2",
        )?;
        match st.query_row(params![workspace_id, nome.trim()], le_partitura) {
            Ok(p) => Ok(Some(p)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(Erro::Banco(e)),
        }
    }

    pub fn obter_partitura(&self, id: &str) -> Resultado<Partitura> {
        let mut st = self.conn.prepare(
            "SELECT id, workspace_id, nome, snapshot_json, criado_em FROM partitura WHERE id = ?1",
        )?;
        st.query_row(params![id], le_partitura).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Erro::nao_encontrado("time salvo", id),
            outro => Erro::Banco(outro),
        })
    }

    pub fn listar_partituras(&self, workspace_id: &str) -> Resultado<Vec<Partitura>> {
        let mut st = self.conn.prepare(
            "SELECT id, workspace_id, nome, snapshot_json, criado_em
             FROM partitura WHERE workspace_id = ?1 ORDER BY criado_em DESC",
        )?;
        let linhas = st.query_map(params![workspace_id], le_partitura)?;
        Ok(linhas.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn remover_partitura(&self, id: &str) -> Resultado<()> {
        let n = self.conn.execute("DELETE FROM partitura WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(Erro::nao_encontrado("time salvo", id));
        }
        Ok(())
    }
}

const SELECT_SESSAO_POR_ID: &str =
    "SELECT id, node_id, adaptador, sessao_externa_id, estado, custo_total,
            iniciada_em, ultimo_sinal_em
     FROM session WHERE id = ?1";

/// 32 bytes de aleatoriedade em hexadecimal, como manda `ESPECIFICACAO.md §4`.
///
/// Dois UUID v4 em vez de um: cada um carrega 122 bits de aleatoriedade do
/// gerador do sistema operacional, e um sozinho daria 16 bytes. Não é uma
/// escolha estética — este token é a única coisa entre um agente e o canvas
/// inteiro dos outros.
fn novo_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

// ------------------------------------------------------------------ leitura

fn le_workspace(r: &Row) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: r.get(0)?,
        nome: r.get(1)?,
        pasta: r.get(2)?,
        criado_em: r.get(3)?,
        ensaio_ativo: r.get(4)?,
        viewport: Viewport { x: r.get(5)?, y: r.get(6)?, zoom: r.get(7)? },
        repo: r.get(8)?,
    })
}

/// Colunas do nó, na ordem que [`le_no`] espera. Uma constante só porque três
/// consultas liam a mesma lista à mão, e a 004 teve de mexer nas três.
const COLUNAS_NO: &str = "id, workspace_id, ensaio_id, tipo, nome, x, y, w, h, z,
                          config_json, criado_em, alterado_em, role_id, recrutado_por";

fn le_no(r: &Row) -> rusqlite::Result<No> {
    let tipo_txt: String = r.get(3)?;
    let config_txt: String = r.get(10)?;
    Ok(No {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        ensaio_id: r.get(2)?,
        // O CHECK do banco já garante o domínio; se chegou lixo aqui, o banco
        // foi editado por fora. Cair para Forma é melhor que derrubar o app.
        tipo: TipoNo::do_texto(&tipo_txt).unwrap_or(TipoNo::Forma),
        nome: r.get(4)?,
        x: r.get(5)?,
        y: r.get(6)?,
        w: r.get(7)?,
        h: r.get(8)?,
        z: r.get(9)?,
        config: serde_json::from_str(&config_txt).unwrap_or_else(|_| serde_json::json!({})),
        criado_em: r.get(11)?,
        alterado_em: r.get(12)?,
        role_id: r.get(13)?,
        recrutado_por: r.get(14)?,
    })
}

fn le_papel(r: &Row) -> rusqlite::Result<Papel> {
    let ferramentas: String = r.get(3)?;
    let autonomia: String = r.get(4)?;
    Ok(Papel {
        id: r.get(0)?,
        nome: r.get(1)?,
        prompt: r.get(2)?,
        ferramentas: serde_json::from_str(&ferramentas).unwrap_or_default(),
        autonomia: Autonomia::do_texto(&autonomia).unwrap_or(Autonomia::Cauteloso),
        modelo: r.get(5)?,
        embutido: r.get::<_, i64>(6)? != 0,
        criado_em: r.get(7)?,
        // JSON quebrado vira lista vazia, não papel quebrado: um servidor
        // externo malformado não pode impedir o agente de trabalhar.
        mcp: serde_json::from_str(&r.get::<_, String>(8)?).unwrap_or_default(),
    })
}

const COLUNAS_PAPEL: &str =
    "id, nome, prompt, ferramentas_json, autonomia, modelo, embutido, criado_em, mcp_json";

const COLUNAS_ENSAIO: &str = "id, workspace_id, nome, branch, caminho_worktree,
                              base_commit, estado, criado_em, alterado_em";

fn le_ensaio(r: &Row) -> rusqlite::Result<Ensaio> {
    let estado: String = r.get(6)?;
    Ok(Ensaio {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        nome: r.get(2)?,
        branch: r.get(3)?,
        caminho_worktree: r.get(4)?,
        base_commit: r.get(5)?,
        // O CHECK do banco já garante o domínio. Lixo aqui quer dizer banco
        // editado por fora; tratar como descartado é o desfecho seguro — um
        // rascunho que não se sabe o que é não pode virar o ativo.
        estado: EstadoEnsaio::do_texto(&estado).unwrap_or(EstadoEnsaio::Descartado),
        criado_em: r.get(7)?,
        alterado_em: r.get(8)?,
    })
}

fn le_partitura(r: &Row) -> rusqlite::Result<Partitura> {
    let snapshot: String = r.get(3)?;
    Ok(Partitura {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        nome: r.get(2)?,
        // Snapshot ilegível vira time vazio em vez de derrubar a listagem: uma
        // partitura corrompida não pode esconder as outras.
        snapshot: serde_json::from_str(&snapshot).unwrap_or_default(),
        criado_em: r.get(4)?,
    })
}

/// Ordem de colunas: id, node_id, adaptador, sessao_externa_id, estado,
/// custo_total, iniciada_em, ultimo_sinal_em. `token` fica **fora** de
/// propósito — o que não é lido não vaza.
fn le_sessao(r: &Row) -> rusqlite::Result<Sessao> {
    let adaptador: String = r.get(2)?;
    let estado: String = r.get(4)?;
    Ok(Sessao {
        id: r.get(0)?,
        node_id: r.get(1)?,
        adaptador: Adaptador::do_texto(&adaptador).unwrap_or(Adaptador::Claude),
        sessao_externa_id: r.get(3)?,
        // Estado ilegível é sessão que não dá para retomar com segurança.
        // Cair para Erro deixa o nó pedindo atenção, que é a verdade.
        estado: EstadoSessao::do_texto(&estado).unwrap_or(EstadoSessao::Erro),
        custo_total: r.get(5)?,
        iniciada_em: r.get(6)?,
        ultimo_sinal_em: r.get(7)?,
    })
}

fn le_mensagem(r: &Row) -> rusqlite::Result<Mensagem> {
    let papel: String = r.get(2)?;
    Ok(Mensagem {
        id: r.get(0)?,
        session_id: r.get(1)?,
        papel: PapelMensagem::do_texto(&papel).unwrap_or(PapelMensagem::Sistema),
        origem_node: r.get(3)?,
        conteudo: r.get(4)?,
        tokens: r.get(5)?,
        custo: r.get(6)?,
        trace_id: r.get(7)?,
        criado_em: r.get(8)?,
    })
}

fn le_ferramenta(r: &Row) -> rusqlite::Result<ChamadaFerramenta> {
    let argumentos: String = r.get(3)?;
    let resultado: Option<String> = r.get(4)?;
    let aprovacao: String = r.get(6)?;
    Ok(ChamadaFerramenta {
        id: r.get(0)?,
        session_id: r.get(1)?,
        ferramenta: r.get(2)?,
        argumentos: serde_json::from_str(&argumentos).unwrap_or_else(|_| serde_json::json!({})),
        resultado: resultado.and_then(|t| serde_json::from_str(&t).ok()),
        erro: r.get(5)?,
        aprovacao: Aprovacao::do_texto(&aprovacao).unwrap_or(Aprovacao::Automatica),
        decidido_por: r.get(7)?,
        criado_em: r.get(8)?,
    })
}

fn le_regra(r: &Row) -> rusqlite::Result<RegraAprovacao> {
    Ok(RegraAprovacao {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        ferramenta: r.get(2)?,
        criado_em: r.get(3)?,
    })
}

fn le_cabo(r: &Row) -> rusqlite::Result<Cabo> {
    let tipo_txt: String = r.get(4)?;
    Ok(Cabo {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        de_node: r.get(2)?,
        para_node: r.get(3)?,
        tipo: TipoCabo::do_texto(&tipo_txt).unwrap_or(TipoCabo::FalaCom),
        criado_em: r.get(5)?,
    })
}

// ------------------------------------------------------------------ ajudas

fn se_vazio(valor: &str, padrao: &str) -> String {
    let v = valor.trim();
    if v.is_empty() { padrao.to_string() } else { v.to_string() }
}

fn nome_padrao(tipo: TipoNo) -> &'static str {
    match tipo {
        TipoNo::Agente => "Agente",
        TipoNo::Nota => "Nota",
        TipoNo::Arquivos => "Arquivos",
        TipoNo::Portal => "Portal",
        TipoNo::Forma => "Forma",
    }
}
