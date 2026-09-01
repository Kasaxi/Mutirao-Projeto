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
pub mod ensaios;
pub mod erro;
pub mod git;
pub mod ferramentas;
pub mod mcp;
pub mod modelo;
pub mod orquestrador;
pub mod papeis;
pub mod partituras;
pub mod processo;

pub use agente::{AdaptadorFalso, AgenteAdapter, ContextoSessao, Fabrica, FabricaFalsa, Roteiro};
pub use arquivos::ItemArquivo;
pub use barramento::{Aprovacoes, Barramento};
pub use claude::{AdaptadorClaude, FabricaClaude};
pub use db::Banco;
pub use papeis::{ferramentas_do_papel, pode_recrutar};
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
    const VERSAO_ESQUEMA: i64 = 6;

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
        /// `Arc` porque desde o M3 o orquestrador precisa se clonar para dentro
        /// da thread de cada turno — é assim que a bomba de eventos consegue
        /// puxar o próximo da fila quando o turno acaba.
        orq: Arc<Orquestrador>,
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

        let orq = Arc::new(Orquestrador::novo(
            banco.clone(),
            Arc::new(FabricaFalsa::com_roteiro(roteiro)),
            sink,
        ));
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
    fn dois_turnos_ao_mesmo_tempo_no_mesmo_no_viram_fila() {
        // A regra "um turno por vez por nó" continua valendo — duas mensagens
        // intercaladas produzem contexto embaralhado e resposta sem sentido. O
        // que mudou no M3 foi o desfecho de quem chega no meio: em vez de
        // recusa, fila. Recusar era defensável quando só o usuário falava com o
        // nó; com outro nó do outro lado, recusar é perder trabalho.
        let b = bancada(Roteiro { atraso_ms: 30, ..roteiro_simples() });
        b.orq.enviar(&b.sessao.id, "primeira").unwrap();
        assert!(b.esperar(|b| b.estado() == EstadoSessao::Pensando));

        b.orq.enviar(&b.sessao.id, "segunda").expect("a segunda entra na fila");
        // Enfileirada é diferente de começada: enquanto o primeiro turno corre,
        // a segunda não pode ter virado linha no histórico.
        assert!(
            b.historico().iter().all(|m| m.conteudo != "segunda"),
            "a segunda começou junto com a primeira"
        );

        // E o segundo turno sai sozinho, sem ninguém falar de novo com o nó.
        assert!(b.esperar(|b| b.historico().iter().filter(|m| m.papel == PapelMensagem::Agente)
            .count()
            == 2));
        assert!(b.esperar_ocioso());

        let ordem: Vec<&str> = b
            .historico()
            .iter()
            .filter(|m| m.papel == PapelMensagem::Usuario)
            .map(|m| if m.conteudo == "primeira" { "1" } else { "2" })
            .collect();
        assert_eq!(ordem, vec!["1", "2"], "a fila é em ordem de chegada");
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
        orq: Arc<Orquestrador>,
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

        let orq = Arc::new(Orquestrador::novo(
            banco.clone(),
            Arc::new(FabricaFalsa::demonstracao()),
            sink.clone(),
        ));
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
            Barramento::subir(b.banco.clone(), b.orq.clone(), b.sink.clone()).unwrap();
        assert!(barramento.porta() > 0);
        assert!(barramento.url_de_aprovacao().starts_with("http://127.0.0.1:"));
        // Porta escolhida pelo sistema: duas cópias do app não brigam, e não
        // existe alvo previsível para quem estiver na mesma máquina.
        let outro =
            Barramento::subir(b.banco.clone(), b.orq.clone(), b.sink.clone()).unwrap();
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

    // ================================================================ M3 ===
    // A ponte. Um nó fala com outro pelas ferramentas do §6, e o que ele
    // enxerga do canvas é decidido pelos cabos — só por eles.

    use crate::ferramentas;
    use serde_json::json;

    /// Dois agentes ligados por `fala_com` e uma nota que só um deles escreve.
    struct Ponte {
        banco: Arc<Mutex<Banco>>,
        orq: Arc<Orquestrador>,
        /// Pesquisador, quem começa a conversa.
        a: Sessao,
        /// Redator, quem responde.
        b_no: String,
        avisos: Arc<Mutex<Vec<EventoNucleo>>>,
        _pasta: Pasta,
    }

    /// O Pesquisador já no meio de um turno, que é de onde o §6 permite as
    /// transições para `aguardando_no` e `aguardando_humano`. Serve aos testes
    /// que chamam a ferramenta direto, sem passar por um adaptador.
    fn ponte() -> Ponte {
        ponte_com(Arc::new(FabricaFalsa::com_roteiro(roteiro_simples())), true)
    }

    fn ponte_com(fabrica: Arc<dyn Fabrica>, em_turno: bool) -> Ponte {
        let pasta = Pasta::nova();
        let banco = Banco::em_memoria().unwrap();
        let ws = banco.criar_workspace("Obra", pasta.0.to_str().unwrap()).unwrap();
        let a = banco.criar_no(&ws.id, TipoNo::Agente, "Pesquisador", 0.0, 0.0).unwrap();
        let b = banco.criar_no(&ws.id, TipoNo::Agente, "Redator", 300.0, 0.0).unwrap();
        // Existe, mas sem cabo: é o nó que precisa ser invisível.
        banco.criar_no(&ws.id, TipoNo::Agente, "Isolado", 600.0, 0.0).unwrap();
        let nota = banco.criar_no(&ws.id, TipoNo::Nota, "Briefing", 0.0, 300.0).unwrap();
        let outra = banco.criar_no(&ws.id, TipoNo::Nota, "Sigilo", 300.0, 300.0).unwrap();

        banco.criar_cabo(&ws.id, &a.id, &b.id, TipoCabo::FalaCom).unwrap();
        banco.criar_cabo(&ws.id, &a.id, &nota.id, TipoCabo::LeNota).unwrap();
        banco.criar_cabo(&ws.id, &a.id, &nota.id, TipoCabo::EscreveNota).unwrap();
        // `Sigilo` fica ligada só ao Redator: o Pesquisador não pode vê-la.
        banco.criar_cabo(&ws.id, &b.id, &outra.id, TipoCabo::LeNota).unwrap();

        let banco = Arc::new(Mutex::new(banco));
        let avisos: Arc<Mutex<Vec<EventoNucleo>>> = Arc::new(Mutex::new(Vec::new()));
        let copia = avisos.clone();
        let sink: Sink = Arc::new(move |e| copia.lock().unwrap().push(e));

        let orq = Arc::new(Orquestrador::novo(banco.clone(), fabrica, sink));
        let sessao_a = orq.abrir_sessao(&a.id, Adaptador::Falso).unwrap();
        if em_turno {
            banco.lock().unwrap().mudar_estado_sessao(&sessao_a.id, EstadoSessao::Pensando).unwrap();
        }

        Ponte { banco, orq, a: sessao_a, b_no: b.id, avisos, _pasta: pasta }
    }

    impl Ponte {
        fn usar(&self, nome: &str, args: serde_json::Value) -> Resultado<serde_json::Value> {
            ferramentas::executar(&self.orq, &self.banco, &self.a, nome, &args)
        }

        fn historico_de(&self, node_id: &str) -> Vec<Mensagem> {
            let banco = self.banco.lock().unwrap();
            let s = banco.sessao_do_no(node_id).unwrap().unwrap();
            banco.historico(&s.id, 100).unwrap()
        }

        fn eventos(&self) -> Vec<EventoNucleo> {
            self.avisos.lock().unwrap().clone()
        }
    }

    // ---- escopo pelos cabos ----------------------------------------------

    #[test]
    fn no_sem_cabo_simplesmente_nao_existe() {
        // A frase precisa ser a MESMA nos dois casos. Duas mensagens
        // diferentes — "não existe" versus "existe mas você não pode" — fazem
        // de cada tentativa uma sonda que mapeia o canvas inteiro.
        let p = ponte();
        let desligado = p
            .usar("enviar_para", json!({ "no": "Isolado", "mensagem": "oi" }))
            .expect_err("nó sem cabo não devia ser alcançável");
        let inexistente = p
            .usar("enviar_para", json!({ "no": "Fantasma", "mensagem": "oi" }))
            .expect_err("nó que não existe não devia ser alcançável");
        assert_eq!(desligado.to_string(), inexistente.to_string().replace("Fantasma", "Isolado"));
        assert!(desligado.to_string().contains("Isolado"), "{desligado}");
    }

    #[test]
    fn listar_nos_mostra_so_o_que_os_cabos_deixam_ver() {
        let p = ponte();
        let v = p.usar("listar_nos", json!({})).unwrap();
        let nomes: Vec<String> = v["nos"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["nome"].as_str().unwrap().to_string())
            .collect();
        assert!(nomes.contains(&"Redator".to_string()));
        assert!(nomes.contains(&"Briefing".to_string()));
        assert!(!nomes.contains(&"Isolado".to_string()), "vazou nó sem cabo: {nomes:?}");
        assert!(!nomes.contains(&"Sigilo".to_string()), "vazou nota de outro nó: {nomes:?}");
    }

    #[test]
    fn ler_nota_sem_cabo_de_leitura_e_recusado() {
        let p = ponte();
        assert!(p.usar("ler_nota", json!({ "nota": "Sigilo" })).is_err());
        // E a que tem cabo passa, mesmo antes de o arquivo existir.
        assert_eq!(p.usar("ler_nota", json!({ "nota": "Briefing" })).unwrap()["conteudo"], "");
    }

    #[test]
    fn escrever_nota_grava_o_md_na_pasta_do_workspace() {
        let p = ponte();
        p.usar("escrever_nota", json!({ "nota": "Briefing", "conteudo": "primeira linha\n" }))
            .unwrap();
        p.usar(
            "escrever_nota",
            json!({ "nota": "Briefing", "conteudo": "segunda", "modo": "acrescentar" }),
        )
        .unwrap();
        assert_eq!(
            p.usar("ler_nota", json!({ "nota": "Briefing" })).unwrap()["conteudo"],
            "primeira linha\nsegunda"
        );
        assert_eq!(
            std::fs::read_to_string(p._pasta.0.join("Briefing.md")).unwrap(),
            "primeira linha\nsegunda"
        );
    }

    // ---- a ponte A→B -----------------------------------------------------

    #[test]
    fn enviar_para_espera_a_resposta_do_outro_no() {
        let p = ponte();
        let r = p
            .usar("enviar_para", json!({ "no": "Redator", "mensagem": "resuma o contrato" }))
            .unwrap();
        assert_eq!(r["resposta"], "O item 4.2 contradiz o anexo I.");
        assert_eq!(r["de"], "Redator");

        // Do lado do Redator ficou registrado quem falou e em que cadeia.
        let dele = p.historico_de(&p.b_no);
        let recado = dele.iter().find(|m| m.papel == PapelMensagem::No).expect("recado do nó");
        assert_eq!(recado.conteudo, "resuma o contrato");
        assert_eq!(recado.origem_node.as_deref(), Some(p.a.node_id.as_str()));
        assert!(recado.trace_id.is_some(), "recado sem cadeia");

        // E a interface soube, para poder animar o cabo.
        let animou = p.eventos().iter().any(|e| {
            matches!(e, EventoNucleo::NoMensagem { para_node, tipo_mensagem, .. }
                if para_node == &p.b_no && *tipo_mensagem == TipoMensagem::Pedido)
        });
        assert!(animou, "nenhum no:mensagem para animar o cabo");
    }

    #[test]
    fn avisar_entrega_e_volta_na_hora() {
        let p = ponte();
        let r = p.usar("avisar", json!({ "no": "Redator", "mensagem": "terminei a parte 1" })).unwrap();
        assert_eq!(r["entregue"], true);
        assert!(r.get("resposta").is_none(), "aviso não devia esperar resposta");
    }

    #[test]
    fn as_refs_citadas_chegam_junto_com_o_recado() {
        let p = ponte();
        p.usar("enviar_para", json!({
            "no": "Redator",
            "mensagem": "escreva a introdução",
            "refs": ["Briefing", "contrato.txt"],
        }))
        .unwrap();
        let recado = p
            .historico_de(&p.b_no)
            .into_iter()
            .find(|m| m.papel == PapelMensagem::No)
            .expect("recado do nó");
        assert!(recado.conteudo.contains("Briefing"), "{}", recado.conteudo);
        assert!(recado.conteudo.contains("contrato.txt"), "{}", recado.conteudo);
    }

    #[test]
    fn nome_ambiguo_nao_vira_chute() {
        let p = ponte();
        let banco = p.banco.lock().unwrap();
        let ws = banco.obter_no(&p.a.node_id).unwrap().workspace_id;
        // Um segundo nó com o MESMO nome do Pesquisador, ligado a ele.
        let gemeo = banco.criar_no(&ws, TipoNo::Agente, "Redator", 900.0, 0.0).unwrap();
        banco.criar_cabo(&ws, &p.a.node_id, &gemeo.id, TipoCabo::FalaCom).unwrap();
        drop(banco);

        // Agora "Redator" é ambíguo: escolher um dos dois mandaria o recado
        // para o lugar errado sem ninguém perceber.
        let e = p
            .usar("enviar_para", json!({ "no": "Redator", "mensagem": "oi" }))
            .expect_err("nome ambíguo devia ser erro");
        assert!(e.to_string().contains("mais de um"), "{e}");
    }

    // ---- os três limites -------------------------------------------------

    #[test]
    fn a_cadeia_para_no_limite_de_saltos() {
        // O limite é do host, não do agente: um agente convencido de que
        // precisa de mais uma rodada sempre acha um motivo.
        let mut t = Trace::novo();
        for _ in 0..MAX_SALTOS {
            t = t.saltar().expect("dentro do limite");
        }
        assert_eq!(t.saltos, MAX_SALTOS);
        assert!(t.saltar().is_none(), "passou de {MAX_SALTOS} saltos");
        // O id não muda no caminho: é a MESMA cadeia andando, e é por ele que o
        // orçamento soma o gasto de todos os nós que ela atravessou.
        assert_eq!(t.id, Trace::novo().saltar().map(|_| t.id.clone()).unwrap());
        assert_ne!(Trace::novo().id, Trace::novo().id, "cada pedido abre a sua cadeia");
    }

    #[test]
    fn a_cadeia_para_quando_estoura_o_orcamento() {
        // O pior desfecho de um ciclo malcomportado não é travar — é não
        // travar, e queimar crédito a noite inteira em silêncio.
        let p = ponte_com(Arc::new(FabricaTravada::nova()), false);

        // Um turno de verdade, que fica aberto: é ele que fixa a cadeia sobre
        // a qual o orçamento incide.
        p.orq.enviar(&p.a.id, "comece").unwrap();
        assert!(esperar(|| p.banco.lock().unwrap().obter_sessao(&p.a.id).unwrap().estado
            == EstadoSessao::Pensando));

        let trace = p
            .historico_de(&p.a.node_id)
            .into_iter()
            .find_map(|m| m.trace_id)
            .expect("o turno abriu uma cadeia");

        // Antes do estouro, a ponte funciona.
        assert!(p.usar("avisar", json!({ "no": "Redator", "mensagem": "oi" })).is_ok());

        // Agora a cadeia fica cara.
        p.banco
            .lock()
            .unwrap()
            .gravar_mensagem_completa(
                &p.a.id,
                PapelMensagem::Agente,
                "turno caro",
                Uso { tokens_entrada: 0, tokens_saida: 0, custo_usd: ORCAMENTO_POR_TRACE_USD },
                Some(&trace),
                None,
            )
            .unwrap();

        let e = p
            .usar("enviar_para", json!({ "no": "Redator", "mensagem": "mais uma rodada" }))
            .expect_err("o orçamento devia ter barrado");
        assert!(e.to_string().contains("orçamento"), "{e}");

        // E o usuário soube: estourar limite avisa, não queima crédito calado.
        let avisou = p.eventos().iter().any(|ev| {
            matches!(ev, EventoNucleo::CadeiaEncerrada { trace_id, motivo, .. }
                if trace_id == &trace && motivo.contains("US$"))
        });
        assert!(avisou, "a cadeia acabou em silêncio: {:?}", p.eventos());
    }

    #[test]
    fn o_prazo_pedido_pelo_agente_tem_teto() {
        // Sem teto, um agente pediria um prazo de dias e prenderia o nó — o
        // pior desfecho, porque um nó preso não pede atenção e não explica nada.
        assert_eq!(ferramentas::prazo_pedido(&json!({})), PRAZO_MENSAGEM_PADRAO_MS);
        assert_eq!(ferramentas::prazo_pedido(&json!({ "prazo_ms": 5_000 })), 5_000);
        assert_eq!(
            ferramentas::prazo_pedido(&json!({ "prazo_ms": 999_999_999u64 })),
            PRAZO_MENSAGEM_TETO_MS
        );
        // Zero seria "não espere nada" — vira o padrão, não uma falha imediata.
        assert_eq!(ferramentas::prazo_pedido(&json!({ "prazo_ms": 0 })), PRAZO_MENSAGEM_PADRAO_MS);
    }

    /// Adaptador que começa o turno e não termina nunca. Serve para segurar uma
    /// cadeia aberta enquanto o teste mexe nela.
    struct FabricaTravada {
        /// Guarda os remetentes vivos: soltos, o canal fecharia e a bomba de
        /// eventos daria o turno por morto.
        presos: Arc<Mutex<Vec<std::sync::mpsc::Sender<EventoAgente>>>>,
    }

    impl FabricaTravada {
        fn nova() -> FabricaTravada {
            FabricaTravada { presos: Arc::new(Mutex::new(Vec::new())) }
        }
    }

    impl Fabrica for FabricaTravada {
        fn criar(&self, _a: Adaptador, _c: &ContextoSessao) -> Resultado<Box<dyn AgenteAdapter>> {
            Ok(Box::new(AdaptadorTravado { presos: self.presos.clone() }))
        }
    }

    struct AdaptadorTravado {
        presos: Arc<Mutex<Vec<std::sync::mpsc::Sender<EventoAgente>>>>,
    }

    impl AgenteAdapter for AdaptadorTravado {
        fn turno(&mut self, _t: &str) -> Resultado<std::sync::mpsc::Receiver<EventoAgente>> {
            let (tx, rx) = std::sync::mpsc::channel();
            self.presos.lock().unwrap().push(tx);
            Ok(rx)
        }
        fn cancelar(&mut self) {
            self.presos.lock().unwrap().clear();
        }
    }

    /// Espera uma condição por até dois segundos. Os turnos rodam em thread;
    /// ler o banco na hora seguinte à chamada leria o estado de antes.
    fn esperar(cond: impl Fn() -> bool) -> bool {
        for _ in 0..400 {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    // ---- espera cruzada ---------------------------------------------------

    /// Um adaptador que, a cada turno, pergunta ao vizinho — e só devolve
    /// resposta quando o vizinho responder. Dois deles apontando um para o
    /// outro é a espera circular que o `fecharia_ciclo` existe para pegar.
    struct FabricaTeimosa {
        orq: Arc<Mutex<Option<Arc<Orquestrador>>>>,
        banco: Arc<Mutex<Banco>>,
    }

    struct AdaptadorTeimoso {
        orq: Arc<Mutex<Option<Arc<Orquestrador>>>>,
        banco: Arc<Mutex<Banco>>,
        session_id: String,
        node_id: String,
    }

    impl Fabrica for FabricaTeimosa {
        fn criar(&self, _a: Adaptador, ctx: &ContextoSessao) -> Resultado<Box<dyn AgenteAdapter>> {
            Ok(Box::new(AdaptadorTeimoso {
                orq: self.orq.clone(),
                banco: self.banco.clone(),
                session_id: ctx.session_id.clone(),
                node_id: ctx.node_id.clone(),
            }))
        }
    }

    impl AgenteAdapter for AdaptadorTeimoso {
        fn turno(&mut self, texto: &str) -> Resultado<std::sync::mpsc::Receiver<EventoAgente>> {
            let (tx, rx) = std::sync::mpsc::channel();
            let orq = self.orq.lock().unwrap().clone().expect("orquestrador ligado");
            let banco = self.banco.clone();
            let session_id = self.session_id.clone();
            let node_id = self.node_id.clone();
            let texto = texto.to_string();

            std::thread::spawn(move || {
                let (sessao, vizinho) = {
                    let b = banco.lock().unwrap();
                    let s = b.obter_sessao(&session_id).unwrap();
                    let v = b
                        .vizinhos(&node_id, TipoCabo::FalaCom)
                        .unwrap()
                        .first()
                        .map(|id| b.obter_no(id).unwrap().nome);
                    (s, v)
                };
                let final_ = match vizinho {
                    Some(nome) => {
                        let r = ferramentas::executar(
                            &orq,
                            &banco,
                            &sessao,
                            "enviar_para",
                            &json!({ "no": nome, "mensagem": format!("e você? ({texto})") }),
                        );
                        match r {
                            Ok(v) => format!("o vizinho disse: {v}"),
                            Err(e) => format!("não deu para perguntar: {e}"),
                        }
                    }
                    None => "não tenho com quem falar".to_string(),
                };
                let _ = tx.send(EventoAgente::TurnoConcluido {
                    texto_final: final_,
                    uso: Uso::default(),
                });
            });
            Ok(rx)
        }

        fn cancelar(&mut self) {}
    }

    #[test]
    fn dois_nos_esperando_um_pelo_outro_nao_travam_o_app() {
        // É a promessa do M3 em uma frase: "um ciclo A→B→A encerra sozinho sem
        // travar o app". O limite de saltos não pega este caso — saltos só
        // contam quando alguém consegue andar — e o prazo pegaria em dez
        // minutos, que para quem está olhando a tela é travar.
        let celula: Arc<Mutex<Option<Arc<Orquestrador>>>> = Arc::new(Mutex::new(None));
        let pasta = Pasta::nova();
        let banco = Banco::em_memoria().unwrap();
        let ws = banco.criar_workspace("Obra", pasta.0.to_str().unwrap()).unwrap();
        let a = banco.criar_no(&ws.id, TipoNo::Agente, "Pesquisador", 0.0, 0.0).unwrap();
        let b = banco.criar_no(&ws.id, TipoNo::Agente, "Redator", 300.0, 0.0).unwrap();
        // Um cabo só: `vizinhos` enxerga os dois sentidos, então cada um vê o
        // outro e os dois querem perguntar.
        banco.criar_cabo(&ws.id, &a.id, &b.id, TipoCabo::FalaCom).unwrap();
        let banco = Arc::new(Mutex::new(banco));

        let fabrica: Arc<dyn Fabrica> =
            Arc::new(FabricaTeimosa { orq: celula.clone(), banco: banco.clone() });
        let orq = Arc::new(Orquestrador::novo(banco.clone(), fabrica, sink_mudo()));
        *celula.lock().unwrap() = Some(orq.clone());

        let sessao = orq.abrir_sessao(&a.id, Adaptador::Falso).unwrap();
        let inicio = std::time::Instant::now();
        orq.enviar(&sessao.id, "comece").unwrap();

        // Os dois nós têm de voltar a ficar parados, e depressa.
        let parou = (0..600).any(|_| {
            let b = banco.lock().unwrap();
            let ea = b.obter_sessao(&sessao.id).unwrap().estado;
            let sb = b.sessao_do_no(&b_id(&b, &ws.id, "Redator")).unwrap();
            let eb = sb.map(|s| s.estado).unwrap_or(EstadoSessao::Ocioso);
            drop(b);
            let quietos = !matches!(ea, EstadoSessao::Pensando | EstadoSessao::AguardandoNo)
                && !matches!(eb, EstadoSessao::Pensando | EstadoSessao::AguardandoNo);
            if !quietos {
                std::thread::sleep(Duration::from_millis(10));
            }
            quietos
        });
        assert!(parou, "os dois nós ficaram travados um pelo outro");
        assert!(
            inicio.elapsed() < Duration::from_secs(20),
            "levou {:?} — o prazo não pode ser o que desata isto",
            inicio.elapsed()
        );

        // E o modelo recebeu uma instrução do que fazer, não um erro mudo.
        let dito = banco
            .lock()
            .unwrap()
            .historico(&sessao.id, 50)
            .unwrap()
            .iter()
            .map(|m| m.conteudo.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            dito.contains("esperando a SUA resposta"),
            "desatou por outro caminho que não a detecção de ciclo: {dito}"
        );
    }

    /// Acha um nó pelo nome dentro do workspace. Só para o teste acima ler o
    /// estado do outro lado da ponte.
    fn b_id(banco: &Banco, workspace_id: &str, nome: &str) -> String {
        banco
            .listar_nos(workspace_id)
            .unwrap()
            .into_iter()
            .find(|n| n.nome == nome)
            .expect("nó existe")
            .id
    }

    // ---- perguntar ao humano e concluir ----------------------------------

    #[test]
    fn perguntar_humano_para_o_no_ate_a_proxima_mensagem_do_usuario() {
        let p = ponte();
        let orq = p.orq.clone();
        let sessao = p.a.clone();
        let banco = p.banco.clone();
        let pergunta = std::thread::spawn(move || {
            ferramentas::executar(
                &orq,
                &banco,
                &sessao,
                "perguntar_humano",
                &json!({ "pergunta": "uso o índice velho ou o novo?", "opcoes": ["velho", "novo"] }),
            )
        });

        // O nó pede atenção e fica parado.
        let pediu = (0..400).any(|_| {
            let e = p.banco.lock().unwrap().obter_sessao(&p.a.id).unwrap().estado;
            if e != EstadoSessao::AguardandoHumano {
                std::thread::sleep(Duration::from_millis(5));
                return false;
            }
            true
        });
        assert!(pediu, "o nó não entrou em aguardando_humano");

        // A próxima mensagem do usuário é a RESPOSTA, não um turno novo.
        p.orq.enviar(&p.a.id, "o novo").unwrap();
        let r = pergunta.join().unwrap().unwrap();
        assert_eq!(r["resposta"], "o novo");

        // E as opções apareceram na conversa, senão o usuário responde no escuro.
        let conversa = p.historico_de(&p.a.node_id);
        assert!(conversa.iter().any(|m| m.conteudo.contains("velho · novo")), "{conversa:?}");
    }

    /// Um adaptador que, ao receber turno, levanta a mão para a pessoa em vez
    /// de responder. É o que o Redator fez ao vivo e derrubou o
    /// `um_ciclo_entre_dois_nos_encerra_sozinho`.
    struct FabricaPerguntadora {
        orq: Arc<Mutex<Option<Arc<Orquestrador>>>>,
        banco: Arc<Mutex<Banco>>,
    }

    struct AdaptadorPerguntador {
        orq: Arc<Mutex<Option<Arc<Orquestrador>>>>,
        banco: Arc<Mutex<Banco>>,
        session_id: String,
        node_id: String,
    }

    impl Fabrica for FabricaPerguntadora {
        fn criar(&self, _a: Adaptador, ctx: &ContextoSessao) -> Resultado<Box<dyn AgenteAdapter>> {
            Ok(Box::new(AdaptadorPerguntador {
                orq: self.orq.clone(),
                banco: self.banco.clone(),
                session_id: ctx.session_id.clone(),
                node_id: ctx.node_id.clone(),
            }))
        }
    }

    impl AgenteAdapter for AdaptadorPerguntador {
        fn turno(&mut self, _texto: &str) -> Resultado<std::sync::mpsc::Receiver<EventoAgente>> {
            let (tx, rx) = std::sync::mpsc::channel();
            let orq = self.orq.lock().unwrap().clone().expect("orquestrador ligado");
            let banco = self.banco.clone();
            let session_id = self.session_id.clone();
            // Só o Redator pergunta; o Pesquisador é quem espera.
            let pergunta = self.node_id.clone();

            std::thread::spawn(move || {
                let sessao = banco.lock().unwrap().obter_sessao(&session_id).unwrap();
                let nome = banco.lock().unwrap().obter_no(&pergunta).unwrap().nome;
                let final_ = if nome == "Redator" {
                    match ferramentas::executar(
                        &orq,
                        &banco,
                        &sessao,
                        "perguntar_humano",
                        &json!({ "pergunta": "uso o índice velho ou o novo?" }),
                    ) {
                        Ok(v) => format!("a pessoa disse: {}", v["resposta"]),
                        Err(e) => format!("ninguém respondeu: {e}"),
                    }
                } else {
                    "nada a fazer".to_string()
                };
                let _ = tx.send(EventoAgente::TurnoConcluido {
                    texto_final: final_,
                    uso: Uso::default(),
                });
            });
            Ok(rx)
        }

        fn cancelar(&mut self) {}
    }

    #[test]
    fn quem_espera_nao_morre_no_prazo_enquanto_a_pessoa_pensa() {
        // Medido ao vivo, e é o que derrubava o teste do ciclo: o Pesquisador
        // esperando o Redator, o Redator perguntando à pessoa. Nenhum dos
        // limites pega — e nem deveria, porque nada travou: alguém precisa
        // responder. O que não pode é o relógio da entrega correr contra o
        // tempo de quem está pensando, e a cadeia inteira morrer por causa de
        // um café.
        let celula: Arc<Mutex<Option<Arc<Orquestrador>>>> = Arc::new(Mutex::new(None));
        let pasta = Pasta::nova();
        let banco = Banco::em_memoria().unwrap();
        let ws = banco.criar_workspace("Obra", pasta.0.to_str().unwrap()).unwrap();
        let a = banco.criar_no(&ws.id, TipoNo::Agente, "Pesquisador", 0.0, 0.0).unwrap();
        let b = banco.criar_no(&ws.id, TipoNo::Agente, "Redator", 300.0, 0.0).unwrap();
        banco.criar_cabo(&ws.id, &a.id, &b.id, TipoCabo::FalaCom).unwrap();
        let banco = Arc::new(Mutex::new(banco));

        let avisos: Arc<Mutex<Vec<EventoNucleo>>> = Arc::new(Mutex::new(Vec::new()));
        let copia = avisos.clone();
        let sink: Sink = Arc::new(move |e| copia.lock().unwrap().push(e));

        let fabrica: Arc<dyn Fabrica> =
            Arc::new(FabricaPerguntadora { orq: celula.clone(), banco: banco.clone() });
        let orq = Arc::new(Orquestrador::novo(banco.clone(), fabrica, sink));
        *celula.lock().unwrap() = Some(orq.clone());

        let sessao_a = orq.abrir_sessao(&a.id, Adaptador::Falso).unwrap();
        banco.lock().unwrap().mudar_estado_sessao(&sessao_a.id, EstadoSessao::Pensando).unwrap();

        // Prazo curto de propósito: com o relógio correndo, a entrega morreria
        // em um segundo. É a sabotagem do teste embutida no próprio teste.
        let entrega = {
            let (orq, banco, sessao_a) = (orq.clone(), banco.clone(), sessao_a.clone());
            std::thread::spawn(move || {
                ferramentas::executar(
                    &orq,
                    &banco,
                    &sessao_a,
                    "enviar_para",
                    &json!({ "no": "Redator", "mensagem": "faça a sua parte", "prazo_ms": 1000 }),
                )
            })
        };

        // O Redator levanta a mão.
        assert!(
            esperar(|| {
                let banco = banco.lock().unwrap();
                banco
                    .sessao_do_no(&b.id)
                    .ok()
                    .flatten()
                    .is_some_and(|s| s.estado == EstadoSessao::AguardandoHumano)
            }),
            "o Redator não chegou a perguntar"
        );

        // Muito depois do prazo de um segundo, a entrega continua de pé.
        std::thread::sleep(Duration::from_millis(2500));
        assert!(
            !entrega.is_finished(),
            "a entrega morreu no prazo enquanto a pessoa ainda decidia"
        );

        // E a tela foi avisada de quem depende de quem — senão o canvas mostra
        // dois nós calados e ninguém sabe que a fila espera um clique.
        let avisou = avisos.lock().unwrap().iter().any(|e| {
            matches!(
                e,
                EventoNucleo::CadeiaEsperaPessoa { node_id, perguntou_nome, .. }
                    if node_id == &a.id && perguntou_nome == "Redator"
            )
        });
        assert!(avisou, "ninguém avisou que a cadeia parou numa pergunta");

        // A pessoa responde, e a cadeia anda.
        let sessao_b = banco.lock().unwrap().sessao_do_no(&b.id).unwrap().unwrap();
        orq.enviar(&sessao_b.id, "o novo").unwrap();
        let r = entrega.join().unwrap().expect("a entrega devia ter dado certo");
        assert!(
            r.to_string().contains("o novo"),
            "a resposta da pessoa não voltou pela cadeia: {r}"
        );
    }

    #[test]
    fn recrutar_de_novo_devolve_quem_ja_existe_em_vez_de_um_bruno2() {
        // Medido ao vivo: com erro no lugar disto, o Organizador contornava o
        // nome ocupado inventando "Bruno2" — um nó a mais no canvas, turnos
        // queimados, e um time com dois Brunos em que ninguém sabe com qual
        // está falando. Ferramenta que o modelo repete tem de ser idempotente.
        let e = elenco();
        let primeiro =
            e.usar("recrutar", json!({ "papel": "Redator", "nome": "Bruno" })).unwrap();
        let segundo =
            e.usar("recrutar", json!({ "papel": "Redator", "nome": "Bruno" })).unwrap();
        assert_eq!(primeiro, segundo, "recrutar o mesmo devia devolver o mesmo");

        let quantos = e
            .banco
            .lock()
            .unwrap()
            .listar_nos(&e.ws)
            .unwrap()
            .iter()
            .filter(|n| n.nome == "Bruno")
            .count();
        assert_eq!(quantos, 1, "nasceu um segundo Bruno");
    }

    #[test]
    fn nome_ocupado_por_outro_papel_explica_quem_e_em_vez_de_convidar_ao_numero() {
        // A recusa continua existindo — dizer que a Ana virou Revisora quando
        // ela é Redatora seria mentir para o modelo, e mentira vira
        // comportamento estranho três turnos depois. O que muda é a mensagem:
        // ela diz QUEM já ocupa o nome, para o contorno óbvio não ser "Ana2".
        let e = elenco();
        e.usar("recrutar", json!({ "papel": "Redator", "nome": "Ana" })).unwrap();
        let err = e
            .usar("recrutar", json!({ "papel": "Revisor", "nome": "Ana" }))
            .expect_err("papel diferente com nome ocupado não devia passar");
        let texto = err.to_string();
        assert!(texto.contains("Redator"), "não disse quem é a Ana: {texto}");
        assert!(texto.contains("número"), "não desaconselhou o Ana2: {texto}");
    }

    #[test]
    fn concluir_vira_linha_na_conversa() {
        let p = ponte();
        p.usar("concluir", json!({ "resumo": "minuta revisada, 3 pontos abertos" })).unwrap();
        let conversa = p.historico_de(&p.a.node_id);
        assert!(
            conversa.iter().any(|m| m.conteudo == "Entregue: minuta revisada, 3 pontos abertos"),
            "{conversa:?}"
        );
    }

    #[test]
    fn ferramenta_que_nao_existe_da_erro_e_nao_panica() {
        let p = ponte();
        assert!(p.usar("formatar_o_disco", json!({})).is_err());
        // Campo obrigatório faltando também é erro de gente, não pânico.
        assert!(p.usar("enviar_para", json!({ "no": "Redator" })).is_err());
    }

    // ---- o servidor MCP --------------------------------------------------

    #[test]
    fn o_handshake_do_mcp_e_o_que_a_cli_faz_de_verdade() {
        // A ordem e os formatos abaixo foram capturados de uma sonda contra a
        // CLI 2.1.251 — ver o cabeçalho de `mcp.rs`.
        let p = ponte();
        let token = p.banco.lock().unwrap().token_da_sessao(&p.a.id).unwrap();
        let chamar = |corpo: serde_json::Value| {
            crate::mcp::tratar(&p.orq, &p.banco, &token, &corpo.to_string())
        };

        // 1. `server/discover`, com id em TEXTO, antes do handshake.
        let r = chamar(json!({
            "jsonrpc": "2.0", "id": "server-discover-probe-1", "method": "server/discover",
        }));
        assert_eq!(r.codigo, 200);
        let v: serde_json::Value = serde_json::from_str(&r.corpo).unwrap();
        assert_eq!(v["id"], "server-discover-probe-1");

        // 2. `initialize` — devolvemos a versão que ele pediu.
        let r = chamar(json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": { "protocolVersion": "2025-11-25" },
        }));
        let v: serde_json::Value = serde_json::from_str(&r.corpo).unwrap();
        assert_eq!(v["result"]["protocolVersion"], "2025-11-25");
        assert_eq!(v["result"]["serverInfo"]["name"], ferramentas::SERVIDOR);

        // 3. `notifications/initialized`: sem id, e portanto sem resposta.
        let r = chamar(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        assert_eq!(r.codigo, 202);
        assert!(r.corpo.is_empty(), "respondemos a uma notificação: {}", r.corpo);

        // 4. `tools/list` traz o que ESTE nó alcança — não o catálogo inteiro.
        //    O filtro por papel mora aqui desde o M5; ver
        //    `o_tools_list_esconde_o_que_o_papel_nao_alcanca`.
        let r = chamar(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }));
        let v: serde_json::Value = serde_json::from_str(&r.corpo).unwrap();
        assert_eq!(
            v["result"]["tools"].as_array().unwrap().len(),
            papeis::ferramentas_do_papel(None).len()
        );

        // 5. Método que não existe vira erro de JSON-RPC, não 500.
        let r = chamar(json!({ "jsonrpc": "2.0", "id": 9, "method": "resources/list" }));
        let v: serde_json::Value = serde_json::from_str(&r.corpo).unwrap();
        assert_eq!(v["error"]["code"], -32601);
    }

    #[test]
    fn o_mcp_so_atende_quem_tem_o_token_da_sessao() {
        let p = ponte();
        let listar = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }).to_string();
        for token in ["", "   ", "token-inventado", &"a".repeat(64)] {
            let r = crate::mcp::tratar(&p.orq, &p.banco, token, &listar);
            assert_eq!(r.codigo, 403, "token {token:?} passou");
            // 403 sem explicação: nem a lista de ferramentas sai para quem não
            // se identificou.
            assert!(!r.corpo.contains("enviar_para"), "vazou o catálogo: {}", r.corpo);
        }
    }

    #[test]
    fn tools_call_aceita_o_nome_com_e_sem_prefixo() {
        let p = ponte();
        let token = p.banco.lock().unwrap().token_da_sessao(&p.a.id).unwrap();
        for nome in ["listar_nos", "mcp__mutirao__listar_nos"] {
            let r = crate::mcp::tratar(
                &p.orq,
                &p.banco,
                &token,
                &json!({
                    "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                    "params": { "name": nome, "arguments": {} },
                })
                .to_string(),
            );
            let v: serde_json::Value = serde_json::from_str(&r.corpo).unwrap();
            assert_eq!(v["result"]["isError"], false, "falhou com {nome}: {}", r.corpo);
            assert!(v["result"]["content"][0]["text"].as_str().unwrap().contains("Redator"));
        }
    }

    #[test]
    fn erro_de_ferramenta_chega_ao_modelo_em_vez_de_virar_erro_de_protocolo() {
        // A diferença importa: erro de JSON-RPC o cliente engole; resultado com
        // `isError` o modelo LÊ — e "esse nó não existe" é o que ele precisa ler
        // para corrigir o rumo sozinho.
        let p = ponte();
        let token = p.banco.lock().unwrap().token_da_sessao(&p.a.id).unwrap();
        let r = crate::mcp::tratar(
            &p.orq,
            &p.banco,
            &token,
            &json!({
                "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                "params": { "name": "enviar_para", "arguments": { "no": "Isolado", "mensagem": "oi" } },
            })
            .to_string(),
        );
        let v: serde_json::Value = serde_json::from_str(&r.corpo).unwrap();
        assert!(v["error"].is_null(), "virou erro de protocolo: {}", r.corpo);
        assert_eq!(v["result"]["isError"], true);
        assert!(v["result"]["content"][0]["text"].as_str().unwrap().contains("Isolado"));
    }

    #[test]
    fn o_conteudo_de_uma_leitura_vai_cru_e_o_resto_vai_como_json() {
        let p = ponte();
        let token = p.banco.lock().unwrap().token_da_sessao(&p.a.id).unwrap();
        p.usar("escrever_nota", json!({ "nota": "Briefing", "conteudo": "linha 1\nlinha 2" }))
            .unwrap();
        let r = crate::mcp::tratar(
            &p.orq,
            &p.banco,
            &token,
            &json!({
                "jsonrpc": "2.0", "id": 4, "method": "tools/call",
                "params": { "name": "ler_nota", "arguments": { "nota": "Briefing" } },
            })
            .to_string(),
        );
        let v: serde_json::Value = serde_json::from_str(&r.corpo).unwrap();
        // Cru: sem aspas escapadas gastando token à toa.
        assert_eq!(v["result"]["content"][0]["text"], "linha 1\nlinha 2");
    }

    // ---- o gate de escrita cobre o MCP também ----------------------------

    #[test]
    fn as_ferramentas_que_gravam_pedem_card_com_o_nome_completo() {
        // Se `escrever_nota` escapasse do card, o barramento seria uma porta
        // dos fundos para exatamente o que o card existe para impedir.
        for f in ferramentas::FERRAMENTAS_QUE_GRAVAM {
            let completo = ferramentas::nome_completo(f);
            assert!(crate::barramento::pede_licenca(&completo), "{completo} escapou do card");
            assert!(crate::barramento::matcher_do_hook(&[]).contains(&completo));
            // Gravar na pasta aceita "não perguntar de novo", como o `Write`.
            assert!(crate::barramento::aceita_regra(&completo));
        }
        // E as que só leem ou conversam continuam sem card — um card que vira
        // ruído é um card que o usuário aprova sem ler.
        for f in ["enviar_para", "listar_nos", "ler_nota", "concluir"] {
            assert!(!crate::barramento::pede_licenca(&ferramentas::nome_completo(f)), "{f}");
        }
    }

    #[test]
    fn os_nomes_mcp_do_card_batem_com_o_catalogo() {
        // `descrever_ferramenta` casa nomes literais porque `match` não aceita
        // expressão. Este teste é o elo que impede os dois lados de divergirem.
        let (r, d) = descrever_ferramenta(
            &ferramentas::nome_completo("escrever_nota"),
            &json!({ "nota": "Briefing", "conteudo": "uma\nduas" }),
        );
        assert_eq!(r, "Escrever na nota Briefing");
        assert!(d.contains("2 linhas"), "{d}");

        let (r, _) = descrever_ferramenta(
            &ferramentas::nome_completo("escrever_arquivo"),
            &json!({ "caminho": "sub/minuta.md", "conteudo": "x" }),
        );
        assert_eq!(r, "Gravar minuta.md");

        // E o card mostra o que vai ser gravado, senão é um card que se aprova
        // sem ler.
        let pedido = crate::barramento::PedidoDoHook::do_json(&json!({
            "tool_name": ferramentas::nome_completo("escrever_nota"),
            "tool_input": { "nota": "Briefing", "conteudo": "o texto que vai para o disco" },
            "tool_use_id": "toolu_1",
        }))
        .unwrap();
        assert_eq!(pedido.argumentos["conteudo"], "o texto que vai para o disco");
    }

    #[test]
    fn o_token_da_sessao_nao_aparece_na_linha_de_comando() {
        // A linha de comando de um processo é legível por qualquer outro
        // processo do mesmo usuário. Por isso hook e MCP vão em arquivo.
        let ctx = ContextoSessao {
            session_id: "ses_teste".into(),
            node_id: "no_teste".into(),
            pasta: std::env::temp_dir().to_string_lossy().to_string(),
            sessao_externa_id: None,
            token: "segredo-que-nao-pode-vazar".into(),
            url_barramento: Some("http://127.0.0.1:7777".into()),
            papel: None,
        };
        assert_eq!(ctx.url_de_aprovacao().unwrap(), "http://127.0.0.1:7777/aprovacao");
        assert_eq!(ctx.url_do_mcp().unwrap(), "http://127.0.0.1:7777/mcp");

        let a = AdaptadorClaude::novo("claude", ctx).unwrap();
        assert!(a.pode_escrever());
        let linha = format!("{:?}", a.comando());
        assert!(!linha.contains("segredo-que-nao-pode-vazar"), "o token vazou: {linha}");
        assert!(linha.contains("--mcp-config"), "sem ponte: {linha}");
        assert!(linha.contains("mcp__mutirao__enviar_para"), "a ponte não foi nomeada: {linha}");
    }

    /// O prompt não é argumento de linha de comando, e isso é conserto de
    /// Windows: instalada pelo npm, a CLI é um `claude.cmd`, e programa em lote
    /// não aceita argumento com quebra de linha — o Rust recusa a chamada antes
    /// de abrir o processo. Um prompt de duas linhas é o caso comum, não o
    /// raro. Some-se o teto de 32 mil caracteres da linha de comando do
    /// Windows, que um documento colado estoura.
    #[test]
    fn o_prompt_nao_entra_na_linha_de_comando() {
        let ctx = ContextoSessao {
            session_id: "ses_teste".into(),
            node_id: "no_teste".into(),
            pasta: std::env::temp_dir().to_string_lossy().to_string(),
            sessao_externa_id: None,
            token: "t".into(),
            url_barramento: None,
            papel: None,
        };
        let a = AdaptadorClaude::novo("claude", ctx).unwrap();
        let linha = format!("{:?}", a.comando());

        // `--print` continua lá, sozinho: é ele que põe a CLI em modo
        // headless. Sem prompt depois dele, ela lê o prompt do stdin —
        // medido na 2.1.252, inclusive com prompt de duas linhas.
        let partes: Vec<&str> = linha.split("\"--print\"").collect();
        assert_eq!(partes.len(), 2, "sem modo headless: {linha}");
        let depois = partes[1].trim_start();
        assert!(
            depois.starts_with("\"--"),
            "depois de --print tem de vir outra flag, não o prompt: {depois}"
        );
    }

    // ================================================================ M4 ===
    // Papéis e times. O que transforma quatro agentes iguais num time.

    use crate::papeis;
    use crate::partituras;

    /// Bancada do M4: um workspace com a biblioteca de papéis já semeada
    /// (`Banco::preparar` faz isso) e um Organizador pronto para montar time.
    struct Elenco {
        banco: Arc<Mutex<Banco>>,
        orq: Arc<Orquestrador>,
        chefe: Sessao,
        ws: String,
        avisos: Arc<Mutex<Vec<EventoNucleo>>>,
        _pasta: Pasta,
    }

    fn elenco() -> Elenco {
        let pasta = Pasta::nova();
        let banco = Banco::em_memoria().unwrap();
        let ws = banco.criar_workspace("Obra", pasta.0.to_str().unwrap()).unwrap();
        let organizador = banco.papel_por_nome("Organizador").unwrap().expect("papel embutido");
        let no = banco
            .criar_no_recrutado(
                &ws.id,
                TipoNo::Agente,
                "Chefe",
                0.0,
                0.0,
                Some(&organizador.id),
                None,
            )
            .unwrap();
        let banco = Arc::new(Mutex::new(banco));

        let avisos: Arc<Mutex<Vec<EventoNucleo>>> = Arc::new(Mutex::new(Vec::new()));
        let copia = avisos.clone();
        let sink: Sink = Arc::new(move |e| copia.lock().unwrap().push(e));

        let orq = Arc::new(Orquestrador::novo(
            banco.clone(),
            Arc::new(FabricaFalsa::com_roteiro(roteiro_simples())),
            sink,
        ));
        let chefe = orq.abrir_sessao(&no.id, Adaptador::Falso).unwrap();
        banco.lock().unwrap().mudar_estado_sessao(&chefe.id, EstadoSessao::Pensando).unwrap();

        Elenco { banco, orq, chefe, ws: ws.id, avisos, _pasta: pasta }
    }

    impl Elenco {
        fn usar(&self, nome: &str, args: serde_json::Value) -> Resultado<serde_json::Value> {
            ferramentas::executar(&self.orq, &self.banco, &self.chefe, nome, &args)
        }

        fn nos(&self) -> Vec<No> {
            self.banco.lock().unwrap().listar_nos(&self.ws).unwrap()
        }
    }

    // ---- a biblioteca ----------------------------------------------------

    #[test]
    fn a_biblioteca_de_papeis_vem_junto_e_semear_duas_vezes_nao_duplica() {
        let b = Banco::em_memoria().unwrap();
        let quantos = b.listar_papeis().unwrap().len();
        assert_eq!(quantos, papeis::embutidos().len(), "a biblioteca não subiu inteira");

        // `Banco::preparar` já semeou; semear de novo é o que acontece a cada
        // abertura do app.
        assert_eq!(papeis::semear(&b).unwrap(), 0, "semeou de novo");
        assert_eq!(b.listar_papeis().unwrap().len(), quantos);

        // Todos vêm marcados como embutidos, e todos dizem alguma coisa.
        for p in b.listar_papeis().unwrap() {
            assert!(p.embutido, "{} não veio marcado", p.nome);
            assert!(p.prompt.len() > 80, "prompt curto demais em {}", p.nome);
        }
    }

    #[test]
    fn embutido_nao_se_apaga_e_a_mensagem_diz_o_que_fazer() {
        let b = Banco::em_memoria().unwrap();
        let p = b.papel_por_nome("Redator").unwrap().unwrap();
        let e = b.remover_papel(&p.id).expect_err("embutido não devia sumir");
        assert!(e.to_string().contains("Duplique"), "{e}");

        // O que a pessoa cria, a pessoa apaga.
        let meu = b
            .criar_papel("Meu jeito", "Você faz do meu jeito.", &[], Autonomia::Padrao, None, false)
            .unwrap();
        assert!(b.remover_papel(&meu.id).is_ok());
    }

    #[test]
    fn apagar_papel_deixa_o_no_sem_papel_em_vez_de_levar_a_conversa_junto() {
        let b = Banco::em_memoria().unwrap();
        let ws = b.criar_workspace("Obra", "/tmp/obra-papel").unwrap();
        let p = b
            .criar_papel("Temporário", "Você é temporário.", &[], Autonomia::Padrao, None, false)
            .unwrap();
        let no = b
            .criar_no_recrutado(&ws.id, TipoNo::Agente, "A", 0.0, 0.0, Some(&p.id), None)
            .unwrap();
        let s = b.criar_sessao(&no.id, Adaptador::Falso).unwrap();
        b.gravar_mensagem(&s.id, PapelMensagem::Usuario, "oi", Uso::default()).unwrap();

        assert_eq!(b.quantos_usam_o_papel(&p.id).unwrap(), 1);
        b.remover_papel(&p.id).unwrap();

        // O nó continua, a conversa continua, o papel some.
        let depois = b.obter_no(&no.id).unwrap();
        assert_eq!(depois.role_id, None);
        assert_eq!(b.historico(&s.id, 10).unwrap().len(), 1);
    }

    #[test]
    fn a_escada_de_autonomia_nunca_pula_o_card() {
        // O `ARQUITETURA.md §8` em forma de teste: nenhum nível de autonomia
        // pode fazer uma gravação passar sem aprovação. O que a escada muda é
        // o que o papel ALCANÇA, não o que ele contorna.
        for nivel in [Autonomia::Cauteloso, Autonomia::Padrao, Autonomia::Solto] {
            let papel = Papel {
                id: "x".into(),
                nome: "N".into(),
                prompt: "p".into(),
                ferramentas: vec![],
                autonomia: nivel,
                modelo: None,
                embutido: false,
                criado_em: 0,
                mcp: vec![],
            };
            for f in papeis::ferramentas_do_papel(Some(&papel)) {
                if ferramentas::FERRAMENTAS_QUE_GRAVAM.contains(&f.as_str()) {
                    assert!(
                        crate::barramento::pede_licenca(&ferramentas::nome_completo(&f)),
                        "{f} escapou do card em {nivel:?}"
                    );
                }
            }
            // E o Bash, quando existe, nunca aceita "não perguntar de novo".
            if papeis::nativas(nivel).contains(&"Bash") {
                assert!(!crate::barramento::aceita_regra("Bash"));
            }
        }
    }

    #[test]
    fn cauteloso_nao_grava_e_solto_roda_comando() {
        let com = |a| Papel {
            id: "x".into(),
            nome: "N".into(),
            prompt: "p".into(),
            ferramentas: vec![],
            autonomia: a,
            modelo: None,
            embutido: false,
            criado_em: 0,
            mcp: vec![],
        };

        let cauteloso = papeis::ferramentas_do_papel(Some(&com(Autonomia::Cauteloso)));
        assert!(!cauteloso.contains(&"escrever_nota".to_string()));
        assert!(cauteloso.contains(&"enviar_para".to_string()), "conversar vale em todo nível");
        assert!(!papeis::nativas(Autonomia::Cauteloso).contains(&"Write"));

        let padrao = papeis::ferramentas_do_papel(Some(&com(Autonomia::Padrao)));
        assert!(padrao.contains(&"escrever_nota".to_string()));
        assert!(!papeis::nativas(Autonomia::Padrao).contains(&"Bash"));

        assert!(papeis::nativas(Autonomia::Solto).contains(&"Bash"));
    }

    #[test]
    fn o_papel_estreita_a_lista_da_autonomia_mas_nunca_a_alarga() {
        // Se pudesse alargar, `cauteloso` deixaria de querer dizer alguma
        // coisa — e um nome que não quer dizer nada dá confiança sem base.
        let esperto = Papel {
            id: "x".into(),
            nome: "N".into(),
            prompt: "p".into(),
            // Pede escrita e time, sendo cauteloso.
            ferramentas: vec![
                "ler_nota".into(),
                "escrever_nota".into(),
                "escrever_arquivo".into(),
            ],
            autonomia: Autonomia::Cauteloso,
            modelo: None,
            embutido: false,
            criado_em: 0,
            mcp: vec![],
        };
        let tem = papeis::ferramentas_do_papel(Some(&esperto));
        assert_eq!(tem, vec!["ler_nota".to_string()], "a autonomia foi contornada: {tem:?}");
    }

    #[test]
    fn so_quem_tem_a_ferramenta_monta_time() {
        let b = Banco::em_memoria().unwrap();
        let organizador = b.papel_por_nome("Organizador").unwrap().unwrap();
        let redator = b.papel_por_nome("Redator").unwrap().unwrap();
        assert!(papeis::pode_recrutar(Some(&organizador)));
        assert!(!papeis::pode_recrutar(Some(&redator)), "o Redator não monta time");
        // E um nó sem papel também não: recrutar é função, não padrão de fábrica.
        assert!(!papeis::pode_recrutar(None));
        assert!(!papeis::ferramentas_do_papel(None).contains(&"recrutar".to_string()));
    }

    // ---- o escopo do papel vale no servidor, não só na CLI ---------------

    #[test]
    fn o_papel_barra_a_ferramenta_mesmo_chamada_direto_pelo_mcp() {
        // O `--tools` da CLI esconde o resto, mas esconder não é impedir: um
        // `tools/call` chega por HTTP e quem monta o corpo é o processo do
        // agente. Este teste bate na porta por fora.
        let e = elenco();
        let papel_sem_escrita = {
            let b = e.banco.lock().unwrap();
            b.papel_por_nome("Pesquisador").unwrap().unwrap()
        };
        e.banco
            .lock()
            .unwrap()
            .definir_papel_do_no(&e.chefe.node_id, Some(&papel_sem_escrita.id))
            .unwrap();

        let token = e.banco.lock().unwrap().token_da_sessao(&e.chefe.id).unwrap();
        let r = crate::mcp::tratar(
            &e.orq,
            &e.banco,
            &token,
            &serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": {
                    "name": "escrever_arquivo",
                    "arguments": { "caminho": "nao-deveria.txt", "conteudo": "oi" },
                },
            })
            .to_string(),
        );
        let v: serde_json::Value = serde_json::from_str(&r.corpo).unwrap();
        assert_eq!(v["result"]["isError"], true, "o Pesquisador gravou: {}", r.corpo);
        // E o arquivo não existe. Recusar e gravar assim mesmo seria pior que
        // não recusar.
        assert!(!e._pasta.0.join("nao-deveria.txt").exists(), "gravou apesar do erro");
    }

    // ---- recrutar --------------------------------------------------------

    #[test]
    fn recrutar_poe_o_agente_no_canvas_ja_ligado_a_quem_recrutou() {
        let e = elenco();
        let nome = e
            .usar("recrutar", serde_json::json!({ "papel": "Redator", "nome": "Redator" }))
            .unwrap();
        assert_eq!(nome["no"], "Redator");

        let nos = e.nos();
        let novo = nos.iter().find(|n| n.nome == "Redator").expect("o recrutado não apareceu");
        assert_eq!(novo.tipo, TipoNo::Agente);
        assert_eq!(novo.recrutado_por.as_deref(), Some(e.chefe.node_id.as_str()));

        // Com papel, senão ele é um agente qualquer com nome bonito.
        let papel = e.banco.lock().unwrap().obter_papel(novo.role_id.as_ref().unwrap()).unwrap();
        assert_eq!(papel.nome, "Redator");

        // E ligado: sem cabo o recrutado é uma ilha, e o §4 diz que sem cabo o
        // nó não existe para ninguém.
        let vizinhos =
            e.banco.lock().unwrap().vizinhos(&e.chefe.node_id, TipoCabo::FalaCom).unwrap();
        assert!(vizinhos.contains(&novo.id), "recrutou e não ligou");

        // O canvas soube, senão o agente trabalha onde ninguém vê.
        let avisou = e.avisos.lock().unwrap().iter().any(|a| {
            matches!(a, EventoNucleo::CanvasMudou { workspace_id, .. } if workspace_id == &e.ws)
        });
        assert!(avisou, "o canvas não foi avisado");
    }

    #[test]
    fn recrutar_com_papel_que_nao_existe_diz_quais_existem() {
        let e = elenco();
        let err = e
            .usar("recrutar", serde_json::json!({ "papel": "Feiticeiro", "nome": "X" }))
            .expect_err("papel inventado não devia passar");
        assert!(err.to_string().contains("Pesquisador"), "não listou os que existem: {err}");
    }

    #[test]
    fn nome_repetido_e_recusado_na_hora_de_recrutar() {
        // Dois "Redator" quebram o `enviar_para`, que resolve vizinho pelo
        // nome. Recusar aqui é melhor que descobrir quando o time trabalha.
        let e = elenco();
        e.usar("recrutar", serde_json::json!({ "papel": "Redator", "nome": "Ana" })).unwrap();
        let err = e
            .usar("recrutar", serde_json::json!({ "papel": "Revisor", "nome": "Ana" }))
            .expect_err("nome repetido não devia passar");
        assert!(err.to_string().contains("já existe"), "{err}");
    }

    #[test]
    fn o_teto_de_recrutas_por_cadeia_para_o_time_de_crescer() {
        // O quinto limite. Os quatro do M3 incidem sobre a conversa; nenhum
        // deles impede um Maestro de recrutar cem num turno só.
        let e = elenco();
        // Um turno de verdade, para existir cadeia sobre a qual o teto incide.
        let travada = Arc::new(FabricaTravada::nova());
        let orq = Arc::new(Orquestrador::novo(e.banco.clone(), travada, sink_mudo()));
        let chefe = orq.abrir_sessao(&e.chefe.node_id, Adaptador::Falso).unwrap();
        e.banco.lock().unwrap().forcar_estado_sessao(&chefe.id, EstadoSessao::Ocioso).unwrap();
        orq.enviar(&chefe.id, "monte o time").unwrap();
        assert!(esperar(|| e.banco.lock().unwrap().obter_sessao(&chefe.id).unwrap().estado
            == EstadoSessao::Pensando));

        for i in 0..MAX_RECRUTAS_POR_CADEIA {
            ferramentas::executar(
                &orq,
                &e.banco,
                &chefe,
                "recrutar",
                &serde_json::json!({ "papel": "Redator", "nome": format!("R{i}") }),
            )
            .unwrap_or_else(|erro| panic!("recruta {i} devia passar: {erro}"));
        }
        let err = ferramentas::executar(
            &orq,
            &e.banco,
            &chefe,
            "recrutar",
            &serde_json::json!({ "papel": "Redator", "nome": "Demais" }),
        )
        .expect_err("passou do teto");
        assert!(err.to_string().contains("limite"), "{err}");
        assert!(e.nos().iter().all(|n| n.nome != "Demais"), "criou apesar do erro");
    }

    #[test]
    fn o_teto_por_workspace_pega_o_que_o_teto_por_cadeia_nao_pega() {
        // Três hoje, três amanhã, três depois: cada turno abre uma cadeia
        // nova, e o teto por cadeia zera junto.
        let e = elenco();
        let banco = e.banco.lock().unwrap();
        for i in 0..MAX_AGENTES_POR_WORKSPACE {
            let _ = banco.criar_no(&e.ws, TipoNo::Agente, &format!("Enchendo {i}"), 0.0, 0.0);
        }
        drop(banco);
        let err = e
            .usar("recrutar", serde_json::json!({ "papel": "Redator", "nome": "Gota d'água" }))
            .expect_err("passou do teto do workspace");
        assert!(err.to_string().contains("dispensar"), "não diz o que fazer: {err}");
    }

    // ---- dispensar -------------------------------------------------------

    #[test]
    fn dispensar_encerra_a_sessao_e_nao_apaga_o_no() {
        // Apagar levaria a conversa junto por CASCADE. Destruir trabalho por
        // conta de um agente é o oposto do §8 — quem apaga nó é a pessoa.
        let e = elenco();
        e.usar("recrutar", serde_json::json!({ "papel": "Redator", "nome": "Bia" })).unwrap();
        let bia = e.nos().into_iter().find(|n| n.nome == "Bia").unwrap();
        let sessao = e.orq.abrir_sessao(&bia.id, Adaptador::Falso).unwrap();
        e.banco
            .lock()
            .unwrap()
            .gravar_mensagem(&sessao.id, PapelMensagem::Agente, "meu trabalho", Uso::default())
            .unwrap();

        e.usar("dispensar", serde_json::json!({ "no": "Bia" })).unwrap();

        assert!(e.nos().iter().any(|n| n.nome == "Bia"), "o nó foi apagado");
        let conversa = e.banco.lock().unwrap().historico(&sessao.id, 20).unwrap();
        assert!(
            conversa.iter().any(|m| m.conteudo == "meu trabalho"),
            "a conversa foi levada junto"
        );
        assert!(
            conversa.iter().any(|m| m.conteudo.contains("Dispensado")),
            "não registrou o que houve: {conversa:?}"
        );
    }

    #[test]
    fn so_quem_recrutou_dispensa() {
        let e = elenco();
        // Um nó que a PESSOA criou, ligado ao chefe.
        let alheio = {
            let b = e.banco.lock().unwrap();
            let n = b.criar_no(&e.ws, TipoNo::Agente, "Da casa", 500.0, 0.0).unwrap();
            b.criar_cabo(&e.ws, &e.chefe.node_id, &n.id, TipoCabo::FalaCom).unwrap();
            n
        };
        let err = e
            .usar("dispensar", serde_json::json!({ "no": "Da casa" }))
            .expect_err("dispensou quem não recrutou");
        assert!(err.to_string().contains("não recrutou"), "{err}");
        assert!(e.nos().iter().any(|n| n.id == alheio.id));
    }

    // ---- partituras ------------------------------------------------------

    #[test]
    fn salvar_e_reabrir_devolve_o_mesmo_time() {
        // O critério do M4 em forma de teste: "amanhã eu reabro o mesmo time
        // como estava".
        let e = elenco();
        e.usar("recrutar", serde_json::json!({ "papel": "Pesquisador", "nome": "Pesq" })).unwrap();
        e.usar("recrutar", serde_json::json!({ "papel": "Redator", "nome": "Red" })).unwrap();
        e.usar("recrutar", serde_json::json!({ "papel": "Revisor", "nome": "Rev" })).unwrap();

        let (partitura, antes) = {
            let b = e.banco.lock().unwrap();
            let snap = partituras::fotografar(&b, &e.ws).unwrap();
            (b.salvar_partitura(&e.ws, "Time do contrato", &snap).unwrap(), b.listar_nos(&e.ws).unwrap())
        };
        assert_eq!(partitura.snapshot.nos.len(), 4, "o chefe e os três recrutados");
        assert!(!partitura.snapshot.cabos.is_empty(), "os cabos não foram junto");

        // Abre num workspace limpo — que é o teste de verdade de "reabrir",
        // porque prova que a partitura não depende dos ids de onde nasceu.
        let outro = Pasta::nova();
        let novos = {
            let b = e.banco.lock().unwrap();
            let ws2 = b.criar_workspace("Outra obra", outro.0.to_str().unwrap()).unwrap();
            partituras::montar(&b, &ws2.id, &partitura).unwrap()
        };

        assert_eq!(novos.len(), antes.len());
        let nomes: Vec<&str> = novos.iter().map(|n| n.nome.as_str()).collect();
        for esperado in ["Chefe", "Pesq", "Red", "Rev"] {
            assert!(nomes.contains(&esperado), "faltou {esperado} em {nomes:?}");
        }
        // Com os papéis certos, senão é um time de estranhos com os nomes certos.
        let b = e.banco.lock().unwrap();
        let red = novos.iter().find(|n| n.nome == "Red").unwrap();
        let papel = b.obter_papel(red.role_id.as_ref().unwrap()).unwrap();
        assert_eq!(papel.nome, "Redator");
        // E ligados entre si.
        assert!(!b.listar_cabos(&red.workspace_id).unwrap().is_empty());
        // Ids novos: reabrir cria, não restaura por cima.
        assert!(novos.iter().all(|n| antes.iter().all(|a| a.id != n.id)));
    }

    #[test]
    fn partitura_nao_leva_conversa_nem_custo() {
        // Ela é a planta do time, não um backup dele.
        let e = elenco();
        e.banco
            .lock()
            .unwrap()
            .gravar_mensagem(&e.chefe.id, PapelMensagem::Agente, "segredo", Uso::default())
            .unwrap();
        let snap = partituras::fotografar(&e.banco.lock().unwrap(), &e.ws).unwrap();
        let json = serde_json::to_string(&snap).unwrap();
        assert!(!json.contains("segredo"), "a conversa foi junto: {json}");
        assert!(!json.contains("custo"), "o custo foi junto");
        // E nada de id: o id pertence ao canvas onde o nó vive.
        assert!(!json.contains(&e.chefe.node_id), "o id do nó foi junto");
    }

    #[test]
    fn reabrir_no_mesmo_workspace_nao_empilha_nem_repete_nome() {
        let e = elenco();
        e.usar("recrutar", serde_json::json!({ "papel": "Redator", "nome": "Red" })).unwrap();

        let partitura = {
            let b = e.banco.lock().unwrap();
            let snap = partituras::fotografar(&b, &e.ws).unwrap();
            b.salvar_partitura(&e.ws, "Time", &snap).unwrap()
        };
        let novos = {
            let b = e.banco.lock().unwrap();
            partituras::montar(&b, &e.ws, &partitura).unwrap()
        };

        // Nomes desambiguados: dois "Red" quebrariam o `enviar_para`.
        let nomes: Vec<&str> = novos.iter().map(|n| n.nome.as_str()).collect();
        assert!(nomes.contains(&"Red 2"), "não desambiguou: {nomes:?}");
        // E deslocados para a direita, em vez de exatamente em cima.
        let antigo = e.nos().into_iter().find(|n| n.nome == "Red").unwrap();
        let novo = novos.iter().find(|n| n.nome == "Red 2").unwrap();
        assert!(novo.x > antigo.x, "caiu em cima do que já estava lá");
    }

    #[test]
    fn salvar_com_o_mesmo_nome_atualiza_em_vez_de_dar_erro() {
        // Quem repete o nome está atualizando o time, não descobrindo um
        // índice único.
        let e = elenco();
        let banco = e.banco.lock().unwrap();
        let um = partituras::fotografar(&banco, &e.ws).unwrap();
        let p1 = banco.salvar_partitura(&e.ws, "Time", &um).unwrap();
        banco.criar_no(&e.ws, TipoNo::Agente, "Mais um", 0.0, 0.0).unwrap();
        let dois = partituras::fotografar(&banco, &e.ws).unwrap();
        let p2 = banco.salvar_partitura(&e.ws, "Time", &dois).unwrap();

        assert_eq!(p1.id, p2.id, "criou uma segunda linha em vez de atualizar");
        assert_eq!(p2.snapshot.nos.len(), um.nos.len() + 1);
        assert_eq!(banco.listar_partituras(&e.ws).unwrap().len(), 1);
    }

    #[test]
    fn papel_que_nao_existe_na_maquina_nao_derruba_o_time_inteiro() {
        // Recusar a partitura por causa de um papel apagado seria perder o
        // time todo por um detalhe que se conserta em dois cliques.
        let e = elenco();
        let partitura = Partitura {
            id: "p".into(),
            workspace_id: e.ws.clone(),
            nome: "Importado".into(),
            snapshot: Snapshot {
                nos: vec![NoSalvo {
                    tipo: TipoNo::Agente,
                    nome: "Estranho".into(),
                    x: 0.0,
                    y: 0.0,
                    w: 320.0,
                    h: 240.0,
                    config: serde_json::json!({}),
                    papel: Some("Papel De Outra Máquina".into()),
                }],
                cabos: vec![],
            },
            criado_em: 0,
        };
        let novos = {
            let b = e.banco.lock().unwrap();
            partituras::montar(&b, &e.ws, &partitura).unwrap()
        };
        assert_eq!(novos.len(), 1);
        assert_eq!(novos[0].role_id, None, "inventou um papel");
    }

    #[test]
    fn no_sem_papel_continua_com_o_que_tinha_antes_do_m4() {
        // Todo nó criado até o M4 está sem papel. Estreitar o que eles
        // alcançam seria mudar o workspace de alguém sem ele pedir — e
        // "atualizei o app e meu agente parou de rodar comando" é o tipo de
        // regressão que ninguém liga ao marco que a causou.
        let nativas = papeis::nativas_do_papel(None);
        for f in ["Read", "Glob", "Grep", "Write", "Edit", "NotebookEdit", "Bash"] {
            assert!(nativas.contains(&f), "nó sem papel perdeu {f}");
        }
        // E continua alcançando as do §6 — menos montar time, que é função de
        // papel e não padrão de fábrica.
        let do_seis = papeis::ferramentas_do_papel(None);
        assert!(do_seis.contains(&"escrever_nota".to_string()));
        assert!(do_seis.contains(&"enviar_para".to_string()));
        assert!(!do_seis.contains(&"recrutar".to_string()));
    }

    // ================================================================ M5 ===
    // Rascunhos. Duas versões do mesmo trabalho ao mesmo tempo, e publicar uma
    // delas sem o usuário entender de Git.

    use crate::ensaios;

    /// Bancada do M5: um workspace com pasta de verdade e histórico oculto.
    ///
    /// As duas pastas ficam fora uma da outra de propósito — é o desenho que
    /// o `git.rs` documenta, e testar com o repositório dentro esconderia
    /// justamente o que ele quer provar.
    struct Obra {
        banco: Arc<Mutex<Banco>>,
        orq: Arc<Orquestrador>,
        ws: String,
        pasta: Pasta,
        _repo: Pasta,
    }

    fn obra() -> Option<Obra> {
        if !crate::git::existe() {
            // Sem git na máquina o recurso não existe, e o teste diz isso em
            // vez de falhar: é o mesmo desfecho que o app dá ao usuário.
            eprintln!("git não instalado; pulando o teste de rascunhos");
            return None;
        }
        let pasta = Pasta::nova();
        let repo = Pasta::nova();
        std::fs::write(pasta.0.join("contrato.txt"), "prazo de 18 meses\n").unwrap();

        let banco = Banco::em_memoria().unwrap();
        let ws = banco.criar_workspace("Obra", pasta.0.to_str().unwrap()).unwrap();
        let caminho_repo = repo.0.join("historico");
        banco.definir_repo(&ws.id, caminho_repo.to_str().unwrap()).unwrap();
        let banco = Arc::new(Mutex::new(banco));

        let orq = Arc::new(Orquestrador::novo(
            banco.clone(),
            Arc::new(FabricaFalsa::com_roteiro(roteiro_simples())),
            sink_mudo(),
        ));
        assert!(ensaios::preparar(&banco.lock().unwrap(), &ws.id).unwrap());

        Some(Obra { banco, orq, ws: ws.id, pasta, _repo: repo })
    }

    impl Obra {
        fn banco(&self) -> std::sync::MutexGuard<'_, Banco> {
            self.banco.lock().unwrap()
        }
    }

    macro_rules! obra_ou_pula {
        () => {
            match obra() {
                Some(o) => o,
                None => return,
            }
        };
    }

    #[test]
    fn a_pasta_do_usuario_fica_literalmente_limpa() {
        // A `Decisão 3` promete "Git existe, mas o usuário nunca fica
        // sabendo". Isto é a promessa medida: nenhum `.git`, nenhum
        // `.mutirao`, nada para o Explorer mostrar.
        let o = obra_ou_pula!();
        let dentro: Vec<String> = std::fs::read_dir(&o.pasta.0)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(dentro, vec!["contrato.txt".to_string()], "sobrou coisa de Git: {dentro:?}");
    }

    #[test]
    fn dois_rascunhos_do_mesmo_trabalho_rodam_ao_mesmo_tempo() {
        // A primeira metade do critério do M5.
        let o = obra_ou_pula!();
        let a = ensaios::criar(&o.banco(), &o.ws, "Com a cláusula nova").unwrap();
        let b = ensaios::criar(&o.banco(), &o.ws, "Sem a cláusula").unwrap();

        // Cada um tem a sua cópia da pasta, e as duas começam iguais.
        for e in [&a, &b] {
            let copia = std::path::Path::new(&e.caminho_worktree).join("contrato.txt");
            assert!(copia.exists(), "o rascunho \"{}\" nasceu vazio", e.nome);
            assert_eq!(std::fs::read_to_string(copia).unwrap(), "prazo de 18 meses\n");
        }

        // Trabalham em paralelo sem se ver.
        std::fs::write(
            std::path::Path::new(&a.caminho_worktree).join("contrato.txt"),
            "prazo de 24 meses\n",
        )
        .unwrap();
        std::fs::write(
            std::path::Path::new(&b.caminho_worktree).join("contrato.txt"),
            "prazo de 12 meses\n",
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(std::path::Path::new(&b.caminho_worktree).join("contrato.txt"))
                .unwrap(),
            "prazo de 12 meses\n",
            "um rascunho enxergou o outro"
        );
        // E a pasta de verdade não mudou nada.
        assert_eq!(
            std::fs::read_to_string(o.pasta.0.join("contrato.txt")).unwrap(),
            "prazo de 18 meses\n",
            "o trabalho de um rascunho vazou para a pasta de verdade"
        );
    }

    #[test]
    fn a_previa_nao_toca_em_nada_e_diz_o_que_muda() {
        let o = obra_ou_pula!();
        let e = ensaios::criar(&o.banco(), &o.ws, "Rascunho").unwrap();
        let worktree = std::path::Path::new(&e.caminho_worktree);
        std::fs::write(worktree.join("contrato.txt"), "prazo de 24 meses\n").unwrap();
        // Um arquivo NOVO, que é o que um agente faz o tempo todo.
        std::fs::write(worktree.join("parecer.txt"), "parecer novo\n").unwrap();

        let previa = ensaios::prever(&o.banco(), &e.id).unwrap();
        let por_caminho: Vec<(&str, TipoMudanca)> =
            previa.alteracoes.iter().map(|m| (m.caminho.as_str(), m.como)).collect();
        assert!(
            por_caminho.contains(&("contrato.txt", TipoMudanca::Alterado)),
            "{por_caminho:?}"
        );
        // O arquivo criado pelo agente PRECISA aparecer: `git add -u` não o
        // pegaria, e ele sumiria na publicação sem ninguém notar.
        assert!(por_caminho.contains(&("parecer.txt", TipoMudanca::Criado)), "{por_caminho:?}");
        assert!(previa.conflitos.is_empty());

        // E nada aconteceu na pasta de verdade — prévia é prévia.
        assert_eq!(
            std::fs::read_to_string(o.pasta.0.join("contrato.txt")).unwrap(),
            "prazo de 18 meses\n"
        );
        assert!(!o.pasta.0.join("parecer.txt").exists(), "a prévia publicou");
    }

    #[test]
    fn publicar_leva_o_rascunho_para_a_pasta_de_verdade() {
        // A segunda metade do critério: "eu publico um deles sem entender de
        // Git".
        let o = obra_ou_pula!();
        let e = ensaios::criar(&o.banco(), &o.ws, "Rascunho").unwrap();
        let worktree = std::path::Path::new(&e.caminho_worktree);
        std::fs::write(worktree.join("contrato.txt"), "prazo de 24 meses\n").unwrap();
        std::fs::write(worktree.join("parecer.txt"), "parecer novo\n").unwrap();

        let feito = ensaios::publicar(&o.banco(), &o.orq, &e.id, &[]).unwrap();
        assert_eq!(feito.alteracoes.len(), 2);

        assert_eq!(
            std::fs::read_to_string(o.pasta.0.join("contrato.txt")).unwrap(),
            "prazo de 24 meses\n"
        );
        assert!(o.pasta.0.join("parecer.txt").exists(), "o arquivo novo não chegou");
        assert_eq!(o.banco().obter_ensaio(&e.id).unwrap().estado, EstadoEnsaio::Publicado);
        // A pasta continua limpa depois de publicar.
        assert!(!o.pasta.0.join(".git").exists());
    }

    #[test]
    fn o_trabalho_feito_a_mao_nao_vira_lixo_de_merge() {
        // Medido no probe: mesclar numa pasta com alteração não gravada NÃO é
        // recusado pelo git — ele mescla e deixa marcador de conflito dentro
        // do arquivo do usuário. Por isso `publicar` grava os dois lados antes.
        let o = obra_ou_pula!();
        let e = ensaios::criar(&o.banco(), &o.ws, "Rascunho").unwrap();
        std::fs::write(
            std::path::Path::new(&e.caminho_worktree).join("parecer.txt"),
            "do rascunho\n",
        )
        .unwrap();

        // O usuário mexeu na pasta enquanto o rascunho rodava.
        std::fs::write(o.pasta.0.join("anotacoes.txt"), "minha anotação\n").unwrap();

        ensaios::publicar(&o.banco(), &o.orq, &e.id, &[]).unwrap();

        let minha = std::fs::read_to_string(o.pasta.0.join("anotacoes.txt")).unwrap();
        assert_eq!(minha, "minha anotação\n", "o trabalho à mão foi mexido");
        assert!(!minha.contains("<<<<"), "marcador de merge no arquivo do usuário");
        assert!(o.pasta.0.join("parecer.txt").exists());
    }

    #[test]
    fn conflito_sem_escolha_nao_publica_nada() {
        // Publicar pela metade é pior que não publicar.
        let o = obra_ou_pula!();
        let e = ensaios::criar(&o.banco(), &o.ws, "Rascunho").unwrap();
        std::fs::write(
            std::path::Path::new(&e.caminho_worktree).join("contrato.txt"),
            "versão do rascunho\n",
        )
        .unwrap();
        std::fs::write(o.pasta.0.join("contrato.txt"), "versão da pasta\n").unwrap();

        let previa = ensaios::prever(&o.banco(), &e.id).unwrap();
        assert!(previa.conflitos.is_empty(), "a prévia vê conflito antes do commit da pasta?");

        let erro = ensaios::publicar(&o.banco(), &o.orq, &e.id, &[])
            .expect_err("devia recusar sem escolha");
        assert!(erro.to_string().contains("ninguém escolheu"), "{erro}");
        // A pasta ficou como estava, sem marcador nenhum.
        let agora = std::fs::read_to_string(o.pasta.0.join("contrato.txt")).unwrap();
        assert_eq!(agora, "versão da pasta\n", "publicou pela metade: {agora}");
        assert_eq!(o.banco().obter_ensaio(&e.id).unwrap().estado, EstadoEnsaio::Aberto);
    }

    #[test]
    fn escolher_um_lado_resolve_o_conflito() {
        let o = obra_ou_pula!();
        let e = ensaios::criar(&o.banco(), &o.ws, "Rascunho").unwrap();
        std::fs::write(
            std::path::Path::new(&e.caminho_worktree).join("contrato.txt"),
            "versão do rascunho\n",
        )
        .unwrap();
        std::fs::write(o.pasta.0.join("contrato.txt"), "versão da pasta\n").unwrap();

        ensaios::publicar(
            &o.banco(),
            &o.orq,
            &e.id,
            &[("contrato.txt".to_string(), LadoDoConflito::Rascunho)],
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(o.pasta.0.join("contrato.txt")).unwrap(),
            "versão do rascunho\n"
        );
    }

    #[test]
    fn descartar_joga_o_rascunho_fora_e_nao_toca_na_pasta() {
        let o = obra_ou_pula!();
        let e = ensaios::criar(&o.banco(), &o.ws, "Rascunho ruim").unwrap();
        std::fs::write(
            std::path::Path::new(&e.caminho_worktree).join("besteira.txt"),
            "não presta\n",
        )
        .unwrap();
        ensaios::trocar(&o.banco(), &o.orq, &o.ws, Some(&e.id)).unwrap();

        ensaios::descartar(&o.banco(), &o.orq, &e.id).unwrap();

        assert_eq!(o.banco().obter_ensaio(&e.id).unwrap().estado, EstadoEnsaio::Descartado);
        assert!(!std::path::Path::new(&e.caminho_worktree).exists(), "o worktree ficou no disco");
        assert!(!o.pasta.0.join("besteira.txt").exists());
        // E o foco voltou para a pasta de verdade, senão o trabalho seguinte
        // aconteceria num caminho que não existe mais.
        assert_eq!(o.banco().pasta_de_trabalho(&o.ws).unwrap(), o.pasta.0.to_string_lossy());
    }

    // ---- o perigo que o M4 anotou --------------------------------------

    #[test]
    fn a_pasta_de_trabalho_e_um_lugar_so() {
        // O risco anotado no fim do M4: se dois lugares respondem "onde
        // escrevo", uma sessão viva grava no worktree errado COM APROVAÇÃO
        // LEGÍTIMA — o card diz a verdade sobre o conteúdo e mente sobre o
        // destino. Este teste percorre os quatro caminhos que resolvem pasta.
        let o = obra_ou_pula!();
        let no = o.banco().criar_no(&o.ws, TipoNo::Agente, "A", 0.0, 0.0).unwrap();
        let sessao = o.orq.abrir_sessao(&no.id, Adaptador::Falso).unwrap();
        let e = ensaios::criar(&o.banco(), &o.ws, "Rascunho").unwrap();

        // Antes de trocar: tudo aponta para a pasta de verdade.
        assert_eq!(o.banco().pasta_de_trabalho(&o.ws).unwrap(), o.pasta.0.to_string_lossy());

        ensaios::trocar(&o.banco(), &o.orq, &o.ws, Some(&e.id)).unwrap();

        // Depois: tudo aponta para o rascunho. O contexto do adaptador é o
        // caminho que mais importa — é ele que vira `current_dir` do processo.
        assert_eq!(o.banco().pasta_de_trabalho(&o.ws).unwrap(), e.caminho_worktree);
        let ctx = o.orq.contexto_de_teste(&sessao).unwrap();
        assert_eq!(ctx.pasta, e.caminho_worktree, "o adaptador ficou na pasta antiga");

        // E as ferramentas do §6 escrevem no rascunho, não na pasta.
        o.banco().mudar_estado_sessao(&sessao.id, EstadoSessao::Pensando).unwrap();
        ferramentas::executar(
            &o.orq,
            &o.banco,
            &sessao,
            "escrever_arquivo",
            &serde_json::json!({ "caminho": "do-agente.txt", "conteudo": "oi" }),
        )
        .unwrap();
        assert!(
            std::path::Path::new(&e.caminho_worktree).join("do-agente.txt").exists(),
            "a ferramenta escreveu fora do rascunho"
        );
        assert!(
            !o.pasta.0.join("do-agente.txt").exists(),
            "a ferramenta escreveu na pasta de verdade estando num rascunho"
        );
    }

    #[test]
    fn trocar_de_rascunho_derruba_os_adaptadores_vivos() {
        // Um processo já aberto guarda a pasta antiga no diretório de trabalho
        // dele, e ninguém avisa o processo de nada. A única saída é derrubá-lo.
        let o = obra_ou_pula!();
        let no = o.banco().criar_no(&o.ws, TipoNo::Agente, "A", 0.0, 0.0).unwrap();
        let sessao = o.orq.abrir_sessao(&no.id, Adaptador::Falso).unwrap();
        o.orq.enviar(&sessao.id, "oi").unwrap();
        assert!(esperar(|| o.banco().obter_sessao(&sessao.id).unwrap().estado
            == EstadoSessao::Ocioso));
        assert_eq!(o.orq.quantos_vivos(), 1, "o adaptador não ficou em cache");

        let e = ensaios::criar(&o.banco(), &o.ws, "Rascunho").unwrap();
        ensaios::trocar(&o.banco(), &o.orq, &o.ws, Some(&e.id)).unwrap();

        assert_eq!(o.orq.quantos_vivos(), 0, "sobrou adaptador apontando para a pasta antiga");
        // A conversa fica: o usuário perde o processo, não o trabalho.
        assert!(!o.banco().historico(&sessao.id, 10).unwrap().is_empty());
    }

    #[test]
    fn sem_historico_o_app_continua_servindo_e_diz_por_que() {
        // Workspace do M0 ao M4, ou máquina sem git: sem rascunho, mas o resto
        // funciona. Perder o recurso é aceitável; fingir que ele funciona não.
        let b = Banco::em_memoria().unwrap();
        let ws = b.criar_workspace("Sem git", "/tmp/sem-git").unwrap();
        assert_eq!(b.obter_workspace(&ws.id).unwrap().repo, None);
        // A pasta de trabalho continua respondendo — é o que faz o resto do
        // app não depender disto.
        assert_eq!(b.pasta_de_trabalho(&ws.id).unwrap(), "/tmp/sem-git");

        let erro = ensaios::criar(&b, &ws.id, "Rascunho").expect_err("sem repo não há rascunho");
        assert!(erro.to_string().contains("Git não está instalado"), "{erro}");
    }

    #[test]
    fn o_tools_list_esconde_o_que_o_papel_nao_alcanca() {
        // Correção medida no M5: `--tools` com `--restricted` gateia as
        // ferramentas NATIVAS, não as de MCP. Rodando com `--tools "Read"` e o
        // nosso servidor no `--mcp-config`, o `system/init` listou `Read` mais
        // todas as nossas. Quem tem de esconder somos nós, aqui.
        let p = ponte();
        let token = p.banco.lock().unwrap().token_da_sessao(&p.a.id).unwrap();
        let listar = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })
            .to_string();
        let nomes = |corpo: &str| -> Vec<String> {
            let v: serde_json::Value = serde_json::from_str(corpo).unwrap();
            v["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["name"].as_str().unwrap().to_string())
                .collect()
        };

        // Sem papel: tudo, menos montar time.
        let r = crate::mcp::tratar(&p.orq, &p.banco, &token, &listar);
        let sem_papel = nomes(&r.corpo);
        assert!(sem_papel.contains(&"escrever_arquivo".to_string()));
        assert!(!sem_papel.contains(&"recrutar".to_string()));

        // Com um papel cauteloso: sem escrita nenhuma.
        let pesquisador = {
            let b = p.banco.lock().unwrap();
            b.papel_por_nome("Pesquisador").unwrap().unwrap()
        };
        p.banco
            .lock()
            .unwrap()
            .definir_papel_do_no(&p.a.node_id, Some(&pesquisador.id))
            .unwrap();
        let r = crate::mcp::tratar(&p.orq, &p.banco, &token, &listar);
        let com_papel = nomes(&r.corpo);
        assert!(!com_papel.contains(&"escrever_arquivo".to_string()), "{com_papel:?}");
        assert!(!com_papel.contains(&"escrever_nota".to_string()), "{com_papel:?}");
        assert!(com_papel.contains(&"ler_arquivo".to_string()), "{com_papel:?}");
        assert!(com_papel.len() < sem_papel.len());

        // E esconder não é impedir: chamar mesmo assim continua sendo recusado.
        let r = crate::mcp::tratar(
            &p.orq,
            &p.banco,
            &token,
            &serde_json::json!({
                "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": { "name": "escrever_arquivo",
                            "arguments": { "caminho": "x.txt", "conteudo": "oi" } },
            })
            .to_string(),
        );
        let v: serde_json::Value = serde_json::from_str(&r.corpo).unwrap();
        assert_eq!(v["result"]["isError"], true, "escondido mas não impedido");
    }

    // ---- host MCP: servidores de fora ------------------------------------

    #[test]
    fn ferramenta_de_servidor_externo_sempre_pede_card() {
        // O §8 é explícito: ação externa sempre pede aprovação, em qualquer
        // nível de autonomia. E aqui o hook é a ÚNICA linha de defesa — a
        // chamada vai direto do processo do agente para o servidor do outro, e
        // nós nunca a vemos.
        let crm = ServidorMcp {
            nome: "crm".into(),
            url: "https://exemplo.invalido/mcp".into(),
            cabecalhos: vec![("Authorization".into(), "Bearer segredo".into())],
        };

        // O matcher pega tudo do servidor, por curinga: não sabemos que
        // ferramentas ele oferece, e não precisamos saber.
        let matcher = crate::barramento::matcher_do_hook(std::slice::from_ref(&crm));
        assert!(matcher.contains("mcp__crm__.*"), "{matcher}");

        // E o veredito não deixa passar como se fosse leitura.
        assert!(crate::barramento::e_de_fora("mcp__crm__buscar_cliente", std::slice::from_ref(&crm)));
        assert!(!crate::barramento::e_de_fora("mcp__mutirao__ler_nota", std::slice::from_ref(&crm)));

        // Nunca aceita "não perguntar de novo": liberar o CRM de alguém de uma
        // vez seria acesso permanente num clique que ninguém lembra depois.
        assert!(!crate::barramento::aceita_regra("mcp__crm__buscar_cliente"));
    }

    #[test]
    fn nome_de_servidor_externo_nao_pode_virar_curinga() {
        // O nome entra num matcher que é REGEX. Um ponto ou asterisco aqui
        // viraria curinga e abriria buraco na aprovação sem ninguém perceber —
        // um servidor chamado ".*" faria `mcp__.*__.*` casar com tudo... e um
        // chamado "a|Bash" faria o matcher aceitar `Bash` como alternativa.
        let b = Banco::em_memoria().unwrap();
        let p = b
            .criar_papel("Vendas", "Você vende.", &[], Autonomia::Padrao, None, false)
            .unwrap();
        for ruim in [".*", "a|b", "crm-x", "com espaço", ""] {
            let erro = b.definir_mcp_do_papel(
                &p.id,
                &[ServidorMcp { nome: ruim.into(), url: "http://x".into(), cabecalhos: vec![] }],
            );
            assert!(erro.is_err(), "aceitou o nome {ruim:?}");
        }
        // E o nome honesto passa.
        let ok = b
            .definir_mcp_do_papel(
                &p.id,
                &[ServidorMcp {
                    nome: "crm_interno".into(),
                    url: "http://127.0.0.1:9/mcp".into(),
                    cabecalhos: vec![("X-Chave".into(), "segredo".into())],
                }],
            )
            .unwrap();
        assert_eq!(ok.mcp.len(), 1);
    }

    #[test]
    fn a_chave_do_servidor_externo_nao_atravessa_a_fronteira_ipc() {
        // Mesma regra do token da sessão: o que chega ao front chega a tudo
        // que roda no front, e a chave do CRM de alguém não é diferente.
        let b = Banco::em_memoria().unwrap();
        let p = b
            .criar_papel("Vendas", "Você vende.", &[], Autonomia::Padrao, None, false)
            .unwrap();
        let com_chave = b
            .definir_mcp_do_papel(
                &p.id,
                &[ServidorMcp {
                    nome: "crm".into(),
                    url: "http://127.0.0.1:9/mcp".into(),
                    cabecalhos: vec![("Authorization".into(), "Bearer segredo-do-crm".into())],
                }],
            )
            .unwrap();

        // O banco guarda, senão o adaptador não teria o que mandar ao servidor.
        assert_eq!(
            b.obter_papel(&p.id).unwrap().mcp[0].cabecalhos[0].1,
            "Bearer segredo-do-crm"
        );

        // Mas o que vai para o front, não.
        let json = serde_json::to_string(&com_chave.sem_segredos()).unwrap();
        assert!(!json.contains("segredo-do-crm"), "a chave vazou: {json}");
        assert!(json.contains("crm"), "o nome do servidor tem de aparecer: {json}");
    }
}
