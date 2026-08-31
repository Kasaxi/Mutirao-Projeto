//! Núcleo do Mutirão.
//!
//! Crate puro: modelo de domínio, banco e regras. Não conhece Tauri, não
//! conhece WebView, não abre janela. Isso é de propósito — dá para rodar
//! `cargo test -p nucleo` em qualquer máquina, inclusive CI Linux, sem as
//! dependências de sistema do Tauri.
//!
//! O shell (`src-tauri`) é uma casca fina por cima disto.

pub mod db;
pub mod erro;
pub mod modelo;

pub use db::Banco;
pub use erro::{Erro, Resultado};
pub use modelo::*;

#[cfg(test)]
mod testes {
    use super::*;

    fn banco_com_workspace() -> (Banco, Workspace) {
        let b = Banco::em_memoria().expect("abrir banco em memória");
        let ws = b.criar_workspace("Obra Vila Verde", "/tmp/vila-verde").unwrap();
        (b, ws)
    }

    #[test]
    fn migration_aplica_e_e_idempotente() {
        let b = Banco::em_memoria().unwrap();
        assert_eq!(b.versao_esquema().unwrap(), 1);
        // reabrir não deve reaplicar nada
        let b2 = Banco::em_memoria().unwrap();
        assert_eq!(b2.versao_esquema().unwrap(), 1);
    }

    #[test]
    fn cria_e_le_workspace() {
        let (b, ws) = banco_com_workspace();
        let lido = b.obter_workspace(&ws.id).unwrap();
        assert_eq!(lido, ws);
        assert_eq!(b.listar_workspaces().unwrap().len(), 1);
    }

    #[test]
    fn workspace_sem_nome_e_recusado() {
        let b = Banco::em_memoria().unwrap();
        let r = b.criar_workspace("   ", "/tmp/x");
        assert!(matches!(r, Err(Erro::Invalido(_))));
    }

    #[test]
    fn workspace_inexistente_da_erro_util() {
        let b = Banco::em_memoria().unwrap();
        let r = b.obter_workspace("nao-existe");
        match r {
            Err(e @ Erro::NaoEncontrado { .. }) => assert_eq!(e.codigo(), "nao_encontrado"),
            outro => panic!("esperava NaoEncontrado, veio {outro:?}"),
        }
    }

    #[test]
    fn viewport_persiste() {
        let (b, ws) = banco_com_workspace();
        let vp = Viewport { x: -120.5, y: 340.0, zoom: 1.75 };
        b.salvar_viewport(&ws.id, vp).unwrap();
        assert_eq!(b.obter_workspace(&ws.id).unwrap().viewport, vp);
    }

    #[test]
    fn zoom_zero_e_recusado() {
        let (b, ws) = banco_com_workspace();
        let r = b.salvar_viewport(&ws.id, Viewport { x: 0.0, y: 0.0, zoom: 0.0 });
        assert!(matches!(r, Err(Erro::Invalido(_))));
    }

    #[test]
    fn no_nasce_com_tamanho_do_tipo_e_z_crescente() {
        let (b, ws) = banco_com_workspace();
        let a = b.criar_no(&ws.id, TipoNo::Agente, "Pesquisador", 10.0, 20.0).unwrap();
        let n = b.criar_no(&ws.id, TipoNo::Nota, "Briefing", 50.0, 60.0).unwrap();
        assert_eq!((a.w, a.h), TipoNo::Agente.tamanho_padrao());
        assert_eq!((n.w, n.h), TipoNo::Nota.tamanho_padrao());
        assert!(n.z > a.z, "z deve crescer para o mais novo ficar por cima");
    }

    #[test]
    fn no_sem_nome_ganha_padrao_do_tipo() {
        let (b, ws) = banco_com_workspace();
        let n = b.criar_no(&ws.id, TipoNo::Nota, "  ", 0.0, 0.0).unwrap();
        assert_eq!(n.nome, "Nota");
    }

    #[test]
    fn no_em_workspace_inexistente_falha_antes_da_fk() {
        let b = Banco::em_memoria().unwrap();
        let r = b.criar_no("fantasma", TipoNo::Agente, "X", 0.0, 0.0);
        assert!(matches!(r, Err(Erro::NaoEncontrado { .. })));
    }

