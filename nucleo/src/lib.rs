//! Núcleo do Mutirão.
//!
//! Crate puro: modelo de domínio, banco e regras. Não conhece Tauri, não
//! conhece WebView, não abre janela. Isso é de propósito — dá para rodar
//! `cargo test -p nucleo` em qualquer máquina, inclusive CI Linux, sem as
//! dependências de sistema do Tauri.
//!
//! O shell (`src-tauri`) é uma casca fina por cima disto.

pub mod agente;
pub mod arquivos;
pub mod barramento;
pub mod claude;
pub mod db;
pub mod erro;
pub mod modelo;
pub mod orquestrador;

pub use agente::{AdaptadorFalso, AgenteAdapter, ContextoSessao, Fabrica, FabricaFalsa, Roteiro};
pub use arquivos::ItemArquivo;
pub use barramento::{Aprovacoes, Barramento};
pub use claude::{AdaptadorClaude, FabricaClaude};
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
    const VERSAO_ESQUEMA: i64 = 3;

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

    // ---- adaptador Claude ------------------------------------------------
    //
    // Contra saída DE VERDADE da CLI 2.1.251, capturada e guardada em
    // `nucleo/testes/claude_stream.jsonl`. Testar tradução contra um JSON
    // inventado só prova que sabemos escrever o que já escrevemos; contra a
    // saída real, prova que entendemos a CLI.

    use crate::claude::{traduzir, Traducao};

    fn eventos_do_fixture() -> Vec<EventoAgente> {
        let bruto = include_str!("../testes/claude_stream.jsonl");
        let mut t = Traducao::default();
        let mut todos = Vec::new();
        for linha in bruto.lines().filter(|l| !l.trim().is_empty()) {
            todos.extend(traduzir(linha, &mut t));
        }
        todos
    }

    #[test]
    fn traduz_o_stream_real_da_cli_em_eventos_do_mutirao() {
        let eventos = eventos_do_fixture();

        let inicio = eventos.iter().find_map(|e| match e {
            EventoAgente::SessaoIniciada { id_externo, modelo, ferramentas } => {
                Some((id_externo.clone(), modelo.clone(), ferramentas.len()))
            }
            _ => None,
        });
        let (id, modelo, ferramentas) = inicio.expect("faltou SessaoIniciada");
        assert_eq!(id, "sess_exemplo");
        assert!(modelo.starts_with("claude-"), "modelo: {modelo}");
        assert!(ferramentas > 0, "o init precisa listar as ferramentas");

        // O texto tem de chegar em pedaços — é o que faz a face conversa
        // parecer conversa em vez de tela que pisca no fim.
        let pedacos: Vec<&str> = eventos
            .iter()
            .filter_map(|e| match e {
                EventoAgente::TextoParcial { delta } => Some(delta.as_str()),
                _ => None,
            })
            .collect();
        assert!(pedacos.len() >= 2, "esperava vários deltas, veio {}", pedacos.len());
        assert!(pedacos.iter().all(|p| !p.is_empty()), "delta vazio não vira evento");

        // Pedido e resultado de ferramenta, casados pelo id.
        let pedidos: Vec<(String, String)> = eventos
            .iter()
            .filter_map(|e| match e {
                EventoAgente::FerramentaPedida { id, nome, .. } => {
                    Some((id.clone(), nome.clone()))
                }
                _ => None,
            })
            .collect();
        let concluidas: Vec<String> = eventos
            .iter()
            .filter_map(|e| match e {
                EventoAgente::FerramentaConcluida { id, .. } => Some(id.clone()),
                _ => None,
            })
            .collect();
        assert!(!pedidos.is_empty(), "faltou FerramentaPedida");
        for (id, _) in &pedidos {
            assert!(concluidas.contains(id), "ferramenta {id} pedida e nunca concluída");
        }
        assert!(
            pedidos.iter().any(|(_, nome)| nome == "Read"),
            "esperava a ferramenta Read no fixture"
        );

        // A linha de atividade da CLI vira o "pensando" da interface.
        assert!(
            eventos.iter().any(|e| matches!(e, EventoAgente::Raciocinando { .. })),
            "task_summary com detalhe deveria virar Raciocinando"
        );
    }

    #[test]
    fn o_custo_vem_da_cli_e_nao_da_nossa_tabela() {
        // Este é o teste que justifica a decisão. No turno capturado a CLI
        // cobrou US$ 0,0496; a tabela de preços daria US$ 0,58, porque ela não
        // sabe que leitura de cache custa um décimo. Um painel de custo 11x
        // errado é pior que painel nenhum.
        let eventos = eventos_do_fixture();
        let uso = eventos
            .iter()
            .find_map(|e| match e {
                EventoAgente::TurnoConcluido { uso, .. } => Some(*uso),
                _ => None,
            })
            .expect("faltou TurnoConcluido");

        assert!((uso.custo_usd - 0.049_611_2).abs() < 1e-6, "custo: {}", uso.custo_usd);

        // A entrada soma o contexto inteiro, cache incluído.
        assert_eq!(uso.tokens_entrada, 6 + 6_071 + 108_511);
        assert_eq!(uso.tokens_saida, 263);

        let pela_tabela = custo_do_uso("claude-opus-5", uso.tokens_entrada, uso.tokens_saida);
        assert!(
            pela_tabela > uso.custo_usd * 10.0,
            "a tabela deveria errar feio aqui: {pela_tabela} vs {}",
            uso.custo_usd
        );
    }

    #[test]
    fn os_dois_erros_reais_da_cli_viram_evento_de_erro() {
        // O fixture traz os dois que a CLI 2.1.251 produziu de verdade:
        // retomada de sessão inexistente e estouro de --max-turns.
        let eventos = eventos_do_fixture();
        let erros: Vec<(String, bool)> = eventos
            .iter()
            .filter_map(|e| match e {
                EventoAgente::Erro { mensagem, recuperavel } => {
                    Some((mensagem.clone(), *recuperavel))
                }
                _ => None,
            })
            .collect();
        assert_eq!(erros.len(), 2, "veio {erros:?}");
        assert!(erros.iter().all(|(_, r)| *r), "o nó precisa aceitar outro turno");
        assert!(erros.iter().any(|(m, _)| m.contains("error_max_turns")), "veio {erros:?}");
    }

    #[test]
    fn erro_sem_texto_e_marcado_para_o_stderr_completar() {
        // Este é o achado que mais mudou o adaptador: nos dois erros reais o
        // campo `result` **nem existe**, e a frase que o usuário precisa ler
        // ("No conversation found with session ID: …") sai pelo stderr. Sem
        // esta marca, o adaptador mandaria o genérico e jogaria fora a única
        // informação útil.
        let mut t = Traducao::default();
        let linha = r#"{"type":"result","subtype":"error_during_execution","is_error":true,
                        "session_id":"x","total_cost_usd":0}"#;
        let eventos = traduzir(&linha.replace('\n', " "), &mut t);
        assert!(matches!(eventos[0], EventoAgente::Erro { .. }));
        assert!(t.erro_sem_texto, "sem isto o stderr é descartado");

        // E o caminho oposto: erro COM texto não espera stderr nenhum.
        let mut t2 = Traducao::default();
        let com_texto = r#"{"type":"result","subtype":"error_during_execution","is_error":true,
                            "result":"deu ruim aqui","total_cost_usd":0}"#;
        traduzir(&com_texto.replace('\n', " "), &mut t2);
        assert!(!t2.erro_sem_texto);
    }

    #[test]
    fn linha_desconhecida_e_ignorada_em_vez_de_quebrar() {
        // A CLI ganha tipos de evento a cada versão. Um adaptador que estoura
        // ao ver um evento novo estoura no dia da atualização, na máquina do
        // usuário.
        let mut t = Traducao::default();
        assert!(traduzir(r#"{"type":"invento_novo","valor":1}"#, &mut t).is_empty());
        assert!(traduzir("isto não é json", &mut t).is_empty());
        assert!(traduzir("", &mut t).is_empty());
        assert!(traduzir(r#"{"type":"system","subtype":"task_summary","detail":null}"#, &mut t)
            .is_empty());
    }

    #[test]
    fn resultado_de_ferramenta_gigante_e_encolhido_antes_de_ir_para_o_banco() {
        // `tool_call.resultado_json` é append-only. Guardar o arquivo inteiro a
        // cada leitura incha o banco sem servir a ninguém: o card mostra "leu
        // contrato.docx", não o contrato.
        let enorme = "x".repeat(50_000);
        let linha = serde_json::json!({
            "type": "user",
            "message": { "content": [
                { "type": "tool_result", "tool_use_id": "t1", "content": enorme }
            ]}
        })
        .to_string();
        let mut t = Traducao::default();
        let eventos = traduzir(&linha, &mut t);
        match &eventos[0] {
            EventoAgente::FerramentaConcluida { resultado: Some(r), .. } => {
                assert_eq!(r["truncado"], true);
                assert!(r["conteudo"].as_str().unwrap().chars().count() <= 2_000);
            }
            outro => panic!("esperava FerramentaConcluida, veio {outro:?}"),
        }
    }

    #[test]
    fn ferramenta_que_falhou_vira_erro_no_card_e_nao_resultado() {
        let linha = serde_json::json!({
            "type": "user",
            "message": { "content": [
                { "type": "tool_result", "tool_use_id": "t1",
                  "content": "File does not exist.", "is_error": true }
            ]}
        })
        .to_string();
        let mut t = Traducao::default();
        match &traduzir(&linha, &mut t)[0] {
            EventoAgente::FerramentaConcluida { resultado, erro, .. } => {
                assert!(resultado.is_none());
                assert_eq!(erro.as_deref(), Some("File does not exist."));
            }
            outro => panic!("veio {outro:?}"),
        }
    }

    #[test]
    fn adaptador_claude_ausente_da_erro_que_diz_o_que_fazer() {
        let r = crate::claude::AdaptadorClaude::detectar("claude-que-nao-existe-mesmo");
        match r {
            Err(Erro::Invalido(m)) => {
                assert!(m.contains("MUTIRAO_CLAUDE_BIN"), "mensagem inútil: {m}");
            }
            outro => panic!("esperava erro explicativo, veio {outro:?}"),
        }
    }

    // ================================================================ M2 ===
    // Aprovação. O agente fica literalmente parado — segurando a resposta HTTP
    // do hook — enquanto o card espera um clique. É isso que torna o card
    // honesto: o arquivo não é gravado e desfeito, ele não chega a ser gravado.

    use crate::barramento::{tratar, Aprovacoes, Veredito};

    struct Balcao {
        banco: Arc<Mutex<Banco>>,
        orq: Orquestrador,
        aprovacoes: Arc<Aprovacoes>,
        sink: Sink,
        token: String,
        sessao_id: String,
        workspace_id: String,
        avisos: Arc<Mutex<Vec<EventoNucleo>>>,
    }

    fn balcao() -> Balcao {
        let banco = Banco::em_memoria().unwrap();
        let ws = banco.criar_workspace("Obra", "/tmp/obra-m2").unwrap();
        let no = banco.criar_no(&ws.id, TipoNo::Agente, "Redator", 0.0, 0.0).unwrap();
        let sessao = banco.criar_sessao(&no.id, Adaptador::Falso).unwrap();
        let token = banco.token_da_sessao(&sessao.id).unwrap();
        // O turno precisa estar em andamento: aprovação só existe dentro de um.
        banco.mudar_estado_sessao(&sessao.id, EstadoSessao::Pensando).unwrap();
        let banco = Arc::new(Mutex::new(banco));

        let avisos: Arc<Mutex<Vec<EventoNucleo>>> = Arc::new(Mutex::new(Vec::new()));
        let copia = avisos.clone();
        let sink: Sink = Arc::new(move |e| copia.lock().unwrap().push(e));

        let orq = Orquestrador::novo(
            banco.clone(),
            Arc::new(FabricaFalsa::demonstracao()),
            sink.clone(),
        );
        Balcao {
            aprovacoes: orq.aprovacoes(),
            banco,
            orq,
            sink,
            token,
            sessao_id: sessao.id,
            workspace_id: ws.id,
            avisos,
        }
    }

    impl Balcao {
        fn corpo(&self, ferramenta: &str, argumentos: serde_json::Value) -> String {
            serde_json::json!({
                "tool_name": ferramenta,
                "tool_input": argumentos,
                "tool_use_id": "toolu_teste",
            })
            .to_string()
        }

        fn id_da_chamada(&self) -> String {
            format!("{}:toolu_teste", self.sessao_id)
        }

        /// Dispara o pedido numa thread — ele bloqueia até alguém decidir.
        fn pedir(&self, corpo: String, prazo: Duration) -> std::thread::JoinHandle<Veredito> {
            let banco = self.banco.clone();
            let apr = self.aprovacoes.clone();
            let sink = self.sink.clone();
            let token = self.token.clone();
            std::thread::spawn(move || {
                tratar(&banco, &apr, &sink, &token, &corpo, prazo).expect("pedido válido")
            })
        }

        fn esperar_card(&self) -> bool {
            for _ in 0..400 {
                if self.aprovacoes.quantas_esperando() == 1 {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            false
        }

        fn eventos(&self) -> Vec<EventoNucleo> {
            self.avisos.lock().unwrap().clone()
        }
    }

    #[test]
    fn token_invalido_nao_conta_nada_a_quem_perguntou() {
        // "Esse nó não existe", nunca "existe mas você não pode" — §4.
        let b = balcao();
        let corpo = b.corpo("Write", serde_json::json!({"file_path": "x", "content": "y"}));
        let r = tratar(
            &b.banco,
            &b.aprovacoes,
            &b.sink,
            "token-que-nao-existe",
            &corpo,
            Duration::from_millis(50),
        );
        match r {
            Err(Erro::NaoEncontrado { id, .. }) => {
                assert_eq!(id, "token", "o token não pode aparecer na mensagem de erro");
            }
            outro => panic!("esperava recusa, veio {outro:?}"),
        }
        // E sem token nenhum, também não.
        assert!(tratar(
            &b.banco,
            &b.aprovacoes,
            &b.sink,
            "",
            &corpo,
            Duration::from_millis(50)
        )
        .is_err());
    }

    #[test]
    fn ler_nao_pede_licenca() {
        // Um card por arquivo aberto viraria ruído, e card que vira ruído é
        // card que o usuário aprova sem ler.
        let b = balcao();
        let corpo = b.corpo("Read", serde_json::json!({"file_path": "contrato.docx"}));
        let v = tratar(
            &b.banco,
            &b.aprovacoes,
            &b.sink,
            &b.token,
            &corpo,
            Duration::from_millis(50),
        )
        .unwrap();
        assert!(v.permitir);
        assert_eq!(b.aprovacoes.quantas_esperando(), 0);
        assert!(b.eventos().is_empty(), "leitura não devia acender card nenhum");
    }

    #[test]
    fn gravacao_espera_o_humano_e_so_entao_libera() {
        let b = balcao();
        let corpo = b.corpo(
            "Write",
            serde_json::json!({"file_path": "/tmp/obra-m2/orçamento.xlsx", "content": "a\nb\nc"}),
        );
        let pedido = b.pedir(corpo, Duration::from_secs(30));

        assert!(b.esperar_card(), "o pedido devia estar esperando alguém");

        // O nó pede atenção e a linha de auditoria já existe, pendente.
        let chamada = b.banco.lock().unwrap().obter_ferramenta(&b.id_da_chamada()).unwrap();
        assert_eq!(chamada.aprovacao, Aprovacao::Pendente);
        assert_eq!(
            b.banco.lock().unwrap().obter_sessao(&b.sessao_id).unwrap().estado,
            EstadoSessao::AguardandoAprovacao
        );

        // O card chegou à interface com o texto mastigado.
        let pedido_ui = b.eventos().into_iter().find_map(|e| match e {
            EventoNucleo::AprovacaoPedida { pedido } => Some(pedido),
            _ => None,
        });
        let p = pedido_ui.expect("faltou aprovacao:pedida");
        assert_eq!(p.resumo, "Gravar orçamento.xlsx");
        assert!(p.detalhe.contains("3 linhas"), "detalhe: {}", p.detalhe);
        assert_eq!(p.previa.as_deref(), Some("a\nb\nc"));

        b.orq.decidir_aprovacao(&b.id_da_chamada(), Decisao::Aprovada, false).unwrap();

        let v = pedido.join().unwrap();
        assert!(v.permitir, "motivo: {}", v.motivo);

        let chamada = b.banco.lock().unwrap().obter_ferramenta(&b.id_da_chamada()).unwrap();
        assert_eq!(chamada.aprovacao, Aprovacao::Aprovada);
        assert_eq!(chamada.decidido_por.as_deref(), Some("usuario"));
        assert_eq!(
            b.banco.lock().unwrap().obter_sessao(&b.sessao_id).unwrap().estado,
            EstadoSessao::Pensando,
            "decidido o card, o turno continua"
        );
    }

    #[test]
    fn negar_bloqueia_e_explica_ao_agente_para_ele_nao_insistir() {
        let b = balcao();
        let corpo = b.corpo("Write", serde_json::json!({"file_path": "x.txt", "content": "y"}));
        let pedido = b.pedir(corpo, Duration::from_secs(30));
        assert!(b.esperar_card());

        b.orq.decidir_aprovacao(&b.id_da_chamada(), Decisao::Negada, false).unwrap();
        let v = pedido.join().unwrap();

        assert!(!v.permitir);
        // A mensagem vai para o modelo. Sem dizer "não tente por outro
        // caminho", um agente prestativo tenta gravar via outra ferramenta.
        assert!(v.motivo.contains("Negado"), "motivo: {}", v.motivo);
        assert!(v.motivo.to_lowercase().contains("não tente"), "motivo: {}", v.motivo);

        let chamada = b.banco.lock().unwrap().obter_ferramenta(&b.id_da_chamada()).unwrap();
        assert_eq!(chamada.aprovacao, Aprovacao::Negada);
    }

    #[test]
    fn nao_perguntar_de_novo_dispensa_o_card_na_proxima() {
        let b = balcao();
        let corpo = b.corpo("Write", serde_json::json!({"file_path": "a.txt", "content": "1"}));
        let pedido = b.pedir(corpo, Duration::from_secs(30));
        assert!(b.esperar_card());
        b.orq.decidir_aprovacao(&b.id_da_chamada(), Decisao::Aprovada, true).unwrap();
        assert!(pedido.join().unwrap().permitir);

        // Segunda gravação: não pode parar em card nenhum.
        let corpo2 = serde_json::json!({
            "tool_name": "Write",
            "tool_input": {"file_path": "b.txt", "content": "2"},
            "tool_use_id": "toolu_outro",
        })
        .to_string();
        let v = tratar(
            &b.banco,
            &b.aprovacoes,
            &b.sink,
            &b.token,
            &corpo2,
            Duration::from_millis(200),
        )
        .unwrap();
        assert!(v.permitir, "a regra devia ter decidido sozinha");

        // E o log de auditoria diz que quem decidiu foi a regra, não o usuário.
        let chamada = b
            .banco
            .lock()
            .unwrap()
            .obter_ferramenta(&format!("{}:toolu_outro", b.sessao_id))
            .unwrap();
        assert_eq!(chamada.decidido_por.as_deref(), Some("regra:Write"));
        assert_eq!(b.banco.lock().unwrap().listar_regras(&b.workspace_id).unwrap().len(), 1);
    }

    #[test]
    fn rodar_comando_nunca_vira_permissao_permanente() {
        // Liberar Bash de uma vez seria entregar a máquina num clique que
        // ninguém lembra uma semana depois.
        let b = balcao();
        let r = b.banco.lock().unwrap().conceder_regra(&b.workspace_id, "Bash");
        match r {
            Err(Erro::Invalido(m)) => assert!(m.contains("máquina"), "mensagem: {m}"),
            outro => panic!("esperava recusa, veio {outro:?}"),
        }
        assert!(b.banco.lock().unwrap().conceder_regra(&b.workspace_id, "Write").is_ok());
    }

    #[test]
    fn regra_concedida_e_revogavel_e_conceder_duas_vezes_nao_duplica() {
        let b = balcao();
        let banco = b.banco.lock().unwrap();
        let r1 = banco.conceder_regra(&b.workspace_id, "Write").unwrap();
        let r2 = banco.conceder_regra(&b.workspace_id, "Write").unwrap();
        assert_eq!(r1.id, r2.id, "revogar precisa apagar tudo, não a metade");
        assert_eq!(banco.listar_regras(&b.workspace_id).unwrap().len(), 1);
        banco.revogar_regra(&r1.id).unwrap();
        assert!(banco.listar_regras(&b.workspace_id).unwrap().is_empty());
        assert!(banco.regra_para(&b.workspace_id, "Write").unwrap().is_none());
    }

    #[test]
    fn card_que_ninguem_responde_acaba_negado_e_nao_pendurado() {
        // Um pedido esperando para sempre deixa o processo do agente de pé e o
        // nó travado sem explicação.
        let b = balcao();
        let corpo = b.corpo("Write", serde_json::json!({"file_path": "x", "content": "y"}));
        let v = tratar(
            &b.banco,
            &b.aprovacoes,
            &b.sink,
            &b.token,
            &corpo,
            Duration::from_millis(120),
        )
        .unwrap();
        assert!(!v.permitir);
        let chamada = b.banco.lock().unwrap().obter_ferramenta(&b.id_da_chamada()).unwrap();
        assert_eq!(chamada.aprovacao, Aprovacao::Negada);
        assert_eq!(chamada.decidido_por.as_deref(), Some("prazo"));
        assert_eq!(b.aprovacoes.quantas_esperando(), 0, "não pode sobrar canal órfão");
    }

    #[test]
    fn decidir_duas_vezes_o_mesmo_card_nao_passa() {
        let b = balcao();
        let corpo = b.corpo("Write", serde_json::json!({"file_path": "x", "content": "y"}));
        let pedido = b.pedir(corpo, Duration::from_secs(30));
        assert!(b.esperar_card());
        b.orq.decidir_aprovacao(&b.id_da_chamada(), Decisao::Aprovada, false).unwrap();
        pedido.join().unwrap();
        // O segundo clique não pode reabrir uma decisão já tomada.
        assert!(b
            .orq
            .decidir_aprovacao(&b.id_da_chamada(), Decisao::Negada, false)
            .is_err());
    }

    #[test]
    fn o_veredito_sai_no_formato_que_a_cli_espera() {
        // Formato medido na CLI 2.1.251, não deduzido: um `permissionDecision`
        // com outro nome faz a CLI ignorar a resposta e perguntar ao vazio.
        let v = Veredito { permitir: false, motivo: "não".into() };
        let j = v.como_json();
        assert_eq!(j["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(j["hookSpecificOutput"]["permissionDecision"], "deny");
        assert_eq!(j["hookSpecificOutput"]["permissionDecisionReason"], "não");
        let sim = Veredito { permitir: true, motivo: "ok".into() };
        assert_eq!(sim.como_json()["hookSpecificOutput"]["permissionDecision"], "allow");
    }

    #[test]
    fn o_card_descreve_a_acao_em_portugues_de_gente() {
        let (r, d) = descrever_ferramenta(
            "Write",
            &serde_json::json!({"file_path": "/a/b/orçamento.xlsx", "content": "x\ny"}),
        );
        assert_eq!(r, "Gravar orçamento.xlsx");
        assert!(d.starts_with("2 linhas"), "detalhe: {d}");

        let (r, d) = descrever_ferramenta("Bash", &serde_json::json!({"command": "rm -rf /"}));
        assert_eq!(r, "Rodar um comando");
        assert_eq!(d, "rm -rf /", "o comando inteiro precisa aparecer antes do clique");

        // Ferramenta nova: mostrar o nome cru é melhor que inventar um verbo
        // errado para algo que o usuário está prestes a autorizar.
        let (r, _) = descrever_ferramenta("FerramentaQueNaoExiste", &serde_json::json!({}));
        assert_eq!(r, "Usar FerramentaQueNaoExiste");
    }

    #[test]
    fn o_barramento_sobe_em_porta_propria_e_so_no_localhost() {
        let b = balcao();
        let barramento =
            Barramento::subir(b.banco.clone(), b.aprovacoes.clone(), b.sink.clone()).unwrap();
        assert!(barramento.porta() > 0);
        assert!(barramento.url_de_aprovacao().starts_with("http://127.0.0.1:"));
        // Porta escolhida pelo sistema: duas cópias do app não brigam, e não
        // existe alvo previsível para quem estiver na mesma máquina.
        let outro =
            Barramento::subir(b.banco.clone(), b.aprovacoes.clone(), b.sink.clone()).unwrap();
        assert_ne!(barramento.porta(), outro.porta());
    }

    // ---- escopo de arquivos ----------------------------------------------
    //
    // "Qualquer caminho que escape da pasta é negado antes de chegar ao disco"
    // — ARQUITETURA.md §8. Esta é a fronteira que separa "o agente trabalha na
    // minha pasta" de "o agente tem a minha máquina".

    use crate::arquivos::{arquivo_da_nota, dentro_do_escopo, escrever_texto, ler_texto, listar};

    struct Pasta(std::path::PathBuf);

    impl Pasta {
        fn nova() -> Pasta {
            let p = std::env::temp_dir().join(format!("mutirao-escopo-{}", novo_id()));
            std::fs::create_dir_all(&p).unwrap();
            Pasta(p)
        }
    }

    impl Drop for Pasta {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn caminho_que_sai_da_pasta_e_recusado() {
        let p = Pasta::nova();
        for fuga in [
            "../fora.txt",
            "a/../../fora.txt",
            "/etc/passwd",
            "sub/../../fora.txt",
        ] {
            assert!(
                matches!(dentro_do_escopo(&p.0, fuga), Err(Erro::ForaDoEscopo)),
                "deixou passar: {fuga}"
            );
        }
        // E o caminho honesto continua passando.
        assert!(dentro_do_escopo(&p.0, "contratos/minuta.docx").is_ok());
        assert!(dentro_do_escopo(&p.0, "./nota.md").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn link_simbolico_para_fora_tambem_e_recusado() {
        // Este é o caso que uma checagem de texto deixa passar: o caminho não
        // tem `..` nenhum e mesmo assim sai da pasta. Só resolver o caminho de
        // verdade pega.
        let p = Pasta::nova();
        let fora = std::env::temp_dir().join(format!("mutirao-fora-{}", novo_id()));
        std::fs::create_dir_all(&fora).unwrap();
        std::fs::write(fora.join("segredo.txt"), "não deveria dar para ler").unwrap();
        std::os::unix::fs::symlink(&fora, p.0.join("atalho")).unwrap();

        assert!(matches!(
            dentro_do_escopo(&p.0, "atalho/segredo.txt"),
            Err(Erro::ForaDoEscopo)
        ));
        assert!(ler_texto(&p.0, "atalho/segredo.txt").is_err());
        let _ = std::fs::remove_dir_all(&fora);
    }

    #[test]
    fn grava_e_le_dentro_da_pasta_criando_subpasta() {
        let p = Pasta::nova();
        // Arquivo que ainda não existe: o escopo se resolve pelo ancestral que
        // existe, senão não daria para criar nada.
        escrever_texto(&p.0, "notas/briefing.md", "# Briefing\n").unwrap();
        assert_eq!(ler_texto(&p.0, "notas/briefing.md").unwrap(), "# Briefing\n");
        assert!(p.0.join("notas").join("briefing.md").exists());
    }

    #[test]
    fn listar_esconde_o_que_comeca_com_ponto_e_poe_pasta_primeiro() {
        let p = Pasta::nova();
        std::fs::create_dir_all(p.0.join("contratos")).unwrap();
        std::fs::create_dir_all(p.0.join(".mutirao")).unwrap();
        std::fs::write(p.0.join("anexo.pdf"), "x").unwrap();
        std::fs::write(p.0.join("Ata.docx"), "yy").unwrap();

        let itens = listar(&p.0, "").unwrap();
        let nomes: Vec<&str> = itens.iter().map(|i| i.nome.as_str()).collect();
        // O Git oculto do §3 não aparece: o usuário vê "Rascunho 2", nunca
        // `.mutirao`.
        assert_eq!(nomes, vec!["contratos", "anexo.pdf", "Ata.docx"], "veio {nomes:?}");
        assert!(itens[0].pasta);
        assert_eq!(itens.iter().find(|i| i.nome == "Ata.docx").unwrap().tamanho, 2);
    }

    #[test]
    fn arquivo_binario_nao_finge_ser_texto() {
        let p = Pasta::nova();
        std::fs::write(p.0.join("imagem.png"), [0xff, 0xd8, 0xff, 0x00, 0x9c]).unwrap();
        match ler_texto(&p.0, "imagem.png") {
            Err(Erro::Invalido(m)) => assert!(m.contains("não é texto"), "mensagem: {m}"),
            outro => panic!("esperava recusa, veio {outro:?}"),
        }
    }

    #[test]
    fn nome_de_nota_vira_arquivo_que_o_windows_aceita() {
        assert_eq!(arquivo_da_nota("Briefing"), "Briefing.md");
        assert_eq!(arquivo_da_nota("Ata da reunião"), "Ata da reunião.md");
        // Estes o Windows recusa em nome de arquivo; virar traço é melhor que
        // falhar na hora de gravar.
        assert_eq!(arquivo_da_nota("a/b:c*d?"), "a-b-c-d-.md");
        assert_eq!(arquivo_da_nota("   "), "nota.md");
        // Ponto nas pontas some: um nó chamado ".oculto" não pode virar um
        // arquivo que o usuário não enxerga na própria pasta.
        assert_eq!(arquivo_da_nota("../fuga"), "-fuga.md");
        assert_eq!(arquivo_da_nota(".oculto"), "oculto.md");
        // E o resultado sempre fica dentro do escopo.
        let p = Pasta::nova();
        assert!(dentro_do_escopo(&p.0, &arquivo_da_nota("../fuga")).is_ok());
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
