// Esconde o console do Windows no build de release, mas mantém no dev —
// é lá que sai o log de erro interno.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod comandos;
mod erro;
mod estado;

use estado::EstadoApp;
use nucleo::Banco;
use tauri::Manager;

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
            app.manage(EstadoApp::novo(banco));
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
        ])
        .run(tauri::generate_context!())
        .expect("erro ao subir o Mutirão");
}
