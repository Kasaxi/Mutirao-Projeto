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
    Adaptador, Banco, EstadoSessao, Fabrica, FabricaClaude, Orquestrador, PapelMensagem, Sink,
    TipoNo,
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Bancada {
    banco: Arc<Mutex<Banco>>,
    orq: Orquestrador,
    sessao_id: String,
    _pasta: std::path::PathBuf,
}

fn bancada() -> Bancada {
    let pasta = std::env::temp_dir().join(format!("mutirao-ao-vivo-{}", nucleo::novo_id()));
    std::fs::create_dir_all(&pasta).unwrap();
    std::fs::write(pasta.join("contrato.txt"), "O prazo do contrato é de 18 meses.\n").unwrap();

    let banco = Banco::em_memoria().unwrap();
    let ws = banco.criar_workspace("Ao vivo", pasta.to_str().unwrap()).unwrap();
    let no = banco.criar_no(&ws.id, TipoNo::Agente, "Leitor", 0.0, 0.0).unwrap();
    let banco = Arc::new(Mutex::new(banco));

    let sink: Sink = Arc::new(|_| {});
    let fabrica: Arc<dyn Fabrica> = Arc::new(FabricaClaude::nova());
    let orq = Orquestrador::novo(banco.clone(), fabrica, sink);
    let sessao = orq.abrir_sessao(&no.id, Adaptador::Claude).unwrap();

    Bancada { banco, orq, sessao_id: sessao.id, _pasta: pasta }
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
