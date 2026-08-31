// Esconde o console do Windows no build de release, mas mantém no dev —
// é lá que sai o log de erro interno.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod comandos;
mod erro;
mod estado;

use estado::EstadoApp;
use nucleo::{Banco, EventoNucleo, Fabrica, FabricaFalsa, Orquestrador, Sink};
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

            // M1 roda o adaptador falso: roteiro em vez de modelo. O adaptador
            // Claude é a próxima peça. Até lá, o app conversa consigo mesmo —
            // e diz isso na barra de cima, porque uma maquete que não se anuncia
            // é uma mentira.
            let fabrica: Arc<dyn Fabrica> = Arc::new(FabricaFalsa::demonstracao());
            println!("[mutirao] adaptador: falso (roteiro de demonstração)");

            let orquestrador = Arc::new(Orquestrador::novo(banco.clone(), fabrica, sink));
            app.manage(EstadoApp::novo(banco, orquestrador));
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
            comandos::sessao_do_no,
            comandos::enviar_mensagem,
            comandos::cancelar_turno,
            comandos::historico,
            comandos::acoes_da_sessao,
            comandos::custo_do_workspace,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao subir o Mutirão");
}

/// Nomes reservados em `ESPECIFICACAO.md §3`. O payload é o próprio
/// `EventoNucleo` serializado — inclusive o campo `tipo`, que deixa o front
/// discriminar sem depender só do nome do evento.
fn emitir(app: &AppHandle, evento: EventoNucleo) {
    let nome = match &evento {
        EventoNucleo::SessaoEvento { .. } => "sessao:evento",
        EventoNucleo::SessaoEstado { .. } => "sessao:estado",
        EventoNucleo::CustoAtualizado { .. } => "custo:atualizado",
    };
    if let Err(e) = app.emit(nome, &evento) {
        // Falhar em avisar não pode derrubar o turno: a resposta já está
        // gravada, e o front relê o histórico quando reabre o nó.
        eprintln!("[mutirao] não consegui emitir {nome}: {e}");
    }
}
