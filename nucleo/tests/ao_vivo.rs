//! Testes que rodam o Claude Code de verdade.
//!
//! **Gastam dinheiro e precisam de rede**, por isso são `#[ignore]`: um
//! `cargo test` normal continua offline, determinístico e de graça — que é a
//! razão de o adaptador falso existir. Estes aqui provam a outra metade: que a
//! tradução casa com a CLI instalada, e não só com o JSONL que guardamos.
//!
//! ```bash
//! cargo test -p nucleo --test ao_vivo -- --ignored --nocapture
//! ```
//!
//! Rode-os ao subir de versão da CLI. É lá que a forma dos eventos muda, e
//! quando muda, os testes de fixture continuam passando sozinhos e felizes.

use nucleo::{
    Adaptador, Banco, Barramento, Decisao, EstadoSessao, EventoNucleo, Fabrica, FabricaClaude,
    Orquestrador, PapelMensagem, PedidoAprovacao, Sink, TipoCabo, TipoNo,
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Bancada {
    banco: Arc<Mutex<Banco>>,
    orq: Arc<Orquestrador>,
    sessao_id: String,
    pasta: std::path::PathBuf,
    /// Cards que o barramento abriu. É por aqui que o teste "clica".
    pedidos: Arc<Mutex<Vec<PedidoAprovacao>>>,
    _barramento: Option<Barramento>,
}

fn bancada() -> Bancada {
    monta(false)
}

/// Com barramento no ar, a escrita fica liberada — e cada gravação para num
/// card, exatamente como pararia para o usuário.
fn bancada_com_aprovacao() -> Bancada {
    monta(true)
}

fn monta(com_barramento: bool) -> Bancada {
    let pasta = std::env::temp_dir().join(format!("mutirao-ao-vivo-{}", nucleo::novo_id()));
    std::fs::create_dir_all(&pasta).unwrap();
    std::fs::write(pasta.join("contrato.txt"), "O prazo do contrato é de 18 meses.\n").unwrap();

    let banco = Banco::em_memoria().unwrap();
    let ws = banco.criar_workspace("Ao vivo", pasta.to_str().unwrap()).unwrap();
    let no = banco.criar_no(&ws.id, TipoNo::Agente, "Leitor", 0.0, 0.0).unwrap();
    let banco = Arc::new(Mutex::new(banco));

    let pedidos: Arc<Mutex<Vec<PedidoAprovacao>>> = Arc::new(Mutex::new(Vec::new()));
    let copia = pedidos.clone();
    let sink: Sink = Arc::new(move |e| {
        if let EventoNucleo::AprovacaoPedida { pedido } = e {
            copia.lock().unwrap().push(pedido);
        }
    });

    let fabrica: Arc<dyn Fabrica> = Arc::new(FabricaClaude::nova());
    let orq = Arc::new(Orquestrador::novo(banco.clone(), fabrica, sink.clone()));

    let barramento = if com_barramento {
        let b = Barramento::subir(banco.clone(), orq.clone(), sink).unwrap();
        orq.ligar_barramento(b.url_base());
        Some(b)
    } else {
        None
    };

    let sessao = orq.abrir_sessao(&no.id, Adaptador::Claude).unwrap();
    Bancada {
        banco,
        orq,
        sessao_id: sessao.id,
        pasta,
        pedidos,
        _barramento: barramento,
    }
}

impl Bancada {
    fn estado(&self) -> EstadoSessao {
        self.banco.lock().unwrap().obter_sessao(&self.sessao_id).unwrap().estado
    }

    /// Turnos de verdade levam dezenas de segundos, não milissegundos.
    fn esperar_ocioso(&self, limite: Duration) -> bool {
        let inicio = Instant::now();
        while inicio.elapsed() < limite {
            if self.estado() == EstadoSessao::Ocioso {
                return true;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        false
    }

    /// Espera o card aparecer e devolve o pedido. É o "usuário olhando a tela".
    fn esperar_card(&self, limite: Duration) -> Option<PedidoAprovacao> {
        let inicio = Instant::now();
        while inicio.elapsed() < limite {
            if let Some(p) = self.pedidos.lock().unwrap().first().cloned() {
                return Some(p);
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        None
    }
}

#[test]
#[ignore = "gasta token e precisa do Claude Code instalado"]
fn a_cli_esta_instalada_e_responde() {
    let f = FabricaClaude::nova();
    let versao = nucleo::AdaptadorClaude::detectar(f.binario())
        .expect("Claude Code não encontrado — instale, ou aponte MUTIRAO_CLAUDE_BIN");
    println!("Claude Code: {versao}");
    assert!(!versao.is_empty());
}

#[test]
#[ignore = "gasta token e precisa do Claude Code instalado"]
fn turno_de_verdade_le_arquivo_responde_e_cobra() {
    let b = bancada();
    b.orq
        .enviar(&b.sessao_id, "leia contrato.txt e diga o prazo em uma frase curta")
        .unwrap();
    assert!(b.esperar_ocioso(Duration::from_secs(240)), "estado ficou em {:?}", b.estado());

    let banco = b.banco.lock().unwrap();
    let hist = banco.historico(&b.sessao_id, 50).unwrap();
    let resposta = hist
        .iter()
        .rev()
        .find(|m| m.papel == PapelMensagem::Agente)
        .expect("nenhuma resposta do agente");
    println!("resposta: {}", resposta.conteudo);
    assert!(resposta.conteudo.contains("18"), "não leu o arquivo: {}", resposta.conteudo);

    // O custo tem de vir da CLI, não da nossa tabela.
    let sessao = banco.obter_sessao(&b.sessao_id).unwrap();
    println!("custo do turno: US$ {:.6}", sessao.custo_total);
    assert!(sessao.custo_total > 0.0, "custo não chegou");
    assert!(sessao.sessao_externa_id.is_some(), "sem id de retomada não há amanhã");

    // A leitura do arquivo tem de ter virado linha de auditoria.
    let acoes = banco.ferramentas_da_sessao(&b.sessao_id).unwrap();
    println!("ações: {:?}", acoes.iter().map(|a| &a.ferramenta).collect::<Vec<_>>());
    assert!(acoes.iter().any(|a| a.ferramenta == "Read"), "esperava uma leitura");
}

/// Prompt de duas linhas — o conserto do Windows, na metade que dá para medir
/// daqui.
///
/// Desde este marco o prompt viaja pelo **stdin**, e não como argumento. A
/// razão é do Windows: instalada pelo npm, a CLI é um `claude.cmd`, e programa
/// em lote não aceita argumento com quebra de linha — o Rust recusa a chamada
/// antes de abrir o processo. Prompt de duas linhas é o caso comum, não o raro,
/// então o app quebraria no uso normal.
///
/// O que este teste prova: o prompt multilinha atravessa o cano inteiro e o
/// turno responde. O que ele não prova: que o `.cmd` roda — isso só o Windows
/// diz, e é por isso que o `processo.rs` diz em voz alta o que não mediu.
#[test]
#[ignore = "gasta token e precisa do Claude Code instalado"]
fn prompt_de_varias_linhas_chega_inteiro() {
    let b = bancada();
    b.orq
        .enviar(
            &b.sessao_id,
            "Vou te dar duas instruções, uma por linha.\n\
             Primeira: não leia arquivo nenhum.\n\
             Segunda: responda com a palavra ABACAXI e mais nada.",
        )
        .unwrap();
    assert!(b.esperar_ocioso(Duration::from_secs(240)), "estado ficou em {:?}", b.estado());

    let banco = b.banco.lock().unwrap();
    let hist = banco.historico(&b.sessao_id, 50).unwrap();
    let resposta = hist
        .iter()
        .rev()
        .find(|m| m.papel == PapelMensagem::Agente)
        .expect("nenhuma resposta do agente");
    println!("resposta: {}", resposta.conteudo);

    // A palavra está na ÚLTIMA linha do prompt. Se ela voltou, o prompt chegou
    // inteiro — que é a coisa toda que este teste existe para dizer.
    assert!(
        resposta.conteudo.to_uppercase().contains("ABACAXI"),
        "a última linha do prompt não chegou: {}",
        resposta.conteudo
    );
}

#[test]
#[ignore = "gasta token e precisa do Claude Code instalado"]
fn segundo_turno_retoma_a_conversa_em_vez_de_comecar_outra() {
    // É o critério "retomar sessão depois de fechar o app" do M1, medido do
    // jeito que importa: o agente precisa lembrar do turno anterior.
    let b = bancada();
    b.orq.enviar(&b.sessao_id, "leia contrato.txt e guarde o prazo").unwrap();
    assert!(b.esperar_ocioso(Duration::from_secs(240)), "primeiro turno não terminou");

    let externa = b.banco.lock().unwrap().obter_sessao(&b.sessao_id).unwrap().sessao_externa_id;
    assert!(externa.is_some());

    b.orq.enviar(&b.sessao_id, "qual era o prazo? responda só o número").unwrap();
    assert!(b.esperar_ocioso(Duration::from_secs(240)), "segundo turno não terminou");

    let banco = b.banco.lock().unwrap();
    let hist = banco.historico(&b.sessao_id, 50).unwrap();
    let ultima = hist.iter().rev().find(|m| m.papel == PapelMensagem::Agente).unwrap();
    println!("segunda resposta: {}", ultima.conteudo);
    assert!(
        ultima.conteudo.contains("18"),
        "não lembrou do turno anterior: {}",
        ultima.conteudo
    );
    assert_eq!(
        banco.obter_sessao(&b.sessao_id).unwrap().sessao_externa_id,
        externa,
        "a retomada não pode trocar de sessão externa"
    );
}

#[test]
#[ignore = "gasta token e precisa do Claude Code instalado"]
fn retomada_de_sessao_que_nao_existe_diz_o_que_houve() {
    // O erro que a CLI reporta sem texto no `result` — a frase útil só existe
    // no stderr. Se este teste voltar a mostrar "erro (error_during_execution)",
    // o adiamento do evento parou de funcionar.
    let b = bancada();
    {
        let banco = b.banco.lock().unwrap();
        banco
            .definir_sessao_externa(&b.sessao_id, "00000000-0000-0000-0000-000000000000")
            .unwrap();
    }
    b.orq.enviar(&b.sessao_id, "oi").unwrap();

    let inicio = Instant::now();
    while inicio.elapsed() < Duration::from_secs(90) {
        if b.estado() == EstadoSessao::Erro {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert_eq!(b.estado(), EstadoSessao::Erro, "esperava erro");

    let banco = b.banco.lock().unwrap();
    let ultima = banco.historico(&b.sessao_id, 10).unwrap().pop().unwrap();
    println!("mensagem de erro: {}", ultima.conteudo);
    assert!(
        ultima.conteudo.to_lowercase().contains("session"),
        "o stderr da CLI não chegou ao usuário: {}",
        ultima.conteudo
    );
}

// ================================================================== M2 =====
// "o agente monta um arquivo na minha pasta e eu aprovo a gravação antes de
// acontecer". Contra o Claude Code de verdade e o barramento de verdade.

#[test]
#[ignore = "gasta token e precisa do Claude Code instalado"]
fn sem_barramento_o_agente_nao_consegue_gravar() {
    // A trava mais importante do M2: sem quem aprove, não se escreve. Se este
    // teste passar a criar o arquivo, a aprovação virou enfeite.
    let b = bancada();
    let alvo = b.pasta.join("nao-deveria-existir.txt");
    b.orq
        .enviar(&b.sessao_id, "crie o arquivo nao-deveria-existir.txt com o texto: oi")
        .unwrap();
    assert!(b.esperar_ocioso(Duration::from_secs(240)), "turno não terminou");
    assert!(!alvo.exists(), "gravou sem barramento: {}", alvo.display());
}

#[test]
#[ignore = "gasta token e precisa do Claude Code instalado"]
fn aprovar_deixa_gravar_e_o_arquivo_aparece_na_pasta() {
    let b = bancada_com_aprovacao();
    let alvo = b.pasta.join("resumo.md");
    b.orq
        .enviar(
            &b.sessao_id,
            "crie o arquivo resumo.md com exatamente este conteúdo: # Resumo\n\nPrazo: 18 meses.",
        )
        .unwrap();

    let pedido = b.esperar_card(Duration::from_secs(180)).expect("nenhum card apareceu");
    println!("card: {} — {}", pedido.resumo, pedido.detalhe);
    assert!(pedido.resumo.contains("resumo.md"), "resumo: {}", pedido.resumo);
    assert!(pedido.previa.is_some(), "o card precisa mostrar o que vai ser gravado");

    // O arquivo AINDA não existe: é isso que torna o card honesto.
    assert!(!alvo.exists(), "gravou antes de eu aprovar");

    b.orq.decidir_aprovacao(&pedido.tool_call_id, Decisao::Aprovada, false).unwrap();
    assert!(b.esperar_ocioso(Duration::from_secs(240)), "turno não terminou");

    let conteudo = std::fs::read_to_string(&alvo).expect("o arquivo devia existir agora");
    println!("gravado: {conteudo:?}");
    assert!(conteudo.contains("18"), "conteúdo: {conteudo}");

    let chamada = b.banco.lock().unwrap().obter_ferramenta(&pedido.tool_call_id).unwrap();
    assert_eq!(chamada.decidido_por.as_deref(), Some("usuario"));
}

#[test]
#[ignore = "gasta token e precisa do Claude Code instalado"]
fn negar_impede_a_gravacao_de_verdade() {
    let b = bancada_com_aprovacao();
    let alvo = b.pasta.join("negado.txt");
    b.orq
        .enviar(&b.sessao_id, "crie o arquivo negado.txt com o texto: nao deveria existir")
        .unwrap();

    let pedido = b.esperar_card(Duration::from_secs(180)).expect("nenhum card apareceu");
    b.orq.decidir_aprovacao(&pedido.tool_call_id, Decisao::Negada, false).unwrap();
    assert!(b.esperar_ocioso(Duration::from_secs(240)), "turno não terminou");

    assert!(!alvo.exists(), "o arquivo foi gravado mesmo negado: {}", alvo.display());

    let banco = b.banco.lock().unwrap();
    let ultima = banco
        .historico(&b.sessao_id, 20)
        .unwrap()
        .into_iter()
        .rev()
        .find(|m| m.papel == PapelMensagem::Agente);
    // O agente precisa ter entendido o "não" e explicado, não tentado de novo
    // por outro caminho.
    println!("resposta depois do não: {:?}", ultima.map(|m| m.conteudo));
}

#[test]
#[ignore = "gasta token e precisa do Claude Code instalado"]
fn ler_nao_abre_card() {
    // Um card por arquivo aberto viraria ruído, e card que vira ruído é card
    // que o usuário aprova sem ler.
    let b = bancada_com_aprovacao();
    b.orq.enviar(&b.sessao_id, "leia contrato.txt e diga o prazo").unwrap();
    assert!(b.esperar_ocioso(Duration::from_secs(240)), "turno não terminou");
    assert!(
        b.pedidos.lock().unwrap().is_empty(),
        "leitura não devia pedir aprovação: {:?}",
        b.pedidos.lock().unwrap()
    );
}

// ==================================================================== M3 ===
// "Pesquisador entrega ao Redator sem eu tocar, e um ciclo A→B→A encerra
// sozinho sem travar o app" — o critério de pronto do M3, medido do único
// jeito que conta: com dois processos do Claude Code de verdade, um falando
// com o outro pelo servidor MCP do barramento.

/// Dois agentes ligados por `fala_com`, com o barramento no ar.
struct Dupla {
    banco: Arc<Mutex<Banco>>,
    orq: Arc<Orquestrador>,
    a: String,
    b: String,
    a_no: String,
    b_no: String,
    recados: Arc<Mutex<Vec<(String, String)>>>,
    pasta: std::path::PathBuf,
    _barramento: Barramento,
}

fn dupla() -> Dupla {
    let pasta = std::env::temp_dir().join(format!("mutirao-ponte-{}", nucleo::novo_id()));
    std::fs::create_dir_all(&pasta).unwrap();
    std::fs::write(
        pasta.join("contrato.txt"),
        "O prazo do contrato é de 18 meses, com reajuste anual pelo IGP-M.\n",
    )
    .unwrap();

    let banco = Banco::em_memoria().unwrap();
    let ws = banco.criar_workspace("Ponte", pasta.to_str().unwrap()).unwrap();
    let a = banco.criar_no(&ws.id, TipoNo::Agente, "Pesquisador", 0.0, 0.0).unwrap();
    let b = banco.criar_no(&ws.id, TipoNo::Agente, "Redator", 400.0, 0.0).unwrap();
    banco.criar_cabo(&ws.id, &a.id, &b.id, nucleo::TipoCabo::FalaCom).unwrap();
    let banco = Arc::new(Mutex::new(banco));

    let recados: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let copia = recados.clone();
    let sink: Sink = Arc::new(move |e| {
        if let EventoNucleo::NoMensagem { de_node, para_node, .. } = e {
            copia.lock().unwrap().push((de_node, para_node));
        }
    });

    let fabrica: Arc<dyn Fabrica> = Arc::new(FabricaClaude::nova());
    let orq = Arc::new(Orquestrador::novo(banco.clone(), fabrica, sink.clone()));
    let barramento = Barramento::subir(banco.clone(), orq.clone(), sink).unwrap();
    orq.ligar_barramento(barramento.url_base());

    let sa = orq.abrir_sessao(&a.id, Adaptador::Claude).unwrap();
    let sb = orq.abrir_sessao(&b.id, Adaptador::Claude).unwrap();

    Dupla {
        banco,
        orq,
        a: sa.id,
        b: sb.id,
        a_no: a.id,
        b_no: b.id,
        recados,
        pasta,
        _barramento: barramento,
    }
}

impl Dupla {
    fn estado(&self, sessao: &str) -> EstadoSessao {
        self.banco.lock().unwrap().obter_sessao(sessao).unwrap().estado
    }

    /// Espera os DOIS nós pararem. Um só parado não prova nada: o outro pode
    /// estar pendurado esperando, que é justamente a falha a evitar.
    fn esperar_os_dois(&self, limite: Duration) -> bool {
        let inicio = Instant::now();
        while inicio.elapsed() < limite {
            let parado = |e| matches!(e, EstadoSessao::Ocioso | EstadoSessao::Erro);
            if parado(self.estado(&self.a)) && parado(self.estado(&self.b)) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        false
    }

    /// Espera os dois pararem, **respondendo como a pessoa responderia**.
    ///
    /// Um agente que levanta a mão não é travamento: é uma pergunta, e no app
    /// existe alguém na frente da tela para respondê-la. Sem isto o teste
    /// mediria uma situação que o app não tem — um canvas sem ninguém — e foi
    /// exatamente onde ele quebrou: o Pesquisador esperando o Redator, o
    /// Redator esperando uma pessoa que o teste nunca fingiu ser.
    ///
    /// Devolve também quantas perguntas apareceram, porque o desfecho muda de
    /// significado conforme elas existam ou não.
    fn esperar_os_dois_com_gente(&self, limite: Duration) -> (bool, usize) {
        let inicio = Instant::now();
        let mut perguntas = 0;
        while inicio.elapsed() < limite {
            for s in [&self.a, &self.b] {
                if self.estado(s) != EstadoSessao::AguardandoHumano {
                    continue;
                }
                perguntas += 1;
                let _ = self.orq.enviar(s, "Decida você, com o que já tem.");
                // Espera sair de `aguardando_humano` antes de olhar de novo.
                // Sem isto, a mesma pergunta seria respondida duas vezes — e a
                // segunda mensagem não é resposta, é turno novo.
                let ate = Instant::now();
                while self.estado(s) == EstadoSessao::AguardandoHumano
                    && ate.elapsed() < Duration::from_secs(10)
                {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            let parado = |e| matches!(e, EstadoSessao::Ocioso | EstadoSessao::Erro);
            if parado(self.estado(&self.a)) && parado(self.estado(&self.b)) {
                return (true, perguntas);
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        (false, perguntas)
    }

    fn conversa(&self, sessao: &str) -> Vec<nucleo::Mensagem> {
        self.banco.lock().unwrap().historico(sessao, 50).unwrap()
    }
}

impl Drop for Dupla {
    fn drop(&mut self) {
        self.orq.encerrar_tudo();
        let _ = std::fs::remove_dir_all(&self.pasta);
    }
}

#[test]
#[ignore = "gasta token e precisa do Claude Code instalado"]
fn o_pesquisador_entrega_ao_redator_sem_eu_tocar() {
    let d = dupla();
    d.orq
        .enviar(
            &d.a,
            "Leia contrato.txt. Depois use a ferramenta enviar_para para pedir ao nó \
             Redator que escreva uma frase de aviso sobre o reajuste, e me devolva a \
             frase que ele responder. Não escreva a frase você mesmo.",
        )
        .unwrap();

    assert!(
        d.esperar_os_dois(Duration::from_secs(420)),
        "os nós não pararam: Pesquisador em {:?}, Redator em {:?}",
        d.estado(&d.a),
        d.estado(&d.b)
    );

    // 1. O recado atravessou, e a interface soube — é o que anima o cabo.
    let recados = d.recados.lock().unwrap().clone();
    println!("recados: {recados:?}");
    assert!(
        recados.iter().any(|(de, para)| de == &d.a_no && para == &d.b_no),
        "nenhum recado do Pesquisador para o Redator"
    );

    // 2. O Redator recebeu como recado de nó, com a origem e a cadeia — não
    //    como se o usuário tivesse falado com ele.
    let dele = d.conversa(&d.b);
    let pedido = dele
        .iter()
        .find(|m| m.papel == PapelMensagem::No)
        .expect("o Redator não recebeu recado nenhum");
    println!("o Redator recebeu: {}", pedido.conteudo);
    assert_eq!(pedido.origem_node.as_deref(), Some(d.a_no.as_str()));
    let cadeia = pedido.trace_id.clone().expect("recado sem cadeia");

    // 3. O Redator trabalhou de verdade.
    let resposta = dele
        .iter()
        .rev()
        .find(|m| m.papel == PapelMensagem::Agente)
        .expect("o Redator não respondeu");
    println!("o Redator respondeu: {}", resposta.conteudo);
    assert!(!resposta.conteudo.trim().is_empty());

    // 4. E a resposta dele voltou para a conversa do Pesquisador, na MESMA
    //    cadeia — é por ela que o orçamento soma os dois lados.
    let minha = d.conversa(&d.a);
    let final_ = minha
        .iter()
        .rev()
        .find(|m| m.papel == PapelMensagem::Agente)
        .expect("o Pesquisador não respondeu");
    println!("o Pesquisador entregou: {}", final_.conteudo);
    assert_eq!(final_.trace_id.as_deref(), Some(cadeia.as_str()), "a cadeia se perdeu no caminho");

    let gasto = d.banco.lock().unwrap().custo_do_trace(&cadeia).unwrap();
    println!("a cadeia inteira custou US$ {gasto:.6}");
    assert!(gasto > 0.0, "o custo por cadeia não somou os dois nós");
}

#[test]
#[ignore = "gasta token e precisa do Claude Code instalado"]
fn um_ciclo_entre_dois_nos_encerra_sozinho() {
    // A outra metade do critério: "sem travar o app". Aqui os dois são
    // instruídos a perguntar um ao outro — que é o pior caso, a espera
    // cruzada. Tem de acabar depressa, não no prazo.
    //
    // O que este teste NÃO garante é qual limite desatou. O modelo pode muito
    // bem responder em vez de perguntar de volta, e aí a espera cruzada nem
    // acontece (foi o que deu na primeira execução: 21 s, o Redator devolveu
    // perguntas em texto em vez de chamar `enviar_para`). A prova determinística
    // da detecção de ciclo é o teste offline
    // `dois_nos_esperando_um_pelo_outro_nao_travam_o_app`, que usa um adaptador
    // teimoso e falha se a checagem for removida. Aqui a pergunta é outra, e é
    // a que só a CLI de verdade responde: com dois processos de verdade
    // conversando, o app trava?
    //
    // E ele **finge ser a pessoa**, o que antes não fazia. O terceiro desfecho
    // possível é o Redator chamar `perguntar_humano` em vez de responder — e
    // aí a cadeia para de propósito, esperando um clique que num teste sem
    // gente nunca vem. Foi assim que este teste passou a falhar, e a falha era
    // do teste: media um canvas sem ninguém na frente, que não é o produto.
    let d = dupla();
    d.orq
        .enviar(
            &d.a,
            "Use a ferramenta enviar_para e pergunte ao nó Redator qual é a opinião \
             DELE sobre o contrato. Peça que ele também consulte você antes de \
             responder. Depois me conte o que aconteceu.",
        )
        .unwrap();

    let inicio = Instant::now();
    let (parou, perguntas) = d.esperar_os_dois_com_gente(Duration::from_secs(420));
    assert!(
        parou,
        "travou: Pesquisador em {:?}, Redator em {:?}",
        d.estado(&d.a),
        d.estado(&d.b)
    );
    println!("a cadeia encerrou em {:?}, com {perguntas} pergunta(s) à pessoa", inicio.elapsed());

    // O prazo padrão de uma mensagem é de dez minutos. Se foi o prazo que
    // desatou isto, não encerrou sozinho — expirou.
    assert!(
        inicio.elapsed() < Duration::from_secs(400),
        "só destravou perto do prazo; o limite não pegou"
    );

    let minha = d.conversa(&d.a);
    for m in &minha {
        println!("[{:?}] {}", m.papel, m.conteudo.chars().take(200).collect::<String>());
    }
    assert!(
        minha.iter().any(|m| m.papel == PapelMensagem::Agente),
        "o Pesquisador não chegou a responder nada"
    );
}

// ==================================================================== M4 ===
// "um prompt monta um time de quatro, e amanhã eu reabro o mesmo time como
// estava" — o critério de pronto do M4.

/// Um Organizador sozinho num workspace, com a biblioteca de papéis no banco.
struct Maestro {
    banco: Arc<Mutex<Banco>>,
    orq: Arc<Orquestrador>,
    sessao: String,
    ws: String,
    pasta: std::path::PathBuf,
    _barramento: Barramento,
}

fn maestro() -> Maestro {
    let pasta = std::env::temp_dir().join(format!("mutirao-time-{}", nucleo::novo_id()));
    std::fs::create_dir_all(&pasta).unwrap();
    std::fs::write(
        pasta.join("contrato.txt"),
        "Prazo: 18 meses. Reajuste anual pelo IGP-M. Multa por atraso: 2% ao mês.\n",
    )
    .unwrap();

    let banco = Banco::em_memoria().unwrap();
    let ws = banco.criar_workspace("Time", pasta.to_str().unwrap()).unwrap();
    let papel = banco.papel_por_nome("Organizador").unwrap().expect("biblioteca embutida");
    let no = banco
        .criar_no_recrutado(&ws.id, TipoNo::Agente, "Chefe", 0.0, 0.0, Some(&papel.id), None)
        .unwrap();
    let banco = Arc::new(Mutex::new(banco));

    let sink: Sink = Arc::new(|_| {});
    let fabrica: Arc<dyn Fabrica> = Arc::new(FabricaClaude::nova());
    let orq = Arc::new(Orquestrador::novo(banco.clone(), fabrica, sink.clone()));
    let barramento = Barramento::subir(banco.clone(), orq.clone(), sink).unwrap();
    orq.ligar_barramento(barramento.url_base());

    let sessao = orq.abrir_sessao(&no.id, Adaptador::Claude).unwrap();
    Maestro {
        banco,
        orq,
        sessao: sessao.id,
        ws: ws.id,
        pasta,
        _barramento: barramento,
    }
}

impl Maestro {
    fn agentes(&self) -> Vec<nucleo::No> {
        self.banco
            .lock()
            .unwrap()
            .listar_nos(&self.ws)
            .unwrap()
            .into_iter()
            .filter(|n| n.tipo == TipoNo::Agente)
            .collect()
    }

    /// Espera TODOS os nós pararem. Um só parado não prova nada num time.
    ///
    /// **Responde como a pessoa responderia.** Qualquer membro do time pode
    /// levantar a mão, e um time de quatro tem quatro chances de fazer isso.
    /// Sem responder, o teste mede um canvas sem ninguém na frente — que não é
    /// o produto — e fica esperando um clique que nunca vem. É a mesma
    /// correção do `esperar_os_dois_com_gente`, e pelo mesmo motivo: desde que
    /// a espera por pergunta não conta contra o prazo (`esperar_resposta`), o
    /// teste sem gente para de terminar em vez de terminar errado.
    fn esperar_o_time(&self, limite: Duration) -> bool {
        let inicio = Instant::now();
        while inicio.elapsed() < limite {
            let sessoes: Vec<nucleo::Sessao> = {
                let banco = self.banco.lock().unwrap();
                banco
                    .listar_nos(&self.ws)
                    .unwrap()
                    .iter()
                    .filter(|n| n.tipo == TipoNo::Agente)
                    .filter_map(|n| banco.sessao_do_no(&n.id).ok().flatten())
                    .collect()
            };

            for s in sessoes.iter().filter(|s| s.estado == EstadoSessao::AguardandoHumano) {
                let _ = self.orq.enviar(&s.id, "Decida você, com o que já tem.");
                // Espera sair de `aguardando_humano`: a segunda mensagem a uma
                // sessão que já respondeu não é resposta, é turno novo.
                let ate = Instant::now();
                while ate.elapsed() < Duration::from_secs(10) {
                    let agora = self.banco.lock().unwrap().obter_sessao(&s.id).unwrap().estado;
                    if agora != EstadoSessao::AguardandoHumano {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }

            if sessoes
                .iter()
                .all(|s| matches!(s.estado, EstadoSessao::Ocioso | EstadoSessao::Erro))
            {
                return true;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        false
    }
}

impl Drop for Maestro {
    fn drop(&mut self) {
        self.orq.encerrar_tudo();
        let _ = std::fs::remove_dir_all(&self.pasta);
    }
}

#[test]
#[ignore = "gasta token e precisa do Claude Code instalado"]
fn um_prompt_monta_um_time() {
    let m = maestro();
    m.orq
        .enviar(
            &m.sessao,
            "Preciso de um parecer sobre o contrato.txt desta pasta. Monte um time: \
             recrute um Pesquisador chamado Ana para ler o contrato e um Redator \
             chamado Bruno para escrever o parecer. Depois peça a cada um a parte \
             dele com enviar_para e me entregue o resultado.",
        )
        .unwrap();

    // Quinze minutos, e não os cinco de antes. Medido em quatro execuções: 176 s
    // com um time de três, 478 s com um de cinco. O Organizador decide sozinho
    // quantos recrutar — o prompt pede dois e ele às vezes traz quatro —, e cada
    // agente a mais é um turno a mais. Cinco minutos cabia no caso bom e
    // estourava no caso normal, o que faz o teste falhar por orçamento e não
    // por defeito. Quem impede o time de crescer sem fim são os tetos de
    // recrutamento, com teste offline próprio; não é papel do relógio daqui.
    if !m.esperar_o_time(Duration::from_secs(900)) {
        // Diagnóstico, não decoração: "o time não parou" sozinho não diz se
        // alguém travou, se alguém está esperando, ou se o Chefe entrou em
        // laço de recrutar. Sem isto a próxima investigação recomeça do zero.
        let banco = m.banco.lock().unwrap();
        for n in m.agentes() {
            let s = banco.sessao_do_no(&n.id).ok().flatten();
            let estado = s.as_ref().map(|s| format!("{:?}", s.estado)).unwrap_or("—".into());
            let ultima = s
                .as_ref()
                .and_then(|s| banco.historico(&s.id, 3).ok())
                .and_then(|h| h.last().map(|m| m.conteudo.chars().take(120).collect::<String>()))
                .unwrap_or_default();
            println!("  {} [{}] papel={:?} — {}", n.nome, estado, n.role_id.is_some(), ultima);
        }
        drop(banco);
        panic!(
            "o time não parou; agentes: {:?}",
            m.agentes().iter().map(|n| &n.nome).collect::<Vec<_>>()
        );
    }

    let agentes = m.agentes();
    println!("time montado: {:?}", agentes.iter().map(|n| &n.nome).collect::<Vec<_>>());
    assert!(agentes.len() >= 3, "o Chefe não recrutou ninguém: {}", agentes.len());

    // Os recrutados vieram com papel e com quem os recrutou — sem isso são
    // agentes anônimos com nome bonito.
    let banco = m.banco.lock().unwrap();
    let recrutados: Vec<&nucleo::No> =
        agentes.iter().filter(|n| n.recrutado_por.is_some()).collect();
    assert!(!recrutados.is_empty(), "ninguém foi marcado como recrutado");
    for r in &recrutados {
        let papel = r
            .role_id
            .as_deref()
            .map(|id| banco.obter_papel(id).unwrap().nome)
            .unwrap_or_default();
        println!("  {} — papel {papel}", r.nome);
        assert!(!papel.is_empty(), "{} veio sem papel", r.nome);
        // E ligado a quem recrutou: sem cabo o recrutado é uma ilha.
        let vizinhos = banco.vizinhos(&r.id, TipoCabo::FalaCom).unwrap();
        assert!(vizinhos.contains(r.recrutado_por.as_ref().unwrap()), "{} ficou solto", r.nome);
    }

    // E o time trabalhou: alguém além do Chefe recebeu recado de nó.
    let trabalhou = recrutados.iter().any(|r| {
        banco
            .sessao_do_no(&r.id)
            .ok()
            .flatten()
            .map(|s| {
                banco
                    .historico(&s.id, 50)
                    .unwrap_or_default()
                    .iter()
                    .any(|msg| msg.papel == PapelMensagem::No)
            })
            .unwrap_or(false)
    });
    assert!(trabalhou, "o time foi montado e ninguém trabalhou");
}

#[test]
#[ignore = "gasta token e precisa do Claude Code instalado"]
fn o_papel_muda_o_que_o_agente_faz_de_verdade() {
    // O papel só vale se o modelo obedecer. Este teste é o único que prova
    // isso: o Pesquisador é instruído a NÃO escrever, e recebe um pedido para
    // escrever. Ele tem de recusar e mandar para quem escreve.
    let m = maestro();
    let (pesquisador, sessao) = {
        let banco = m.banco.lock().unwrap();
        let papel = banco.papel_por_nome("Pesquisador").unwrap().unwrap();
        let no = banco
            .criar_no_recrutado(
                &m.ws,
                TipoNo::Agente,
                "Ana",
                400.0,
                0.0,
                Some(&papel.id),
                None,
            )
            .unwrap();
        drop(banco);
        let s = m.orq.abrir_sessao(&no.id, Adaptador::Claude).unwrap();
        (no, s)
    };

    m.orq
        .enviar(
            &sessao.id,
            "Leia contrato.txt e grave um arquivo parecer.txt com o resumo.",
        )
        .unwrap();
    assert!(m.esperar_o_time(Duration::from_secs(300)), "o turno não terminou");

    let banco = m.banco.lock().unwrap();
    let resposta = banco
        .historico(&sessao.id, 50)
        .unwrap()
        .into_iter()
        .rev()
        .find(|msg| msg.papel == PapelMensagem::Agente)
        .expect("sem resposta");
    println!("o Pesquisador respondeu: {}", resposta.conteudo);

    // A prova dura: o arquivo NÃO existe. O papel `cauteloso` não recebe
    // ferramenta de escrita nenhuma, nem nativa nem do §6.
    assert!(
        !m.pasta.join("parecer.txt").exists(),
        "o Pesquisador gravou apesar do papel dizer que não escreve"
    );
    let _ = pesquisador;
}

#[test]
#[ignore = "gasta token e precisa do Claude Code instalado"]
fn amanha_eu_reabro_o_mesmo_time() {
    // A segunda metade do critério. Não usa a CLI para nada além de existir:
    // o que se mede aqui é que o time salvo volta inteiro, com papéis e cabos.
    let m = maestro();
    {
        let banco = m.banco.lock().unwrap();
        for (nome, papel) in [("Ana", "Pesquisador"), ("Bruno", "Redator"), ("Célia", "Revisor")] {
            let p = banco.papel_por_nome(papel).unwrap().unwrap();
            let no = banco
                .criar_no_recrutado(&m.ws, TipoNo::Agente, nome, 400.0, 0.0, Some(&p.id), None)
                .unwrap();
            let chefe = banco.listar_nos(&m.ws).unwrap()[0].id.clone();
            banco.criar_cabo(&m.ws, &chefe, &no.id, TipoCabo::FalaCom).unwrap();
        }
    }
    assert_eq!(m.agentes().len(), 4, "o time de quatro");

    // Salva, e abre num workspace novo — que é o teste de verdade de
    // "reabrir", porque prova que a partitura não depende dos ids de origem.
    let banco = m.banco.lock().unwrap();
    let snapshot = nucleo::partituras::fotografar(&banco, &m.ws).unwrap();
    let partitura = banco.salvar_partitura(&m.ws, "Parecer de contrato", &snapshot).unwrap();

    let outra = std::env::temp_dir().join(format!("mutirao-amanha-{}", nucleo::novo_id()));
    std::fs::create_dir_all(&outra).unwrap();
    let ws2 = banco.criar_workspace("Amanhã", outra.to_str().unwrap()).unwrap();
    let novos = nucleo::partituras::montar(&banco, &ws2.id, &partitura).unwrap();

    assert_eq!(novos.len(), 4);
    let mut com_papel = 0;
    for n in &novos {
        if let Some(id) = &n.role_id {
            println!("  {} — {}", n.nome, banco.obter_papel(id).unwrap().nome);
            com_papel += 1;
        }
    }
    assert_eq!(com_papel, 4, "alguém voltou sem papel");
    assert!(!banco.listar_cabos(&ws2.id).unwrap().is_empty(), "os cabos não voltaram");
    let _ = std::fs::remove_dir_all(&outra);
}

// ==================================================================== M5 ===
// "dois ensaios do mesmo trabalho rodam ao mesmo tempo e eu publico um deles
// sem entender de Git" — o critério de pronto do M5.

struct DoisRascunhos {
    banco: Arc<Mutex<Banco>>,
    orq: Arc<Orquestrador>,
    ws: String,
    no: String,
    pasta: std::path::PathBuf,
    repo: std::path::PathBuf,
    _barramento: Barramento,
}

fn dois_rascunhos() -> Option<DoisRascunhos> {
    if !nucleo::git::existe() {
        eprintln!("git não instalado; pulando");
        return None;
    }
    let raiz = std::env::temp_dir().join(format!("mutirao-m5-{}", nucleo::novo_id()));
    let pasta = raiz.join("obra");
    let repo = raiz.join("historico");
    std::fs::create_dir_all(&pasta).unwrap();
    std::fs::write(
        pasta.join("contrato.txt"),
        "Prazo: 18 meses. Reajuste anual pelo IGP-M.\n",
    )
    .unwrap();

    let banco = Banco::em_memoria().unwrap();
    let ws = banco.criar_workspace("Obra", pasta.to_str().unwrap()).unwrap();
    banco.definir_repo(&ws.id, repo.to_str().unwrap()).unwrap();
    let no = banco.criar_no(&ws.id, TipoNo::Agente, "Leitor", 0.0, 0.0).unwrap();
    let banco = Arc::new(Mutex::new(banco));

    let sink: Sink = Arc::new(|_| {});
    let fabrica: Arc<dyn Fabrica> = Arc::new(FabricaClaude::nova());
    let orq = Arc::new(Orquestrador::novo(banco.clone(), fabrica, sink.clone()));
    let barramento = Barramento::subir(banco.clone(), orq.clone(), sink).unwrap();
    orq.ligar_barramento(barramento.url_base());
    nucleo::ensaios::preparar(&banco.lock().unwrap(), &ws.id).unwrap();

    Some(DoisRascunhos {
        banco,
        orq,
        ws: ws.id,
        no: no.id,
        pasta,
        repo,
        _barramento: barramento,
    })
}

impl DoisRascunhos {
    fn esperar(&self, sessao: &str, limite: Duration) -> bool {
        let inicio = Instant::now();
        while inicio.elapsed() < limite {
            let e = self.banco.lock().unwrap().obter_sessao(sessao).unwrap().estado;
            if matches!(e, EstadoSessao::Ocioso | EstadoSessao::Erro) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        false
    }

    /// Aprova tudo que o agente pedir. O card é do M2 e já tem teste próprio;
    /// aqui ele é só o caminho até a gravação acontecer.
    fn aprovar_tudo(&self) {
        let orq = self.orq.clone();
        let banco = self.banco.clone();
        std::thread::spawn(move || {
            for _ in 0..600 {
                let pendentes: Vec<String> = banco
                    .lock()
                    .unwrap()
                    .conn_pendentes()
                    .unwrap_or_default();
                for id in pendentes {
                    let _ = orq.decidir_aprovacao(&id, Decisao::Aprovada, false);
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        });
    }
}

impl Drop for DoisRascunhos {
    fn drop(&mut self) {
        self.orq.encerrar_tudo();
        let _ = std::fs::remove_dir_all(self.pasta.parent().unwrap());
        let _ = &self.repo;
    }
}

#[test]
#[ignore = "gasta token e precisa do Claude Code instalado"]
fn dois_agentes_trabalham_em_dois_rascunhos_do_mesmo_trabalho() {
    let Some(d) = dois_rascunhos() else { return };
    d.aprovar_tudo();

    let a = nucleo::ensaios::criar(&d.banco.lock().unwrap(), &d.ws, "Prazo maior").unwrap();
    let b = nucleo::ensaios::criar(&d.banco.lock().unwrap(), &d.ws, "Prazo menor").unwrap();

    // O MESMO nó trabalha nos dois, um de cada vez — que é o que a interface
    // faz quando o usuário troca de rascunho.
    for (ensaio, pedido, esperado) in [
        (&a, "Reescreva contrato.txt trocando o prazo para 24 meses. Só isso.", "24"),
        (&b, "Reescreva contrato.txt trocando o prazo para 12 meses. Só isso.", "12"),
    ] {
        nucleo::ensaios::trocar(&d.banco.lock().unwrap(), &d.orq, &d.ws, Some(&ensaio.id))
            .unwrap();
        let sessao = d.orq.abrir_sessao(&d.no, Adaptador::Claude).unwrap();
        d.orq.enviar(&sessao.id, pedido).unwrap();
        assert!(
            d.esperar(&sessao.id, Duration::from_secs(300)),
            "o turno em \"{}\" não terminou",
            ensaio.nome
        );

        let escrito = std::fs::read_to_string(
            std::path::Path::new(&ensaio.caminho_worktree).join("contrato.txt"),
        )
        .unwrap();
        println!("{} → {}", ensaio.nome, escrito.trim());
        assert!(escrito.contains(esperado), "\"{}\" ficou com: {escrito}", ensaio.nome);
    }

    // Os dois rascunhos guardam versões diferentes ao mesmo tempo...
    let em_a = std::fs::read_to_string(
        std::path::Path::new(&a.caminho_worktree).join("contrato.txt"),
    )
    .unwrap();
    let em_b = std::fs::read_to_string(
        std::path::Path::new(&b.caminho_worktree).join("contrato.txt"),
    )
    .unwrap();
    assert!(em_a.contains("24") && em_b.contains("12"), "um rascunho vazou no outro");

    // ...e a pasta de verdade não mudou nada.
    let de_verdade = std::fs::read_to_string(d.pasta.join("contrato.txt")).unwrap();
    assert!(
        de_verdade.contains("18"),
        "o trabalho de um rascunho vazou para a pasta de verdade: {de_verdade}"
    );

    // Publicar um deles leva o trabalho — sem o usuário entender de Git.
    nucleo::ensaios::trocar(&d.banco.lock().unwrap(), &d.orq, &d.ws, None).unwrap();
    let feito =
        nucleo::ensaios::publicar(&d.banco.lock().unwrap(), &d.orq, &a.id, &[]).unwrap();
    println!("publicado: {:?}", feito.alteracoes);

    let publicado = std::fs::read_to_string(d.pasta.join("contrato.txt")).unwrap();
    println!("a pasta de verdade agora: {}", publicado.trim());
    assert!(publicado.contains("24"), "publicar não levou o trabalho: {publicado}");

    // E o outro rascunho continua lá, intocado, com a versão dele.
    let b_depois = std::fs::read_to_string(
        std::path::Path::new(&b.caminho_worktree).join("contrato.txt"),
    )
    .unwrap();
    assert!(b_depois.contains("12"), "publicar um rascunho mexeu no outro");

    // A pasta do usuário continua limpa: nenhum vestígio de Git.
    let dentro: Vec<String> = std::fs::read_dir(&d.pasta)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert!(!dentro.iter().any(|n| n.starts_with('.')), "sobrou coisa oculta: {dentro:?}");
}
