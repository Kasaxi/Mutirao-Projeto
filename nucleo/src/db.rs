use crate::erro::{Erro, Resultado};
use crate::modelo::*;
use rusqlite::{params, Connection, Row};
use std::path::Path;

/// Migrations embutidas no binário. Para adicionar uma, some um item aqui —
/// nunca edite um arquivo já publicado, mesmo para corrigir. O índice do
/// vetor + 1 é a versão gravada em `PRAGMA user_version`.
const MIGRATIONS: &[&str] = &[include_str!("../migrations/001_inicial.sql")];

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
        Ok(banco)
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
            let tx = self.conn.transaction()?;
            tx.execute_batch(MIGRATIONS[i as usize])?;
            tx.pragma_update(None, "user_version", i + 1)?;
            tx.commit()?;
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
