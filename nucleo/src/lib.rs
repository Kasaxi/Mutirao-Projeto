//! Núcleo do Mutirão.
//!
//! Crate puro: modelo de domínio, banco e regras. Não conhece Tauri, não
//! conhece WebView, não abre janela. Isso é de propósito — dá para rodar
//! `cargo test -p nucleo` em qualquer máquina, inclusive CI Linux, sem as
//! dependências de sistema do Tauri.
//!
//! O shell (`src-tauri`) é uma casca fina por cima disto.

pub mod agente;
pub mod db;
pub mod erro;
pub mod modelo;
pub mod orquestrador;

pub use agente::{AdaptadorFalso, AgenteAdapter, ContextoSessao, Fabrica, FabricaFalsa, Roteiro};
pub use db::Banco;
pub use erro::{Erro, Resultado};
pub use modelo::*;
pub use orquestrador::{sink_mudo, Orquestrador, Sink};

#[cfg(test)]
mod testes {
    use super::*;

    fn banco_com_workspace() -> (Banco, Workspace) {
        let b = Banco::em_memoria().expect("abrir banco em memória");
        let ws = b.criar_workspace("Obra Vila Verde", "/tmp/vila-verde").unwrap();
        (b, ws)
    }

    /// Quantas migrations existem hoje. Ao somar uma, este número sobe junto —
    /// é o lembrete de que o esquema mudou e o teste de baixo cobre a subida.
    const VERSAO_ESQUEMA: i64 = 2;

    #[test]
    fn migration_aplica_e_e_idempotente() {
        let b = Banco::em_memoria().unwrap();
        assert_eq!(b.versao_esquema().unwrap(), VERSAO_ESQUEMA);
        // reabrir não deve reaplicar nada
        let b2 = Banco::em_memoria().unwrap();
        assert_eq!(b2.versao_esquema().unwrap(), VERSAO_ESQUEMA);
    }

    #[test]
    fn migration_002_reconstroi_session_sem_perder_os_filhos() {
        // A 002 troca o CHECK de `session.adaptador`, e no SQLite isso obriga
        // a derrubar e recriar a tabela. Com foreign_keys ligada, o DROP levaria
        // `message` e `tool_call` junto por CASCADE. Este teste existe porque
        // esse estrago é silencioso: nada falha, os dados só somem.
        let (b, ws) = banco_com_workspace();
        let no = b.criar_no(&ws.id, TipoNo::Agente, "A", 0.0, 0.0).unwrap();
        let s = b.criar_sessao(&no.id, Adaptador::Falso).unwrap();
        b.gravar_mensagem(&s.id, PapelMensagem::Usuario, "oi", Uso::default()).unwrap();

        // Sessão de adaptador falso só entra se a 002 tiver mesmo rodado.
        assert_eq!(s.adaptador, Adaptador::Falso);
        assert_eq!(b.historico(&s.id, 10).unwrap().len(), 1);
        // E o índice único do token precisa ter voltado com a tabela nova.
        let outro = b.criar_no(&ws.id, TipoNo::Agente, "B", 0.0, 0.0).unwrap();
        let s2 = b.criar_sessao(&outro.id, Adaptador::Claude).unwrap();
        assert_ne!(b.token_da_sessao(&s.id).unwrap(), b.token_da_sessao(&s2.id).unwrap());
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
        assert_eq!(serde_json::to_string(&Adaptador::Falso).unwrap(), "\"falso\"");
        assert_eq!(
            serde_json::to_string(&PapelMensagem::Usuario).unwrap(),
            "\"usuario\""
        );
    }

    // ================================================================ M1 ===
    // Sessão, turno, custo. Tudo contra o adaptador falso: orquestração
    // testada contra a API de verdade seria lenta, cara e não-determinística.

    use crate::agente::{FabricaFalsa, Roteiro};
    use crate::orquestrador::{Orquestrador, Sink};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// Banco, orquestrador e uma sessão pronta para receber turno, mais a
    /// lista de tudo que o núcleo contou à interface.
    struct Bancada {
        banco: Arc<Mutex<Banco>>,
        orq: Orquestrador,
        sessao: Sessao,
        node_id: String,
        workspace_id: String,
        avisos: Arc<Mutex<Vec<EventoNucleo>>>,
    }

