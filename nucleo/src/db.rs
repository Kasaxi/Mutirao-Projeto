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
            "SELECT id, nome, pasta, criado_em, ensaio_ativo, vp_x, vp_y, vp_zoom
             FROM workspace ORDER BY criado_em DESC",
        )?;
        let linhas = st.query_map([], le_workspace)?;
        Ok(linhas.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn obter_workspace(&self, id: &str) -> Resultado<Workspace> {
        let mut st = self.conn.prepare(
            "SELECT id, nome, pasta, criado_em, ensaio_ativo, vp_x, vp_y, vp_zoom
             FROM workspace WHERE id = ?1",
        )?;
        st.query_row(params![id], le_workspace)
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Erro::nao_encontrado("workspace", id),
                outro => Erro::Banco(outro),
            })
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
            criado_em: t,
            alterado_em: t,
        };
        self.conn.execute(
            "INSERT INTO node (id, workspace_id, ensaio_id, tipo, nome, x, y, w, h, z,
                               config_json, criado_em, alterado_em)
             VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![no.id, no.workspace_id, no.tipo.como_texto(), no.nome,
                    no.x, no.y, no.w, no.h, no.z,
                    no.config.to_string(), no.criado_em, no.alterado_em],
        )?;
        Ok(no)
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
        let mut st = self.conn.prepare(
            "SELECT id, workspace_id, ensaio_id, tipo, nome, x, y, w, h, z,
                    config_json, criado_em, alterado_em
             FROM node WHERE workspace_id = ?1 ORDER BY z ASC",
        )?;
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
        let mut st = self.conn.prepare(
            "SELECT id, workspace_id, ensaio_id, tipo, nome, x, y, w, h, z,
                    config_json, criado_em, alterado_em
             FROM node WHERE id = ?1",
        )?;
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
        let m = Mensagem {
            id: novo_id(),
            session_id: session_id.to_string(),
            papel,
            origem_node: None,
            conteudo: conteudo.to_string(),
            tokens: uso.tokens(),
            custo: if uso.custo_usd.is_finite() { uso.custo_usd } else { 0.0 },
            trace_id: None,
            criado_em: agora(),
        };
        self.conn.execute(
            "INSERT INTO message (id, session_id, papel, origem_node, conteudo,
                                  tokens, custo, trace_id, criado_em)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, NULL, ?7)",
            params![m.id, m.session_id, m.papel.como_texto(), m.conteudo,
                    m.tokens, m.custo, m.criado_em],
        )?;
        Ok(m)
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
        self.conn.execute(
            "INSERT INTO tool_call (id, session_id, ferramenta, argumentos_json,
                                    resultado_json, erro, aprovacao, decidido_por,
                                    decidido_em, criado_em)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5, NULL, NULL, ?6)",
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

    pub fn ferramentas_da_sessao(&self, session_id: &str) -> Resultado<Vec<ChamadaFerramenta>> {
        let mut st = self.conn.prepare(
            "SELECT id, session_id, ferramenta, argumentos_json, resultado_json, erro,
                    aprovacao, decidido_por, criado_em
             FROM tool_call WHERE session_id = ?1 ORDER BY criado_em ASC, rowid ASC",
        )?;
        let linhas = st.query_map(params![session_id], le_ferramenta)?;
        Ok(linhas.collect::<Result<Vec<_>, _>>()?)
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
    })
}

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
