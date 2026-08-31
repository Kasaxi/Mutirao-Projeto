use crate::erro::{ErroIpc, ResultadoIpc};
use crate::estado::EstadoApp;
use nucleo::modelo::*;
use nucleo::ItemArquivo;
use tauri::State;

// Todo comando segue o mesmo desenho:
//   - nome em português, verbo primeiro, igual ao contrato em ESPECIFICACAO.md
//   - devolve o objeto criado/alterado inteiro, para o front não precisar reler
//   - erro sempre como ErroIpc { codigo, mensagem }
//
// Nada de lógica aqui: isto é casca. Regra é no crate `nucleo`.

// ------------------------------------------------------------- workspace

/// `pasta` vazia quer dizer "escolha por mim". Resolver isso é trabalho da
/// casca, não do núcleo: onde ficam os documentos de alguém é pergunta do
/// sistema operacional, e o núcleo não conhece nenhum.
#[tauri::command]
pub fn criar_workspace(
    app: tauri::AppHandle,
    estado: State<EstadoApp>,
    nome: String,
    pasta: String,
) -> ResultadoIpc<Workspace> {
    let pasta = if pasta.trim().is_empty() {
        let raiz = tauri::Manager::path(&app)
            .document_dir()
            .or_else(|_| tauri::Manager::path(&app).home_dir())
            .map_err(|_| nucleo::Erro::invalido("não achei onde criar a pasta do workspace"))?;
        let alvo = raiz.join("Mutirão").join(nome_de_pasta(&nome));
        std::fs::create_dir_all(&alvo).map_err(nucleo::Erro::from)?;
        alvo.to_string_lossy().to_string()
    } else {
        std::fs::create_dir_all(&pasta).map_err(nucleo::Erro::from)?;
        pasta
    };
    let banco = estado.banco()?;
    let ws = banco.criar_workspace(&nome, &pasta)?;

    // O histórico oculto fica na pasta de dados do app, **fora** da pasta do
    // usuário — ver `nucleo/src/git.rs`. Onde ficam os dados de um app é
    // pergunta do sistema operacional, então quem responde é a casca.
    if let Ok(dados) = tauri::Manager::path(&app).app_data_dir() {
        let repo = dados.join("historicos").join(&ws.id);
        banco.definir_repo(&ws.id, &repo.to_string_lossy())?;
    }
    // Falhar aqui não derruba a criação: sem histórico o workspace continua
    // servindo para tudo menos rascunho, e é isso que o app diz na tela.
    if let Err(e) = nucleo::ensaios::preparar(&banco, &ws.id) {
        eprintln!("[mutirao] sem histórico para {}: {e}", ws.nome);
    }
    Ok(banco.obter_workspace(&ws.id)?)
    // NOTA: o `abrir_workspace` faz o mesmo para workspace de antes do M5.
}

/// O nome do workspace virando nome de pasta. Mesmo espírito de
/// `arquivos::arquivo_da_nota`: tira o que o Windows recusa, mantém acento.
fn nome_de_pasta(nome: &str) -> String {
    let limpo: String = nome
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let limpo = limpo.trim().trim_matches('.').trim().to_string();
    if limpo.is_empty() {
        "Mutirão".to_string()
    } else {
        limpo
    }
}

#[tauri::command]
pub fn listar_workspaces(estado: State<EstadoApp>) -> ResultadoIpc<Vec<Workspace>> {
    Ok(estado.banco()?.listar_workspaces()?)
}