    fn bancada(roteiro: Roteiro) -> Bancada {
        let banco = Banco::em_memoria().unwrap();
        let ws = banco.criar_workspace("Obra", "/tmp/obra").unwrap();
        let no = banco.criar_no(&ws.id, TipoNo::Agente, "Pesquisador", 0.0, 0.0).unwrap();
        let banco = Arc::new(Mutex::new(banco));

        let avisos: Arc<Mutex<Vec<EventoNucleo>>> = Arc::new(Mutex::new(Vec::new()));
        let copia = avisos.clone();
        let sink: Sink = Arc::new(move |e| copia.lock().unwrap().push(e));

        let orq = Orquestrador::novo(
            banco.clone(),
            Arc::new(FabricaFalsa::com_roteiro(roteiro)),
            sink,
        );
        let sessao = orq.abrir_sessao(&no.id, Adaptador::Falso).unwrap();
        Bancada {
            banco,
            orq,
            sessao,
            node_id: no.id,
            workspace_id: ws.id,
            avisos,
        }
    }

    impl Bancada {
        fn estado(&self) -> EstadoSessao {
            self.banco.lock().unwrap().obter_sessao(&self.sessao.id).unwrap().estado
        }

        fn historico(&self) -> Vec<Mensagem> {
            self.banco.lock().unwrap().historico(&self.sessao.id, 100).unwrap()
        }

        /// Espera o turno acabar. O turno roda numa thread; sem esperar, o
        /// teste leria o estado antes de o primeiro evento chegar.
        fn esperar_ocioso(&self) -> bool {
            self.esperar(|b| b.estado() == EstadoSessao::Ocioso)
        }

