// Esconde o console do Windows no build de release, mas mantém no dev —
// é lá que sai o log de erro interno.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod comandos;
mod erro;
mod estado;

use estado::EstadoApp;
use nucleo::{
    Adaptador, AdaptadorClaude, Banco, Barramento, EventoNucleo, Fabrica, FabricaClaude,
    FabricaFalsa, Orquestrador, Sink,
};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // O banco fica na pasta de dados do app, não junto dos arquivos do
            // usuário: %APPDATA%\app.mutirao.desktop\mutirao.db no Windows.
            let dir = app.path().app_data_dir()?;
            let caminho = dir.join("mutirao.db");
            let banco = Banco::abrir(&caminho).map_err(|e| {
                // Sem banco não há app. Falhar aqui, alto e claro, é melhor
                // que abrir uma janela que não salva nada.
                format!("não consegui abrir o banco em {}: {e}", caminho.display())
            })?;
            println!("[mutirao] banco em {}", caminho.display());

            let banco = Arc::new(Mutex::new(banco));

            // O sink é a única ponte do núcleo para a interface, e a única
            // coisa que ele faz é traduzir variante em nome de evento. Regra
            // de negócio nenhuma passa por aqui.
            let handle = app.handle().clone();
            let sink: Sink = Arc::new(move |evento| emitir(&handle, evento));

            let (fabrica, adaptador, detalhe) = escolher_adaptador();
            println!("[mutirao] adaptador: {} — {detalhe}", adaptador.como_texto());

            let orquestrador = Arc::new(Orquestrador::novo(banco.clone(), fabrica, sink.clone()));

            // O barramento precisa do orquestrador inteiro: da fila de
            // pendências, para o clique do usuário chegar a quem espera, e das
            // ferramentas do §6, que enfileiram turno e levam recado entre nós.
            match Barramento::subir(banco.clone(), orquestrador.clone(), sink) {
                Ok(b) => {
                    println!("[mutirao] barramento em {}", b.url_base());
                    orquestrador.ligar_barramento(b.url_base());
                    // Guardado no estado do app para viver enquanto ele viver.
                    app.manage(b);
                }
                Err(e) => {
                    // Sem barramento não há quem aprove, e sem quem aprove o
                    // adaptador roda somente leitura. Falhar aqui seria pior:
                    // o app ainda serve para conversar e ler.
                    eprintln!("[mutirao] barramento não subiu ({e}); as sessões vão só ler");
                }
            }

            app.manage(EstadoApp::novo(banco, orquestrador, adaptador, detalhe));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            comandos::criar_workspace,
            comandos::listar_workspaces,
            comandos::abrir_workspace,
            comandos::salvar_viewport,
            comandos::criar_no,
            comandos::mover_no,
            comandos::renomear_no,
            comandos::trazer_para_frente,
            comandos::remover_no,
            comandos::criar_cabo,
            comandos::remover_cabo,
            comandos::abrir_sessao,
            comandos::adaptador_em_uso,
            comandos::sessao_do_no,
            comandos::enviar_mensagem,
            comandos::cancelar_turno,
            comandos::historico,
            comandos::acoes_da_sessao,
            comandos::custo_do_workspace,
            comandos::decidir_aprovacao,
            comandos::listar_regras,
            comandos::revogar_regra,
            comandos::aprovacoes_pendentes,
            comandos::listar_pasta,
            comandos::ler_nota,
            comandos::escrever_nota,
            comandos::listar_papeis,
            comandos::criar_papel,
            comandos::editar_papel,
            comandos::remover_papel,
            comandos::quantos_usam_o_papel,
            comandos::definir_papel_do_no,
            comandos::salvar_time,
            comandos::listar_times,
            comandos::abrir_time,
            comandos::remover_time,
            comandos::listar_rascunhos,
            comandos::criar_rascunho,
            comandos::trocar_rascunho,
            comandos::descartar_rascunho,
            comandos::prever_publicacao,
            comandos::publicar_rascunho,
            comandos::definir_mcp_do_papel,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao subir o Mutirão");
}

/// Decide quem responde: o Claude Code instalado, ou o roteiro.
///
/// A regra é procurar a CLI e usá-la; sem ela, cair no falso e **dizer isso**,
/// no log e na barra do app. Cair calado no roteiro seria a pior falha de
/// honestidade possível num programa que cobra por token — o usuário acharia
/// que está conversando com um modelo.
///
/// `MUTIRAO_ADAPTADOR=falso` força o roteiro mesmo com a CLI instalada, que é
/// o que se quer ao mexer na interface sem gastar dinheiro a cada recarga.
fn escolher_adaptador() -> (Arc<dyn Fabrica>, Adaptador, String) {
    let falso = || -> (Arc<dyn Fabrica>, Adaptador, String) {
        (
            Arc::new(FabricaFalsa::demonstracao()),
            Adaptador::Falso,
            "roteiro de demonstração — nenhum token é gasto".to_string(),
        )
    };

    if std::env::var("MUTIRAO_ADAPTADOR").as_deref() == Ok("falso") {
        return falso();
    }

    let claude = FabricaClaude::nova();
    match AdaptadorClaude::detectar(claude.binario()) {
        Ok(versao) => (Arc::new(claude), Adaptador::Claude, versao),
        Err(e) => {
            eprintln!("[mutirao] {e}");
            let (f, a, _) = falso();
            (f, a, format!("Claude Code não encontrado; usando roteiro. {e}"))
        }
    }
}

/// Nomes reservados em `ESPECIFICACAO.md §3`. O payload é o próprio
/// `EventoNucleo` serializado — inclusive o campo `tipo`, que deixa o front
/// discriminar sem depender só do nome do evento.
fn emitir(app: &AppHandle, evento: EventoNucleo) {
    let nome = match &evento {
        EventoNucleo::SessaoEvento { .. } => "sessao:evento",
        EventoNucleo::SessaoEstado { .. } => "sessao:estado",
        EventoNucleo::CustoAtualizado { .. } => "custo:atualizado",
        EventoNucleo::AprovacaoPedida { .. } => "aprovacao:pedida",
        EventoNucleo::AprovacaoDecidida { .. } => "aprovacao:decidida",
        EventoNucleo::NoMensagem { .. } => "no:mensagem",
        EventoNucleo::CadeiaEncerrada { .. } => "cadeia:encerrada",
        EventoNucleo::CanvasMudou { .. } => "canvas:mudou",
    };
    if let Err(e) = app.emit(nome, &evento) {
        // Falhar em avisar não pode derrubar o turno: a resposta já está
        // gravada, e o front relê o histórico quando reabre o nó.
        eprintln!("[mutirao] não consegui emitir {nome}: {e}");
    }
}
