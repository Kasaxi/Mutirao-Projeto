//! Partituras: o time salvo para reabrir amanhã.
//!
//! ## O que uma partitura NÃO é
//!
//! Não é backup. Ela guarda **quem trabalha e como está ligado** — nós, papéis,
//! cabos, posições — e não guarda conversa, sessão, custo nem arquivo. Reabrir
//! monta o mesmo time pronto para trabalhar de novo; não ressuscita o que já
//! foi dito.
//!
//! Essa fronteira decide o resto do módulo. É por ela que [`NoSalvo`] não tem
//! id (o id pertence ao canvas onde o nó vive, e reabrir cria nós novos) e que
//! o papel é gravado **pelo nome** (uma partitura precisa abrir noutra máquina,
//! onde o mesmo papel tem outro id).
//!
//! É também o que torna "reabrir" seguro: como nada é sobrescrito, abrir duas
//! vezes dá dois times, e não um time corrompido pela metade.

use crate::db::Banco;
use crate::erro::{Erro, Resultado};
use crate::modelo::*;
use std::collections::HashMap;

/// Fotografa o time de um workspace.
///
/// Leva todos os nós, não só os de agente: uma nota compartilhada e a pasta de
/// arquivos fazem parte de como o time trabalha. O que fica de fora é o que
/// pertence à execução daquele dia.
pub fn fotografar(banco: &Banco, workspace_id: &str) -> Resultado<Snapshot> {
    let nos = banco.listar_nos(workspace_id)?;
    let cabos = banco.listar_cabos(workspace_id)?;

    // Índice por id para os cabos poderem virar pares de posição no vetor.
    let posicao: HashMap<&str, usize> =
        nos.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();

    let salvos = nos
        .iter()
        .map(|n| {
            let papel = n
                .role_id
                .as_deref()
                .and_then(|id| banco.obter_papel(id).ok())
                .map(|p| p.nome);
            NoSalvo {
                tipo: n.tipo,
                nome: n.nome.clone(),
                x: n.x,
                y: n.y,
                w: n.w,
                h: n.h,
                config: n.config.clone(),
                papel,
            }
        })
        .collect();

    let cabos = cabos
        .iter()
        .filter_map(|c| {
            // Cabo cuja ponta não está no snapshot não vai junto. Não deveria
            // acontecer — os dois vieram da mesma consulta —, mas gravar um
            // índice inventado transformaria um cabo perdido num nó errado.
            Some(CaboSalvo {
                de: *posicao.get(c.de_node.as_str())?,
                para: *posicao.get(c.para_node.as_str())?,
                tipo: c.tipo,
            })
        })
        .collect();

    Ok(Snapshot { nos: salvos, cabos })
}

/// Monta o time de volta no workspace, e devolve os nós criados.
///
/// Nada é sobrescrito: os nós são novos, com ids novos. Se já houver alguém
/// com o mesmo nome, o novo ganha um sufixo — renomear é chato, mas dois
/// "Redator" quebram o `enviar_para`, que resolve vizinho pelo nome.
pub fn montar(banco: &Banco, workspace_id: &str, partitura: &Partitura) -> Resultado<Vec<No>> {
    if partitura.snapshot.nos.is_empty() {
        return Err(Erro::invalido("este time salvo está vazio"));
    }

    let existentes = banco.listar_nos(workspace_id)?;
    let quantos_agentes = existentes.iter().filter(|n| n.tipo == TipoNo::Agente).count();
    let agentes_no_time =
        partitura.snapshot.nos.iter().filter(|n| n.tipo == TipoNo::Agente).count();
    if quantos_agentes + agentes_no_time > MAX_AGENTES_POR_WORKSPACE {
        return Err(Erro::invalido(format!(
            "abrir este time passaria de {MAX_AGENTES_POR_WORKSPACE} agentes no workspace. \
             Apague alguém antes, ou abra em outro workspace."
        )));
    }

    // Deslocamento: o time reaberto não pode cair exatamente em cima do que já
    // está lá. Empurrar para a direita do que existe é previsível e sempre
    // funciona; procurar buraco vazio ficaria mais bonito e menos explicável.
    let (dx, dy) = deslocamento(&existentes, &partitura.snapshot);

    let mut usados: Vec<String> =
        existentes.iter().map(|n| n.nome.trim().to_lowercase()).collect();
    let mut criados: Vec<No> = Vec::with_capacity(partitura.snapshot.nos.len());

    for salvo in &partitura.snapshot.nos {
        let nome = nome_livre(&salvo.nome, &usados);
        usados.push(nome.trim().to_lowercase());

        let role_id = match &salvo.papel {
            // Papel que não existe nesta máquina não impede o time de abrir: o
            // nó nasce sem papel e a pessoa escolhe um. Recusar a partitura
            // inteira por causa de um papel apagado seria perder o time todo
            // por um detalhe que se conserta em dois cliques.
            Some(p) => banco.papel_por_nome(p)?.map(|p| p.id),
            None => None,
        };

        let no = banco.criar_no_recrutado(
            workspace_id,
            salvo.tipo,
            &nome,
            salvo.x + dx,
            salvo.y + dy,
            role_id.as_deref(),
            None,
        )?;
        // Tamanho e config vêm do que foi salvo, não do padrão do tipo: uma
        // nota redimensionada e um arquivo apontado fazem parte do time.
        banco.mover_no(&no.id, no.x, no.y, salvo.w, salvo.h)?;
        if salvo.config != serde_json::json!({}) {
            banco.definir_config_no(&no.id, &salvo.config)?;
        }
        criados.push(banco.obter_no(&no.id)?);
    }

    for cabo in &partitura.snapshot.cabos {
        let (Some(de), Some(para)) = (criados.get(cabo.de), criados.get(cabo.para)) else {
            continue; // índice fora do vetor: snapshot editado à mão
        };
        // Um cabo repetido não derruba a montagem do resto.
        let _ = banco.criar_cabo(workspace_id, &de.id, &para.id, cabo.tipo);
    }

    Ok(criados)
}

/// Para onde empurrar o time reaberto.
///
/// Para a direita de tudo que já existe, alinhado pelo topo do que foi salvo.
/// Workspace vazio não desloca nada — abrir um time num canvas limpo tem de
/// devolver as posições que foram salvas.
fn deslocamento(existentes: &[No], snapshot: &Snapshot) -> (f64, f64) {
    if existentes.is_empty() {
        return (0.0, 0.0);
    }
    const VAO: f64 = 80.0;
    let direita = existentes.iter().map(|n| n.x + n.w).fold(f64::MIN, f64::max);
    let esquerda_do_time = snapshot.nos.iter().map(|n| n.x).fold(f64::MAX, f64::min);
    (direita + VAO - esquerda_do_time, 0.0)
}

/// Um nome que ainda não está em uso, comparando sem caixa.
///
/// "Redator" vira "Redator 2", e não "Redator (1)": o nome vai para dentro de
/// um `enviar_para` escrito por um modelo, e parêntese é convite a erro de
/// digitação.
fn nome_livre(desejado: &str, usados: &[String]) -> String {
    let base = desejado.trim();
    if !usados.contains(&base.to_lowercase()) {
        return base.to_string();
    }
    for n in 2..1000 {
        let tentativa = format!("{base} {n}");
        if !usados.contains(&tentativa.to_lowercase()) {
            return tentativa;
        }
    }
    // Mil "Redator" no mesmo canvas é problema de outra natureza.
    format!("{base} {}", &novo_id()[..4])
}