#[tauri::command]
pub fn abrir_workspace(
    app: tauri::AppHandle,
    estado: State<EstadoApp>,
    workspace_id: String,
) -> ResultadoIpc<EstadoCanvas> {
    let banco = estado.banco()?;
    // Workspace do M0 ao M4 não tem repositório: ganha um aqui, na primeira
    // abertura depois da atualização. Sem isto, quem já usava o app nunca
    // teria rascunho e não saberia por quê.
    if banco.obter_workspace(&workspace_id)?.repo.is_none() {
        if let Ok(dados) = tauri::Manager::path(&app).app_data_dir() {
            let repo = dados.join("historicos").join(&workspace_id);
            banco.definir_repo(&workspace_id, &repo.to_string_lossy())?;
        }
    }
    // Idempotente: um repositório que já existe não é tocado.
    if let Err(e) = nucleo::ensaios::preparar(&banco, &workspace_id) {
        eprintln!("[mutirao] sem histórico para {workspace_id}: {e}");
    }
    Ok(banco.estado_canvas(&workspace_id)?)
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

// --------------------------------------------------------------- sessões

/// Abre a face conversa de um nó. Devolve a sessão existente se já houver —
/// reabrir o app continua a conversa, não começa outra.
#[tauri::command]
pub fn abrir_sessao(estado: State<EstadoApp>, node_id: String) -> ResultadoIpc<Sessao> {
    // O adaptador não vem do front: quem sabe qual está disponível é quem
    // procurou a CLI na máquina, na subida do app.
    Ok(estado.orquestrador().abrir_sessao(&node_id, estado.adaptador())?)
}

/// Qual agente está de fato respondendo, e o que dizer sobre ele. A interface
/// mostra isso na barra — um app que conversa com um roteiro e não avisa está
/// mentindo para quem o usa.
#[tauri::command]
pub fn adaptador_em_uso(estado: State<EstadoApp>) -> ResultadoIpc<AdaptadorEmUso> {
    Ok(AdaptadorEmUso {
        adaptador: estado.adaptador(),
        detalhe: estado.detalhe_do_adaptador().to_string(),
    })
}

#[derive(serde::Serialize)]
pub struct AdaptadorEmUso {
    pub adaptador: Adaptador,
    pub detalhe: String,
}

// -------------------------------------------------------------- arquivos

/// A árvore de arquivos do nó `arquivos`, lida do disco de verdade.
#[tauri::command]
pub fn listar_pasta(
    estado: State<EstadoApp>,
    workspace_id: String,
    sub: String,
) -> ResultadoIpc<Vec<ItemArquivo>> {
    let pasta = estado.banco()?.pasta_de_trabalho(&workspace_id)?;
    Ok(nucleo::arquivos::listar(std::path::Path::new(&pasta), &sub)?)
}

/// Lê a nota de um nó. A nota é um `.md` na pasta do workspace — o usuário
/// abre no editor dele, manda por e-mail, versiona. É a diferença entre
/// "memória do app" e "arquivo meu".
#[tauri::command]
pub fn ler_nota(estado: State<EstadoApp>, node_id: String) -> ResultadoIpc<Nota> {
    let banco = estado.banco()?;
    let (no, pasta, arquivo) = nota_do_no(&banco, &node_id)?;
    let caminho = std::path::Path::new(&pasta);
    // Nota que ainda não tem arquivo não é erro: é uma nota em branco.
    let conteudo = match nucleo::arquivos::ler_texto(caminho, &arquivo) {
        Ok(c) => c,
        Err(_) if !caminho.join(&arquivo).exists() => String::new(),
        Err(e) => return Err(e.into()),
    };
    let _ = no;
    Ok(Nota { arquivo, conteudo })
}

#[tauri::command]
pub fn escrever_nota(
    estado: State<EstadoApp>,
    node_id: String,
    conteudo: String,
) -> ResultadoIpc<()> {
    let banco = estado.banco()?;
    let (_, pasta, arquivo) = nota_do_no(&banco, &node_id)?;
    nucleo::arquivos::escrever_texto(std::path::Path::new(&pasta), &arquivo, &conteudo)?;
    Ok(())
}

#[derive(serde::Serialize)]
pub struct Nota {
    pub arquivo: String,
    pub conteudo: String,
}

/// Resolve nó → pasta do workspace → arquivo da nota, fixando o arquivo na
/// config do nó na primeira vez. Depois disso, renomear o nó não muda mais o
/// arquivo — senão o `.md` anterior ficaria órfão na pasta do usuário.
fn nota_do_no(
    banco: &nucleo::Banco,
    node_id: &str,
) -> Result<(nucleo::No, String, String), ErroIpc> {
    let no = banco.obter_no(node_id)?;
    if no.tipo != TipoNo::Nota {
        return Err(nucleo::Erro::invalido("esse nó não é uma nota").into());
    }
    let pasta = banco.pasta_de_trabalho(&no.workspace_id)?;
    let arquivo = nucleo::arquivos::arquivo_da_nota_do_no(&no);
    if no.config.get("arquivo").is_none() {
        let mut config = no.config.clone();
        config["arquivo"] = serde_json::Value::String(arquivo.clone());
        banco.definir_config_no(node_id, &config)?;
    }
    Ok((no, pasta, arquivo))
}

// ------------------------------------------------------------- aprovação

/// O usuário clicou em Aprovar ou Negar. `lembrar` é a caixa "não perguntar de
/// novo nesta pasta" — só vale para aprovação, e só para ferramenta que grava.
#[tauri::command]
pub fn decidir_aprovacao(
    estado: State<EstadoApp>,
    tool_call_id: String,
    decisao: Decisao,
    lembrar: bool,
) -> ResultadoIpc<()> {
    Ok(estado.orquestrador().decidir_aprovacao(&tool_call_id, decisao, lembrar)?)
}

/// As permissões permanentes deste workspace. Toda permissão concedida precisa
/// ser visível e revogável depois — `ARQUITETURA.md §8`.
#[tauri::command]
pub fn listar_regras(
    estado: State<EstadoApp>,
    workspace_id: String,
) -> ResultadoIpc<Vec<RegraAprovacao>> {
    Ok(estado.banco()?.listar_regras(&workspace_id)?)
}

#[tauri::command]
pub fn revogar_regra(estado: State<EstadoApp>, id: String) -> ResultadoIpc<()> {
    Ok(estado.banco()?.revogar_regra(&id)?)
}

/// Aprovações ainda esperando alguém, para a interface reabrir os cards ao
/// voltar para um nó — um evento perdido não pode deixar o agente parado
/// para sempre sem nada na tela.
#[tauri::command]
pub fn aprovacoes_pendentes(
    estado: State<EstadoApp>,
    session_id: String,
) -> ResultadoIpc<Vec<PedidoAprovacao>> {
    let banco = estado.banco()?;
    let sessao = banco.obter_sessao(&session_id)?;
    let pendentes = banco
        .ferramentas_da_sessao(&session_id)?
        .into_iter()
        .filter(|c| c.aprovacao == Aprovacao::Pendente)
        .map(|c| {
            // Mesma descrição que o card recebeu pelo evento: quem traduz
            // "Write com file_path" para "Gravar orçamento.xlsx" é o núcleo,
            // e só ele — senão o card reaberto conta outra história.
            let (resumo, detalhe) = descrever_ferramenta(&c.ferramenta, &c.argumentos);
            PedidoAprovacao {
                tool_call_id: c.id,
                session_id: c.session_id,
                node_id: sessao.node_id.clone(),
                ferramenta: c.ferramenta,
                resumo,
                detalhe,
                previa: c
                    .argumentos
                    .get("content")
                    .or_else(|| c.argumentos.get("new_string"))
                    .or_else(|| c.argumentos.get("command"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.chars().take(600).collect()),
                criado_em: c.criado_em,
            }
        })
        .collect();
    Ok(pendentes)
}

#[tauri::command]
pub fn sessao_do_no(estado: State<EstadoApp>, node_id: String) -> ResultadoIpc<Option<Sessao>> {
    Ok(estado.banco()?.sessao_do_no(&node_id)?)
}

/// Manda um turno. Volta assim que o turno começa: a resposta chega pelos
/// eventos `sessao:evento` e `sessao:estado`, não por este retorno.
#[tauri::command]
pub fn enviar_mensagem(
    estado: State<EstadoApp>,
    session_id: String,
    texto: String,
) -> ResultadoIpc<()> {
    Ok(estado.orquestrador().enviar(&session_id, &texto)?)
}

#[tauri::command]
pub fn cancelar_turno(estado: State<EstadoApp>, session_id: String) -> ResultadoIpc<()> {
    Ok(estado.orquestrador().cancelar(&session_id)?)
}

#[tauri::command]
pub fn historico(
    estado: State<EstadoApp>,
    session_id: String,
    limite: i64,
) -> ResultadoIpc<Vec<Mensagem>> {
    Ok(estado.banco()?.historico(&session_id, limite)?)
}

/// As ações do agente, que na face conversa viram cards em vez de texto de log.
#[tauri::command]
pub fn acoes_da_sessao(
    estado: State<EstadoApp>,
    session_id: String,
) -> ResultadoIpc<Vec<ChamadaFerramenta>> {
    Ok(estado.banco()?.ferramentas_da_sessao(&session_id)?)
}

#[tauri::command]
pub fn custo_do_workspace(
    estado: State<EstadoApp>,
    workspace_id: String,
) -> ResultadoIpc<CustoWorkspace> {
    let (total, por_no) = estado.banco()?.custo_do_workspace(&workspace_id)?;
    Ok(CustoWorkspace { total, por_no })
}

/// Tupla não atravessa fronteira com nome; um objeto, sim.
#[derive(serde::Serialize)]
pub struct CustoWorkspace {
    pub total: f64,
    pub por_no: Vec<CustoDoNo>,
}

// ================================================================== M4 =====
// Papéis e times. A casca continua casca: cada comando é uma linha de despacho
// para o núcleo, e a regra mora lá.

/// Os papéis, **sem os segredos**. A chave do servidor externo de alguém não
/// atravessa a fronteira: o que chega ao front chega a tudo que roda no front.
/// Mesma regra do token da sessão — ver `ESPECIFICACAO.md §4`.
#[tauri::command]
pub fn listar_papeis(estado: State<EstadoApp>) -> ResultadoIpc<Vec<Papel>> {
    Ok(estado
        .banco()?
        .listar_papeis()?
        .into_iter()
        .map(Papel::sem_segredos)
        .collect())
}

#[tauri::command]
pub fn criar_papel(
    estado: State<EstadoApp>,
    nome: String,
    prompt: String,
    ferramentas: Vec<String>,
    autonomia: Autonomia,
    modelo: Option<String>,
) -> ResultadoIpc<Papel> {
    Ok(estado.banco()?.criar_papel(
        &nome,
        &prompt,
        &ferramentas,
        autonomia,
        modelo.as_deref(),
        // Só o app cria embutido. Um papel feito pela pessoa é dela, e
        // portanto apagável.
        false,
    )?)
}

#[tauri::command]
pub fn editar_papel(
    estado: State<EstadoApp>,
    id: String,
    prompt: String,
    ferramentas: Vec<String>,
    autonomia: Autonomia,
    modelo: Option<String>,
) -> ResultadoIpc<Papel> {
    Ok(estado.banco()?.editar_papel(
        &id,
        &prompt,
        &ferramentas,
        autonomia,
        modelo.as_deref(),
    )?)
}

#[tauri::command]
pub fn remover_papel(estado: State<EstadoApp>, id: String) -> ResultadoIpc<()> {
    Ok(estado.banco()?.remover_papel(&id)?)
}

/// Quantos nós usam este papel. O front mostra antes de apagar, para o clique
/// não ser às cegas.
#[tauri::command]
pub fn quantos_usam_o_papel(estado: State<EstadoApp>, id: String) -> ResultadoIpc<usize> {
    Ok(estado.banco()?.quantos_usam_o_papel(&id)?)
}

/// `role_id` nulo tira o papel do nó, e é assim de propósito: um nó pode voltar
/// a ser um agente sem papel, que é o que ele era antes do M4.
#[tauri::command]
pub fn definir_papel_do_no(
    estado: State<EstadoApp>,
    node_id: String,
    role_id: Option<String>,
) -> ResultadoIpc<No> {
    let banco = estado.banco()?;
    banco.definir_papel_do_no(&node_id, role_id.as_deref())?;
    Ok(banco.obter_no(&node_id)?)
}

// ------------------------------------------------------------------ times

#[tauri::command]
pub fn salvar_time(
    estado: State<EstadoApp>,
    workspace_id: String,
    nome: String,
) -> ResultadoIpc<Partitura> {
    let banco = estado.banco()?;
    let snapshot = nucleo::partituras::fotografar(&banco, &workspace_id)?;
    Ok(banco.salvar_partitura(&workspace_id, &nome, &snapshot)?)
}

#[tauri::command]
pub fn listar_times(
    estado: State<EstadoApp>,
    workspace_id: String,
) -> ResultadoIpc<Vec<Partitura>> {
    Ok(estado.banco()?.listar_partituras(&workspace_id)?)
}

/// Abre um time salvo **no workspace pedido**, que não é necessariamente o de
/// onde ele foi salvo: é isso que faz uma partitura ser reutilizável em vez de
/// um botão de desfazer.
#[tauri::command]
pub fn abrir_time(
    estado: State<EstadoApp>,
    workspace_id: String,
    partitura_id: String,
) -> ResultadoIpc<Vec<No>> {
    let banco = estado.banco()?;
    let partitura = banco.obter_partitura(&partitura_id)?;
    Ok(nucleo::partituras::montar(&banco, &workspace_id, &partitura)?)
}

#[tauri::command]
pub fn remover_time(estado: State<EstadoApp>, id: String) -> ResultadoIpc<()> {
    Ok(estado.banco()?.remover_partitura(&id)?)
}

// ================================================================== M5 =====
// Rascunhos. Nenhuma palavra de Git chega ao front — nem nos nomes dos
// comandos, nem nos campos que eles devolvem.

#[tauri::command]
pub fn listar_rascunhos(
    estado: State<EstadoApp>,
    workspace_id: String,
) -> ResultadoIpc<Vec<Ensaio>> {
    Ok(estado.banco()?.listar_ensaios(&workspace_id)?)
}

#[tauri::command]
pub fn criar_rascunho(
    estado: State<EstadoApp>,
    workspace_id: String,
    nome: String,
) -> ResultadoIpc<Ensaio> {
    let banco = estado.banco()?;
    Ok(nucleo::ensaios::criar(&banco, &workspace_id, &nome)?)
}

/// Troca o rascunho em foco. `ensaioId` nulo volta para a pasta de verdade.
///
/// Passa pelo `ensaios::trocar`, e não pelo banco direto, porque trocar
/// **precisa** derrubar os adaptadores vivos antes — senão um agente já aberto
/// continua gravando na pasta anterior.
#[tauri::command]
pub fn trocar_rascunho(
    estado: State<EstadoApp>,
    workspace_id: String,
    ensaio_id: Option<String>,
) -> ResultadoIpc<()> {
    let banco = estado.banco()?;
    Ok(nucleo::ensaios::trocar(
        &banco,
        estado.orquestrador(),
        &workspace_id,
        ensaio_id.as_deref(),
    )?)
}

#[tauri::command]
pub fn descartar_rascunho(estado: State<EstadoApp>, ensaio_id: String) -> ResultadoIpc<()> {
    let banco = estado.banco()?;
    Ok(nucleo::ensaios::descartar(&banco, estado.orquestrador(), &ensaio_id)?)
}

/// O que muda ao publicar. Não escreve nada — é o que a tela mostra antes do
/// clique.
#[tauri::command]
pub fn prever_publicacao(
    estado: State<EstadoApp>,
    ensaio_id: String,
) -> ResultadoIpc<PreviaPublicacao> {
    let banco = estado.banco()?;
    Ok(nucleo::ensaios::prever(&banco, &ensaio_id)?)
}

#[tauri::command]
pub fn publicar_rascunho(
    estado: State<EstadoApp>,
    ensaio_id: String,
    escolhas: Vec<(String, LadoDoConflito)>,
) -> ResultadoIpc<PreviaPublicacao> {
    let banco = estado.banco()?;
    Ok(nucleo::ensaios::publicar(&banco, estado.orquestrador(), &ensaio_id, &escolhas)?)
}

/// Servidores MCP externos de um papel. Os cabeçalhos vêm do front porque é lá
/// que o usuário os digita, e vão direto para o banco sem voltar.
#[tauri::command]
pub fn definir_mcp_do_papel(
    estado: State<EstadoApp>,
    id: String,
    servidores: Vec<ServidorMcp>,
) -> ResultadoIpc<Papel> {
    Ok(estado.banco()?.definir_mcp_do_papel(&id, &servidores)?.sem_segredos())
}
