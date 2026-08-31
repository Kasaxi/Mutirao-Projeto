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
    Orquestrador, PapelMensagem, PedidoAprovacao, Sink, TipoNo,
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
    assert!(
        d.esperar_os_dois(Duration::from_secs(420)),
        "travou: Pesquisador em {:?}, Redator em {:?}",
        d.estado(&d.a),
        d.estado(&d.b)
    );
    println!("a cadeia encerrou em {:?}", inicio.elapsed());

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