    #[test]
    fn mover_persiste_geometria() {
        let (b, ws) = banco_com_workspace();
        let n = b.criar_no(&ws.id, TipoNo::Agente, "A", 0.0, 0.0).unwrap();
        b.mover_no(&n.id, 100.0, 200.0, 500.0, 400.0).unwrap();
        let lido = &b.listar_nos(&ws.id).unwrap()[0];
        assert_eq!((lido.x, lido.y, lido.w, lido.h), (100.0, 200.0, 500.0, 400.0));
        assert!(lido.alterado_em >= lido.criado_em);
    }

    #[test]
    fn geometria_invalida_e_recusada() {
        let (b, ws) = banco_com_workspace();
        let n = b.criar_no(&ws.id, TipoNo::Agente, "A", 0.0, 0.0).unwrap();
        assert!(b.mover_no(&n.id, 0.0, 0.0, -1.0, 10.0).is_err());
        assert!(b.mover_no(&n.id, f64::NAN, 0.0, 10.0, 10.0).is_err());
    }

    #[test]
    fn trazer_para_frente_muda_ordem_de_listagem() {
        let (b, ws) = banco_com_workspace();
        let a = b.criar_no(&ws.id, TipoNo::Nota, "A", 0.0, 0.0).unwrap();
        let _z = b.criar_no(&ws.id, TipoNo::Nota, "B", 0.0, 0.0).unwrap();
        b.trazer_para_frente(&a.id).unwrap();
        let nos = b.listar_nos(&ws.id).unwrap();
        assert_eq!(nos.last().unwrap().id, a.id, "listagem vem em ordem de z");
    }

    #[test]
    fn remover_no_leva_os_cabos_junto() {
        let (b, ws) = banco_com_workspace();
        let a = b.criar_no(&ws.id, TipoNo::Agente, "A", 0.0, 0.0).unwrap();
        let c = b.criar_no(&ws.id, TipoNo::Agente, "B", 0.0, 0.0).unwrap();
        b.criar_cabo(&ws.id, &a.id, &c.id, TipoCabo::FalaCom).unwrap();
        assert_eq!(b.listar_cabos(&ws.id).unwrap().len(), 1);
        b.remover_no(&a.id).unwrap();
        assert_eq!(b.listar_cabos(&ws.id).unwrap().len(), 0, "CASCADE deve limpar");
    }

    #[test]
    fn cabo_para_si_mesmo_e_recusado() {
        let (b, ws) = banco_com_workspace();
        let a = b.criar_no(&ws.id, TipoNo::Agente, "A", 0.0, 0.0).unwrap();
        assert!(b.criar_cabo(&ws.id, &a.id, &a.id, TipoCabo::FalaCom).is_err());
    }

    #[test]
    fn cabo_duplicado_da_mensagem_de_gente() {
        let (b, ws) = banco_com_workspace();
        let a = b.criar_no(&ws.id, TipoNo::Agente, "A", 0.0, 0.0).unwrap();
        let c = b.criar_no(&ws.id, TipoNo::Agente, "B", 0.0, 0.0).unwrap();
        b.criar_cabo(&ws.id, &a.id, &c.id, TipoCabo::FalaCom).unwrap();
        match b.criar_cabo(&ws.id, &a.id, &c.id, TipoCabo::FalaCom) {
            Err(Erro::Invalido(m)) => assert!(m.contains("já estão ligados")),
            outro => panic!("esperava Invalido, veio {outro:?}"),
        }
    }

    #[test]
    fn vizinhos_enxergam_os_dois_sentidos_do_cabo() {
        let (b, ws) = banco_com_workspace();
        let a = b.criar_no(&ws.id, TipoNo::Agente, "A", 0.0, 0.0).unwrap();
        let c = b.criar_no(&ws.id, TipoNo::Agente, "B", 0.0, 0.0).unwrap();
        b.criar_cabo(&ws.id, &a.id, &c.id, TipoCabo::FalaCom).unwrap();
        assert_eq!(b.vizinhos(&a.id, TipoCabo::FalaCom).unwrap(), vec![c.id.clone()]);
        assert_eq!(b.vizinhos(&c.id, TipoCabo::FalaCom).unwrap(), vec![a.id.clone()]);
    }

