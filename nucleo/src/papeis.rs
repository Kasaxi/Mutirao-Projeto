//! Papéis: o que transforma "um agente" em "o Revisor".
//!
//! Um papel é prompt de sistema + conjunto de ferramentas + autonomia, e
//! opcionalmente um modelo. Sem ele, quatro nós lado a lado são o mesmo
//! programa com nomes diferentes — e quatro iguais não são um time, são uma
//! repetição.
//!
//! ## A escada de autonomia não pula o card
//!
//! [`Autonomia`] escolhe **quais ferramentas o papel enxerga**, nunca se a
//! aprovação aparece. É a distinção que o `ARQUITETURA.md §8` exige: um nível
//! que dispensasse o card seria o "pular todas as permissões" que a §8 proíbe,
//! com outro nome. Um papel `solto` grava com card igual a um `padrao`; ele só
//! alcança mais coisa.
//!
//! Concretamente:
//!
//! | | lê e conversa | grava nota e arquivo | roda comando |
//! |---|---|---|---|
//! | `cauteloso` | sim | não | não |
//! | `padrao` | sim | sim, com card | não |
//! | `solto` | sim | sim, com card | sim, com card **sempre** |
//!
//! O "sempre" da última coluna não é ênfase: `Bash` não entra em
//! `FERRAMENTAS_QUE_ACEITAM_REGRA`, então nem o "não perguntar de novo" o
//! libera. Ver `barramento::aceita_regra`.

use crate::db::Banco;
use crate::erro::Resultado;
use crate::ferramentas;
use crate::modelo::*;

/// Ferramentas nativas do Claude Code liberadas por nível.
///
/// Separadas das do §6 porque são de outro dono: estas vêm com a CLI, aquelas
/// são nossas. Quem junta as duas listas é o adaptador.
pub fn nativas(autonomia: Autonomia) -> &'static [&'static str] {
    match autonomia {
        Autonomia::Cauteloso => &["Read", "Glob", "Grep"],
        Autonomia::Padrao => &["Read", "Glob", "Grep", "Write", "Edit", "NotebookEdit"],
        Autonomia::Solto => {
            &["Read", "Glob", "Grep", "Write", "Edit", "NotebookEdit", "Bash"]
        }
    }
}

/// Ferramentas do §6 liberadas por nível, sem o prefixo do servidor.
///
/// Falar com outro nó, ler e perguntar valem em qualquer nível: são o que faz
/// o nó ser parte de um mutirão em vez de um programa sozinho. O que a escada
/// controla é gravar.
pub fn do_barramento(autonomia: Autonomia) -> Vec<&'static str> {
    let mut v = vec![
        "enviar_para",
        "avisar",
        "listar_nos",
        "ler_nota",
        "listar_arquivos",
        "ler_arquivo",
        "perguntar_humano",
        "concluir",
    ];
    if autonomia != Autonomia::Cauteloso {
        v.push("escrever_nota");
        v.push("escrever_arquivo");
    }
    v
}

/// O conjunto final de ferramentas do §6 para um papel.
///
/// Um papel pode **estreitar** a lista da autonomia (é para isso que serve
/// `ferramentas` no papel), nunca alargá-la. Se pudesse alargar, `cauteloso`
/// deixaria de querer dizer alguma coisa — e um nome que não quer dizer nada é
/// pior que nome nenhum, porque dá confiança sem base.
pub fn ferramentas_do_papel(papel: Option<&Papel>) -> Vec<String> {
    let Some(papel) = papel else {
        // Agente sem papel: tudo que o barramento oferece, como era antes do
        // M4 — menos montar time. Tirar o resto quebraria todo nó criado até
        // aqui; deixar `recrutar` daria a um nó anônimo o poder de encher o
        // canvas, e recrutar é função de papel, não padrão de fábrica.
        return ferramentas::catalogo()
            .iter()
            .filter_map(|f| f.get("name").and_then(|n| n.as_str()))
            .filter(|n| !ferramentas::FERRAMENTAS_DE_TIME.contains(n))
            .map(String::from)
            .collect();
    };

    let mut permitidas = do_barramento(papel.autonomia);
    // As de time não passam pela escada de autonomia — quem as tem é quem as
    // listou. Ver `pode_recrutar`.
    permitidas.extend(ferramentas::FERRAMENTAS_DE_TIME);

    if papel.ferramentas.is_empty() {
        // Lista vazia = "o que a autonomia der", e a autonomia não dá time.
        return do_barramento(papel.autonomia).into_iter().map(String::from).collect();
    }
    papel
        .ferramentas
        .iter()
        .filter(|f| permitidas.contains(&f.as_str()))
        .cloned()
        .collect()
}

/// As nativas de um papel.
///
/// Sem papel, o conjunto **completo** — que é exatamente o que o adaptador
/// oferecia antes de papel existir. Não é descuido: todo nó criado até o M4
/// está sem papel, e estreitar o que eles alcançam seria mudar o workspace de
/// alguém sem ele pedir. Quem escolhe um papel escolhe a escada junto; quem
/// não escolheu continua com o que tinha.
pub fn nativas_do_papel(papel: Option<&Papel>) -> &'static [&'static str] {
    match papel {
        Some(p) => nativas(p.autonomia),
        None => nativas(Autonomia::Solto),
    }
}

/// Este papel pode recrutar? Só quem tem a ferramenta na lista.
///
/// `recrutar` fica **fora** de [`do_barramento`] de propósito: não é uma
/// questão de autonomia, é uma questão de função. Um Revisor `solto` roda
/// comando e ainda assim não monta time; um Organizador `padrao` monta.
pub fn pode_recrutar(papel: Option<&Papel>) -> bool {
    papel.map(|p| p.ferramentas.iter().any(|f| f == "recrutar")).unwrap_or(false)
}

