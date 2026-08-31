use crate::erro::ResultadoIpc;
use crate::estado::EstadoApp;
use nucleo::modelo::*;
use tauri::State;

// Todo comando segue o mesmo desenho:
//   - nome em português, verbo primeiro, igual ao contrato em ESPECIFICACAO.md
//   - devolve o objeto criado/alterado inteiro, para o front não precisar reler
//   - erro sempre como ErroIpc { codigo, mensagem }
//
// Nada de lógica aqui: isto é casca. Regra é no crate `nucleo`.

// ------------------------------------------------------------- workspace

#[tauri::command]
pub fn criar_workspace(
    estado: State<EstadoApp>,
    nome: String,
    pasta: String,
) -> ResultadoIpc<Workspace> {
    Ok(estado.banco()?.criar_workspace(&nome, &pasta)?)
}

#[tauri::command]
pub fn listar_workspaces(estado: State<EstadoApp>) -> ResultadoIpc<Vec<Workspace>> {
    Ok(estado.banco()?.listar_workspaces()?)
}

#[tauri::command]
pub fn abrir_workspace(
    estado: State<EstadoApp>,
    workspace_id: String,
) -> ResultadoIpc<EstadoCanvas> {
    Ok(estado.banco()?.estado_canvas(&workspace_id)?)
}

#[tauri::command]
pub fn salvar_viewport(
    estado: State<EstadoApp>,
    workspace_id: String,
    x: f64,
    y: f64,
    zoom: f64,
) -> ResultadoIpc<()> {
    Ok(estado.banco()?.salvar_viewport(&workspace_id, Viewport { x, y, zoom })?)
}

// -------------------------------------------------------------------- nós

#[tauri::command]
pub fn criar_no(
    estado: State<EstadoApp>,
    workspace_id: String,
    tipo: TipoNo,
    nome: String,
    x: f64,
    y: f64,
) -> ResultadoIpc<No> {
    Ok(estado.banco()?.criar_no(&workspace_id, tipo, &nome, x, y)?)
}

/// Chamado no fim do arrasto, não a cada frame.
#[tauri::command]
pub fn mover_no(
    estado: State<EstadoApp>,
    id: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> ResultadoIpc<()> {
    Ok(estado.banco()?.mover_no(&id, x, y, w, h)?)
}

#[tauri::command]
pub fn renomear_no(estado: State<EstadoApp>, id: String, nome: String) -> ResultadoIpc<()> {
    Ok(estado.banco()?.renomear_no(&id, &nome)?)
}

#[tauri::command]
pub fn trazer_para_frente(estado: State<EstadoApp>, id: String) -> ResultadoIpc<i64> {
    Ok(estado.banco()?.trazer_para_frente(&id)?)
}

#[tauri::command]
pub fn remover_no(estado: State<EstadoApp>, id: String) -> ResultadoIpc<()> {
    Ok(estado.banco()?.remover_no(&id)?)
}

// ----------------------------------------------------------------- cabos

#[tauri::command]
pub fn criar_cabo(
    estado: State<EstadoApp>,
    workspace_id: String,
    de_node: String,
    para_node: String,
    tipo: TipoCabo,
) -> ResultadoIpc<Cabo> {
    Ok(estado.banco()?.criar_cabo(&workspace_id, &de_node, &para_node, tipo)?)
}

#[tauri::command]
pub fn remover_cabo(estado: State<EstadoApp>, id: String) -> ResultadoIpc<()> {
    Ok(estado.banco()?.remover_cabo(&id)?)
}