    #[test]
    fn vizinhos_nao_vazam_entre_tipos_de_cabo() {
        let (b, ws) = banco_com_workspace();
        let a = b.criar_no(&ws.id, TipoNo::Agente, "A", 0.0, 0.0).unwrap();
        let nota = b.criar_no(&ws.id, TipoNo::Nota, "N", 0.0, 0.0).unwrap();
        b.criar_cabo(&ws.id, &a.id, &nota.id, TipoCabo::LeNota).unwrap();
        // ligado por le_nota não dá direito de falar
        assert!(b.vizinhos(&a.id, TipoCabo::FalaCom).unwrap().is_empty());
    }

    #[test]
    fn estado_canvas_traz_tudo_de_uma_vez() {
        let (b, ws) = banco_com_workspace();
        let a = b.criar_no(&ws.id, TipoNo::Agente, "A", 0.0, 0.0).unwrap();
        let n = b.criar_no(&ws.id, TipoNo::Nota, "N", 0.0, 0.0).unwrap();
        b.criar_cabo(&ws.id, &a.id, &n.id, TipoCabo::LeNota).unwrap();
        let e = b.estado_canvas(&ws.id).unwrap();
        assert_eq!(e.workspace.id, ws.id);
        assert_eq!(e.nos.len(), 2);
        assert_eq!(e.cabos.len(), 1);
    }

    #[test]
    fn workspaces_nao_veem_os_nos_um_do_outro() {
        let b = Banco::em_memoria().unwrap();
        let w1 = b.criar_workspace("Um", "/tmp/um").unwrap();
        let w2 = b.criar_workspace("Dois", "/tmp/dois").unwrap();
        b.criar_no(&w1.id, TipoNo::Agente, "A", 0.0, 0.0).unwrap();
        assert_eq!(b.listar_nos(&w1.id).unwrap().len(), 1);
        assert_eq!(b.listar_nos(&w2.id).unwrap().len(), 0);
    }

    // ---- máquina de estados do turno -------------------------------------

    #[test]
    fn transicoes_legitimas_do_turno() {
        use EstadoSessao::*;
        assert!(Ocioso.pode_ir_para(Pensando));
        assert!(Pensando.pode_ir_para(AguardandoAprovacao));
        assert!(AguardandoAprovacao.pode_ir_para(Pensando));
        assert!(AguardandoNo.pode_ir_para(Erro)); // prazo estourou
        assert!(Erro.pode_ir_para(Ocioso));
    }

    #[test]
    fn transicoes_ilegitimas_do_turno() {
        use EstadoSessao::*;
        assert!(!Ocioso.pode_ir_para(AguardandoAprovacao), "aprovação sem turno não existe");
        assert!(!Ocioso.pode_ir_para(Erro));
        assert!(!AguardandoHumano.pode_ir_para(AguardandoAprovacao));
    }

    #[test]
    fn estados_que_pedem_atencao() {
        use EstadoSessao::*;
        assert!(AguardandoAprovacao.pede_atencao());
        assert!(AguardandoHumano.pede_atencao());
        assert!(Erro.pede_atencao());
        assert!(!Pensando.pede_atencao(), "pensando não é pedido de socorro");
        assert!(!Ocioso.pede_atencao());
    }

    #[test]
    fn serializacao_dos_enums_bate_com_o_typescript() {
        // O front espera snake_case. Se isto quebrar, `src/lib/tipos.ts` quebra junto.
        assert_eq!(serde_json::to_string(&TipoNo::Agente).unwrap(), "\"agente\"");
        assert_eq!(serde_json::to_string(&TipoCabo::FalaCom).unwrap(), "\"fala_com\"");
        assert_eq!(
            serde_json::to_string(&EstadoSessao::AguardandoAprovacao).unwrap(),
            "\"aguardando_aprovacao\""
        );
    }
}