// ------------------------------------------------------------- a biblioteca

/// Os papéis que vêm com o app.
///
/// Escritos para trabalho geral, não para código: o Mutirão é para quem
/// analisa contrato e monta orçamento tanto quanto para quem programa. Os
/// prompts falam de entregar coisa, não de "ser um assistente prestativo".
///
/// Cada um diz o que **não** faz. É a parte que mais economiza turno: um
/// Pesquisador que sabe que não escreve para de tentar e passa a pedir.
pub fn embutidos() -> Vec<(&'static str, &'static str, Vec<&'static str>, Autonomia)> {
    vec![
        (
            "Pesquisador",
            "Você é o Pesquisador de um mutirão de agentes. Seu trabalho é achar e \
             entender material: ler os arquivos e notas a que você tem acesso, extrair \
             o que importa e resumir com precisão.\n\n\
             Você NÃO escreve arquivos nem notas. Quando o resultado precisar ser \
             gravado, mande para quem escreve com enviar_para.\n\n\
             Cite sempre de onde tirou cada afirmação — nome do arquivo, trecho. Se o \
             material não responde à pergunta, diga isso em vez de preencher a lacuna. \
             Quando terminar, chame concluir com um resumo do que achou.",
            vec![],
            Autonomia::Cauteloso,
        ),
        (
            "Redator",
            "Você é o Redator de um mutirão de agentes. Seu trabalho é transformar \
             material bruto em texto que uma pessoa leia sem esforço.\n\n\
             Escreva em português claro, na voz de quem entende do assunto e respeita o \
             tempo de quem lê. Sem enrolação de abertura, sem resumo do que você vai \
             dizer antes de dizer.\n\n\
             Você não inventa fato: se faltar informação, pergunte a quem pesquisou com \
             enviar_para, ou a quem está na frente da tela com perguntar_humano. Toda \
             gravação sua passa pela aprovação da pessoa — escreva como se cada arquivo \
             fosse ser lido antes de salvo, porque vai.",
            vec![],
            Autonomia::Padrao,
        ),
        (
            "Revisor",
            "Você é o Revisor de um mutirão de agentes. Seu trabalho é achar o que está \
             errado antes que chegue a quem pediu.\n\n\
             Leia contra a fonte, não contra a sua memória: confira número com número, \
             data com data, cláusula com cláusula. Aponte o problema, onde ele está e o \
             que fazer — nessa ordem, e sem rodeio.\n\n\
             Você NÃO corrige o texto: quem escreve é o Redator. Mande o que achou com \
             enviar_para. Se estiver tudo certo, diga que está tudo certo — inventar \
             ressalva para parecer útil é o pior que um revisor faz.",
            vec![],
            Autonomia::Cauteloso,
        ),
        (
            "Analista",
            "Você é o Analista de um mutirão de agentes. Seu trabalho é o que exige \
             cálculo e ferramenta: planilha, conversão de formato, conferência de \
             número, extração de dado de arquivo.\n\n\
             Você pode rodar comandos, e cada comando passa pela aprovação da pessoa — \
             sempre, sem exceção e sem 'não perguntar de novo'. Então escreva comandos \
             que uma pessoa consiga ler e aprovar com segurança: um de cada vez, com o \
             que ele faz dito antes.\n\n\
             Mostre a conta, não só o resultado. Um número sem a conta atrás não dá para \
             conferir, e o que não dá para conferir não serve.",
            vec![],
            Autonomia::Solto,
        ),
        (
            "Organizador",
            "Você é o Organizador de um mutirão de agentes. Seu trabalho é montar o time \
             e repartir o trabalho — não fazer o trabalho.\n\n\
             Ao receber uma tarefa: quebre-a em partes, recrute quem falta com recrutar \
             (o papel vem da biblioteca: Pesquisador, Redator, Revisor, Analista), e \
             mande cada parte para o seu com enviar_para. Recrute o mínimo que resolve: \
             cada agente custa dinheiro, e um time grande demais gasta mais tempo se \
             coordenando que trabalhando.\n\n\
             Você não escreve o texto nem faz a pesquisa. Junte o que voltou, confira se \
             responde ao que foi pedido, e entregue com concluir. Se alguém do time \
             travar ou devolver coisa fraca, é seu o trabalho de redistribuir.",
            vec![
                "enviar_para",
                "avisar",
                "listar_nos",
                "ler_nota",
                "escrever_nota",
                "ler_arquivo",
                "listar_arquivos",
                "perguntar_humano",
                "concluir",
                "recrutar",
                "dispensar",
            ],
            Autonomia::Padrao,
        ),
    ]
}

/// Põe a biblioteca no banco, se ainda não estiver lá.
///
/// Idempotente pelo nome: rodar em toda subida do app não duplica nada. Um
/// embutido não dá para apagar (ver [`Banco::remover_papel`]), então ele não
/// volta sozinho depois de o usuário tirá-lo — ele nunca sai.
pub fn semear(banco: &Banco) -> Resultado<usize> {
    let mut novos = 0;
    for (nome, prompt, ferramentas, autonomia) in embutidos() {
        if banco.papel_por_nome(nome)?.is_some() {
            continue;
        }
        let ferramentas: Vec<String> = ferramentas.into_iter().map(String::from).collect();
        banco.criar_papel(nome, prompt, &ferramentas, autonomia, None, true)?;
        novos += 1;
    }
    Ok(novos)
}