        fn esperar(&self, cond: impl Fn(&Bancada) -> bool) -> bool {
            for _ in 0..400 {
                if cond(self) {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            false
        }
    }

    /// Roteiro mínimo: um turno que responde e cobra, sem atraso.
    fn roteiro_simples() -> Roteiro {
        Roteiro {
            atraso_ms: 0,
            eventos: vec![
                EventoAgente::SessaoIniciada {
                    id_externo: "ext_123".into(),
                    modelo: "claude-opus-5".into(),
                    ferramentas: vec![],
                },
                EventoAgente::TurnoConcluido {
                    texto_final: "O item 4.2 contradiz o anexo I.".into(),
                    uso: Uso {
                        tokens_entrada: 1_000,
                        tokens_saida: 200,
                        custo_usd: custo_do_uso("claude-opus-5", 1_000, 200),
                    },
                },
            ],
        }
    }

    #[test]
    fn sessao_nasce_ociosa_e_e_reaproveitada() {
        let b = bancada(roteiro_simples());
        assert_eq!(b.sessao.estado, EstadoSessao::Ocioso);
        // Reabrir o nó não pode começar conversa nova.
        let outra = b.orq.abrir_sessao(&b.node_id, Adaptador::Falso).unwrap();
        assert_eq!(outra.id, b.sessao.id);
    }

    #[test]
    fn so_no_de_agente_abre_sessao() {
        let (banco, ws) = banco_com_workspace();
        let nota = banco.criar_no(&ws.id, TipoNo::Nota, "Briefing", 0.0, 0.0).unwrap();
        assert!(matches!(
            banco.criar_sessao(&nota.id, Adaptador::Falso),
            Err(Erro::Invalido(_))
        ));
    }

    #[test]
    fn o_token_do_mcp_nunca_sai_no_json_da_sessao() {
        // A sessão atravessa a fronteira IPC. Se o token viajasse junto, ele
        // chegaria ao front — e o que chega ao front chega a tudo que roda
        // no front. Ver ESPECIFICACAO.md §4.
        let b = bancada(roteiro_simples());
        let json = serde_json::to_string(&b.sessao).unwrap();
        assert!(!json.contains("token"), "campo token vazou: {json}");
        let guardado = b.banco.lock().unwrap().token_da_sessao(&b.sessao.id).unwrap();
        assert_eq!(guardado.len(), 64, "32 bytes em hexadecimal");
        assert!(!json.contains(&guardado), "valor do token vazou: {json}");
    }

    #[test]
    fn turno_completo_grava_conversa_estado_e_custo() {
        let b = bancada(roteiro_simples());
        b.orq.enviar(&b.sessao.id, "resuma este PDF").unwrap();
        assert!(b.esperar_ocioso(), "turno não terminou");

        let hist = b.historico();
        assert_eq!(hist.len(), 2, "pergunta e resposta");
        assert_eq!(hist[0].papel, PapelMensagem::Usuario);
        assert_eq!(hist[0].conteudo, "resuma este PDF");
        assert_eq!(hist[1].papel, PapelMensagem::Agente);
        assert!(hist[1].conteudo.contains("4.2"));

        // 1000 entrada a US$5/M + 200 saída a US$25/M = 0,005 + 0,005
        let sessao = b.banco.lock().unwrap().obter_sessao(&b.sessao.id).unwrap();
        assert!((sessao.custo_total - 0.010).abs() < 1e-9, "custo: {}", sessao.custo_total);
        assert_eq!(sessao.sessao_externa_id.as_deref(), Some("ext_123"));

        // e a interface foi avisada do custo, com a fatia do nó
        let avisos = b.avisos.lock().unwrap();
        let custo = avisos.iter().find_map(|e| match e {
            EventoNucleo::CustoAtualizado { workspace_id, total, por_no } => {
                Some((workspace_id.clone(), *total, por_no.clone()))
            }
            _ => None,
        });
        let (ws, total, por_no) = custo.expect("nenhum custo:atualizado");
        assert_eq!(ws, b.workspace_id);
        assert!((total - 0.010).abs() < 1e-9);
        assert_eq!(por_no.len(), 1);
        assert_eq!(por_no[0].node_id, b.node_id);
    }

    #[test]
    fn dois_turnos_ao_mesmo_tempo_no_mesmo_no_sao_recusados() {
        // A regra "um turno por vez por nó" existe porque duas mensagens
        // intercaladas produzem contexto embaralhado e resposta sem sentido.
        let b = bancada(Roteiro { atraso_ms: 30, ..roteiro_simples() });
        b.orq.enviar(&b.sessao.id, "primeira").unwrap();
        assert!(b.esperar(|b| b.estado() == EstadoSessao::Pensando));

        match b.orq.enviar(&b.sessao.id, "segunda") {
            Err(Erro::Invalido(m)) => assert!(m.contains("turno"), "mensagem: {m}"),
            outro => panic!("esperava recusa, veio {outro:?}"),
        }
        assert!(b.esperar_ocioso());
        // a segunda não pode ter entrado no histórico nem pela metade
        assert!(b.historico().iter().all(|m| m.conteudo != "segunda"));
    }

    #[test]
    fn mensagem_vazia_e_recusada_antes_de_gastar_qualquer_coisa() {
        let b = bancada(roteiro_simples());
        assert!(matches!(b.orq.enviar(&b.sessao.id, "   "), Err(Erro::Invalido(_))));
        assert_eq!(b.estado(), EstadoSessao::Ocioso);
        assert!(b.historico().is_empty());
    }

    #[test]
    fn cancelar_devolve_o_no_para_ocioso_e_registra_quem_parou() {
        let b = bancada(Roteiro { atraso_ms: 40, ..roteiro_simples() });
        b.orq.enviar(&b.sessao.id, "vai demorar").unwrap();
        assert!(b.esperar(|b| b.estado() == EstadoSessao::Pensando));

        b.orq.cancelar(&b.sessao.id).unwrap();
        assert_eq!(b.estado(), EstadoSessao::Ocioso);

        let hist = b.historico();
        assert_eq!(hist.last().unwrap().papel, PapelMensagem::Sistema);
        assert!(hist.last().unwrap().conteudo.contains("interrompido"));

        // Cancelar não é erro: o nó não pode ficar pedindo atenção por isso.
        assert!(!b.estado().pede_atencao());
        // E o que estava a caminho não pode entrar depois do cancelamento.
        std::thread::sleep(Duration::from_millis(120));
        assert_eq!(b.estado(), EstadoSessao::Ocioso);
        assert!(b.historico().iter().all(|m| m.papel != PapelMensagem::Agente));
    }

    #[test]
    fn cancelar_duas_vezes_nao_estraga_nada() {
        // O usuário clica de novo quando a primeira vez não pareceu funcionar.
        let b = bancada(roteiro_simples());
        b.orq.cancelar(&b.sessao.id).unwrap();
        b.orq.cancelar(&b.sessao.id).unwrap();
        assert_eq!(b.estado(), EstadoSessao::Ocioso);
    }

    #[test]
    fn adaptador_que_cala_no_meio_deixa_o_no_em_erro_e_nao_pensando() {
        // Pior desfecho possível: nó "pensando" para sempre. Não pede atenção,
        // não aceita turno novo e não explica nada. Ver o risco "agente travado
        // sem avisar" no ARQUITETURA.md §11.
        let b = bancada(Roteiro {
            atraso_ms: 0,
            eventos: vec![EventoAgente::TextoParcial { delta: "começando…".into() }],
        });
        b.orq.enviar(&b.sessao.id, "oi").unwrap();
        assert!(b.esperar(|b| b.estado() == EstadoSessao::Erro), "ficou em {:?}", b.estado());
        assert!(b.estado().pede_atencao());
        assert!(b.historico().last().unwrap().conteudo.contains("parou de responder"));
    }

    #[test]
    fn erro_do_agente_deixa_o_no_pedindo_atencao_e_aceita_turno_novo() {
        let b = bancada(Roteiro {
            atraso_ms: 0,
            eventos: vec![EventoAgente::Erro {
                mensagem: "A chave de API foi recusada.".into(),
                recuperavel: true,
            }],
        });
        b.orq.enviar(&b.sessao.id, "oi").unwrap();
        assert!(b.esperar(|b| b.estado() == EstadoSessao::Erro));
        assert!(b.historico().last().unwrap().conteudo.contains("chave"));
        // Mandar de novo é reconhecer o erro: não deve exigir botão nenhum.
        assert!(b.orq.enviar(&b.sessao.id, "tenta de novo").is_ok());
    }

    #[test]
    fn texto_parcial_vira_resposta_quando_o_final_vem_vazio() {
        // Adaptador que só manda pedaços não pode perder a resposta.
        let b = bancada(Roteiro {
            atraso_ms: 0,
            eventos: vec![
                EventoAgente::TextoParcial { delta: "Li os três PDFs. ".into() },
                EventoAgente::TextoParcial { delta: "O item 4.2 contradiz o anexo I.".into() },
                EventoAgente::TurnoConcluido { texto_final: String::new(), uso: Uso::default() },
            ],
        });
        b.orq.enviar(&b.sessao.id, "resuma").unwrap();
        assert!(b.esperar_ocioso());
        assert_eq!(
            b.historico().last().unwrap().conteudo,
            "Li os três PDFs. O item 4.2 contradiz o anexo I."
        );
    }

    #[test]
    fn ferramenta_vira_linha_de_auditoria_com_pedido_e_resultado() {
        let b = bancada(Roteiro {
            atraso_ms: 0,
            eventos: vec![
                EventoAgente::FerramentaPedida {
                    id: "fer_1".into(),
                    nome: "ler_arquivo".into(),
                    argumentos: serde_json::json!({ "caminho": "contrato.docx" }),
                },
                EventoAgente::FerramentaConcluida {
                    id: "fer_1".into(),
                    resultado: Some(serde_json::json!({ "bytes": 100 })),
                    erro: None,
                },
                EventoAgente::TurnoConcluido {
                    texto_final: "pronto".into(),
                    uso: Uso::default(),
                },
            ],
        });
        b.orq.enviar(&b.sessao.id, "leia").unwrap();
        assert!(b.esperar_ocioso());

        let acoes = b.banco.lock().unwrap().ferramentas_da_sessao(&b.sessao.id).unwrap();
        assert_eq!(acoes.len(), 1);
        assert_eq!(acoes[0].ferramenta, "ler_arquivo");
        assert_eq!(acoes[0].argumentos["caminho"], "contrato.docx");
        assert_eq!(acoes[0].resultado.as_ref().unwrap()["bytes"], 100);
        assert_eq!(acoes[0].aprovacao, Aprovacao::Automatica);
    }

    #[test]
    fn custo_sai_da_tabela_de_precos_e_modelo_desconhecido_nao_vira_zero() {
        // 1M de entrada a US$5 e 1M de saída a US$25.
        assert!((custo_do_uso("claude-opus-5", 1_000_000, 1_000_000) - 30.0).abs() < 1e-9);
        assert!((custo_do_uso("claude-sonnet-5", 1_000_000, 0) - 2.0).abs() < 1e-9);
        // Zero mentiria e sumiria do painel; NaN faz o front mostrar "—".
        assert!(custo_do_uso("modelo-que-nao-existe", 1_000, 1_000).is_nan());
    }

    #[test]
    fn custo_desconhecido_nao_envenena_o_total_da_sessao() {
        let b = bancada(Roteiro {
            atraso_ms: 0,
            eventos: vec![EventoAgente::TurnoConcluido {
                texto_final: "oi".into(),
                uso: Uso {
                    tokens_entrada: 10,
                    tokens_saida: 10,
                    custo_usd: custo_do_uso("modelo-desconhecido", 10, 10),
                },
            }],
        });
        b.orq.enviar(&b.sessao.id, "oi").unwrap();
        assert!(b.esperar_ocioso());
        let total = b.banco.lock().unwrap().obter_sessao(&b.sessao.id).unwrap().custo_total;
        assert_eq!(total, 0.0, "um NaN somado uma vez apaga o total para sempre");
    }

    #[test]
    fn evento_do_agente_serializa_com_discriminante_para_o_typescript() {
        // O front trata EventoAgente como união discriminada por `tipo`. Se o
        // formato mudar aqui, `src/lib/tipos.ts` para de casar.
        let e = EventoAgente::TextoParcial { delta: "oi".into() };
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"tipo":"texto_parcial","delta":"oi"}"#
        );
        let e = EventoAgente::FerramentaPedida {
            id: "1".into(),
            nome: "ler_arquivo".into(),
            argumentos: serde_json::json!({}),
        };
        assert!(serde_json::to_string(&e).unwrap().contains(r#""tipo":"ferramenta_pedida""#));
    }

    #[test]
    fn roteiro_de_demonstracao_cobra_pela_mesma_tabela_que_o_de_verdade() {
        // Se o preço mudar, o custo falso muda junto — maquete que mente sobre
        // dinheiro é pior que maquete nenhuma.
        let r = Roteiro::demonstracao("resuma este contrato");
        let uso = r.eventos.iter().find_map(|e| match e {
            EventoAgente::TurnoConcluido { uso, .. } => Some(*uso),
            _ => None,
        });
        let uso = uso.expect("o roteiro precisa terminar em TurnoConcluido");
        assert!((uso.custo_usd - custo_do_uso("claude-opus-5", 1_420, 96)).abs() < 1e-12);
        assert!(uso.custo_usd > 0.0);
    }

    #[test]
    fn historico_devolve_as_mais_recentes_em_ordem_de_leitura() {
        let b = bancada(roteiro_simples());
        {
            let banco = b.banco.lock().unwrap();
            for i in 0..5 {
                banco
                    .gravar_mensagem(
                        &b.sessao.id,
                        PapelMensagem::Usuario,
                        &format!("m{i}"),
                        Uso::default(),
                    )
                    .unwrap();
            }
        }
        let ultimas = b.banco.lock().unwrap().historico(&b.sessao.id, 3).unwrap();
        let textos: Vec<_> = ultimas.iter().map(|m| m.conteudo.as_str()).collect();
        assert_eq!(textos, vec!["m2", "m3", "m4"], "as 3 mais novas, de cima para baixo");
    }

    #[test]
    fn reabrir_o_app_destrava_turno_que_ficou_no_ar() {
        // Fechar o Mutirão enquanto um agente pensa deixaria o nó em
        // "pensando" para sempre: não pede atenção, não aceita turno novo e
        // não explica nada. Precisa de banco em arquivo — o em memória some
        // junto com a conexão e nunca reproduziria isto.
        let caminho = std::env::temp_dir().join(format!("mutirao-teste-{}.db", novo_id()));
        let (id_sessao, id_aprovacao) = {
            let b = Banco::abrir(&caminho).unwrap();
            let ws = b.criar_workspace("Obra", "/tmp/obra").unwrap();
            let no = b.criar_no(&ws.id, TipoNo::Agente, "A", 0.0, 0.0).unwrap();
            let s = b.criar_sessao(&no.id, Adaptador::Falso).unwrap();
            b.mudar_estado_sessao(&s.id, EstadoSessao::Pensando).unwrap();

            // Este outro ficou esperando o usuário decidir. A pergunta é
            // legítima e tem de sobreviver ao fechamento.
            let no2 = b.criar_no(&ws.id, TipoNo::Agente, "B", 0.0, 0.0).unwrap();
            let s2 = b.criar_sessao(&no2.id, Adaptador::Falso).unwrap();
            b.mudar_estado_sessao(&s2.id, EstadoSessao::Pensando).unwrap();
            b.mudar_estado_sessao(&s2.id, EstadoSessao::AguardandoHumano).unwrap();
            (s.id, s2.id)
        };

        let b = Banco::abrir(&caminho).unwrap();
        assert_eq!(b.obter_sessao(&id_sessao).unwrap().estado, EstadoSessao::Erro);
        assert!(b.historico(&id_sessao, 10).unwrap().last().unwrap().conteudo.contains("fechado"));
        assert_eq!(
            b.obter_sessao(&id_aprovacao).unwrap().estado,
            EstadoSessao::AguardandoHumano,
            "pergunta pendente não pode ser apagada pela abertura"
        );

        drop(b);
        let _ = std::fs::remove_file(&caminho);
    }

    #[test]
    fn remover_o_no_leva_sessao_e_conversa_junto() {
        let b = bancada(roteiro_simples());
        {
            let banco = b.banco.lock().unwrap();
            banco
                .gravar_mensagem(&b.sessao.id, PapelMensagem::Usuario, "oi", Uso::default())
                .unwrap();
            banco.remover_no(&b.node_id).unwrap();
            assert!(banco.sessao_do_no(&b.node_id).unwrap().is_none());
            assert!(banco.historico(&b.sessao.id, 10).unwrap().is_empty());
        }
    }
}
