import type {
  Adaptador,
  Autonomia,
  Cabo,
  Ensaio,
  ChamadaFerramenta,
  CustoDoNo,
  Decisao,
  EstadoCanvas,
  EstadoSessao,
  EventoAgente,
  ItemArquivo,
  Mensagem,
  No,
  Nota,
  PapelMensagem,
  LadoDoConflito,
  Papel,
  Partitura,
  PreviaPublicacao,
  ServidorMcp,
  PedidoAprovacao,
  RegraAprovacao,
  Sessao,
  TipoCabo,
  TipoNo,
  Workspace,
} from "./tipos";
import {
  FERRAMENTAS_MCP_QUE_GRAVAM,
  FERRAMENTAS_QUE_ACEITAM_REGRA,
  nomeCompletoMcp,
} from "./tipos";

// Camada única de acesso ao núcleo. Nenhum componente chama `invoke` direto:
// assim o contrato IPC fica num arquivo só, e o modo navegador existe.
//
// MODO NAVEGADOR: rodando `npm run dev` fora do Tauri não há backend. Em vez
// de quebrar, cai num núcleo falso em memória com as mesmas regras. Serve para
// desenvolver interface rápido e para tirar screenshot em CI. Nada é salvo.

const dentroDoTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function chamar<T>(comando: string, args?: Record<string, unknown>): Promise<T> {
  if (!dentroDoTauri) return falso<T>(comando, args ?? {});
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(comando, args);
}

export const ipc = {
  criarWorkspace: (nome: string, pasta: string) =>
    chamar<Workspace>("criar_workspace", { nome, pasta }),

  listarWorkspaces: () => chamar<Workspace[]>("listar_workspaces"),

  abrirWorkspace: (workspaceId: string) =>
    chamar<EstadoCanvas>("abrir_workspace", { workspaceId }),

  salvarViewport: (workspaceId: string, x: number, y: number, zoom: number) =>
    chamar<void>("salvar_viewport", { workspaceId, x, y, zoom }),

  criarNo: (workspaceId: string, tipo: TipoNo, nome: string, x: number, y: number) =>
    chamar<No>("criar_no", { workspaceId, tipo, nome, x, y }),

  moverNo: (id: string, x: number, y: number, w: number, h: number) =>
    chamar<void>("mover_no", { id, x, y, w, h }),

  renomearNo: (id: string, nome: string) => chamar<void>("renomear_no", { id, nome }),

  trazerParaFrente: (id: string) => chamar<number>("trazer_para_frente", { id }),

  removerNo: (id: string) => chamar<void>("remover_no", { id }),

  criarCabo: (workspaceId: string, deNode: string, paraNode: string, tipo: TipoCabo) =>
    chamar<Cabo>("criar_cabo", { workspaceId, deNode, paraNode, tipo }),

  removerCabo: (id: string) => chamar<void>("remover_cabo", { id }),

  // ------------------------------------------------------------- sessões

  // Sem `adaptador`: quem decide qual agente responde é o backend, que é quem
  // procurou a CLI na máquina. Um front que escolhe isso acaba mentindo.
  abrirSessao: (nodeId: string) => chamar<Sessao>("abrir_sessao", { nodeId }),

  adaptadorEmUso: () =>
    chamar<{ adaptador: Adaptador; detalhe: string }>("adaptador_em_uso"),

  sessaoDoNo: (nodeId: string) => chamar<Sessao | null>("sessao_do_no", { nodeId }),

  enviarMensagem: (sessionId: string, texto: string) =>
    chamar<void>("enviar_mensagem", { sessionId, texto }),

  cancelarTurno: (sessionId: string) => chamar<void>("cancelar_turno", { sessionId }),

  historico: (sessionId: string, limite = 200) =>
    chamar<Mensagem[]>("historico", { sessionId, limite }),

  acoesDaSessao: (sessionId: string) =>
    chamar<ChamadaFerramenta[]>("acoes_da_sessao", { sessionId }),

  custoDoWorkspace: (workspaceId: string) =>
    chamar<{ total: number; por_no: CustoDoNo[] }>("custo_do_workspace", { workspaceId }),

  // ------------------------------------------------------------ arquivos

  listarPasta: (workspaceId: string, sub: string) =>
    chamar<ItemArquivo[]>("listar_pasta", { workspaceId, sub }),

  lerNota: (nodeId: string) => chamar<Nota>("ler_nota", { nodeId }),

  escreverNota: (nodeId: string, conteudo: string) =>
    chamar<void>("escrever_nota", { nodeId, conteudo }),

  // ----------------------------------------------------------- aprovação

  decidirAprovacao: (toolCallId: string, decisao: Decisao, lembrar: boolean) =>
    chamar<void>("decidir_aprovacao", { toolCallId, decisao, lembrar }),

  listarRegras: (workspaceId: string) =>
    chamar<RegraAprovacao[]>("listar_regras", { workspaceId }),

  revogarRegra: (id: string) => chamar<void>("revogar_regra", { id }),

  aprovacoesPendentes: (sessionId: string) =>
    chamar<PedidoAprovacao[]>("aprovacoes_pendentes", { sessionId }),

  // ------------------------------------------------------ papéis e times

  listarPapeis: () => chamar<Papel[]>("listar_papeis"),

  criarPapel: (
    nome: string,
    prompt: string,
    ferramentas: string[],
    autonomia: Autonomia,
    modelo: string | null,
  ) => chamar<Papel>("criar_papel", { nome, prompt, ferramentas, autonomia, modelo }),

  editarPapel: (
    id: string,
    prompt: string,
    ferramentas: string[],
    autonomia: Autonomia,
    modelo: string | null,
  ) => chamar<Papel>("editar_papel", { id, prompt, ferramentas, autonomia, modelo }),

  removerPapel: (id: string) => chamar<void>("remover_papel", { id }),

  quantosUsamOPapel: (id: string) => chamar<number>("quantos_usam_o_papel", { id }),

  /** `roleId` nulo tira o papel: o nó volta a ser um agente sem papel. */
  definirPapelDoNo: (nodeId: string, roleId: string | null) =>
    chamar<No>("definir_papel_do_no", { nodeId, roleId }),

  salvarTime: (workspaceId: string, nome: string) =>
    chamar<Partitura>("salvar_time", { workspaceId, nome }),

  listarTimes: (workspaceId: string) =>
    chamar<Partitura[]>("listar_times", { workspaceId }),

  abrirTime: (workspaceId: string, partituraId: string) =>
    chamar<No[]>("abrir_time", { workspaceId, partituraId }),

  removerTime: (id: string) => chamar<void>("remover_time", { id }),

  // ---------------------------------------------------------- rascunhos

  listarRascunhos: (workspaceId: string) =>
    chamar<Ensaio[]>("listar_rascunhos", { workspaceId }),

  criarRascunho: (workspaceId: string, nome: string) =>
    chamar<Ensaio>("criar_rascunho", { workspaceId, nome }),

  /** `ensaioId` nulo volta para a pasta de verdade. */
  trocarRascunho: (workspaceId: string, ensaioId: string | null) =>
    chamar<void>("trocar_rascunho", { workspaceId, ensaioId }),

  descartarRascunho: (ensaioId: string) =>
    chamar<void>("descartar_rascunho", { ensaioId }),

  /** Não escreve nada: é o que a tela mostra antes do clique. */
  preverPublicacao: (ensaioId: string) =>
    chamar<PreviaPublicacao>("prever_publicacao", { ensaioId }),

  publicarRascunho: (ensaioId: string, escolhas: Array<[string, LadoDoConflito]>) =>
    chamar<PreviaPublicacao>("publicar_rascunho", { ensaioId, escolhas }),

  definirMcpDoPapel: (id: string, servidores: ServidorMcp[]) =>
    chamar<Papel>("definir_mcp_do_papel", { id, servidores }),
};

/**
 * Assina um evento do núcleo. Devolve a função que cancela a assinatura —
 * chame-a no cleanup do efeito, ou cada remontagem deixa um ouvinte para trás
 * e o mesmo delta de texto entra duas vezes na bolha.
 */
export async function escutar<T>(
  evento: string,
  aoReceber: (payload: T) => void,
): Promise<() => void> {
  if (!dentroDoTauri) return escutarFalso(evento, aoReceber as Ouvinte);
  const { listen } = await import("@tauri-apps/api/event");
  return await listen<T>(evento, (e) => aoReceber(e.payload));
}

export const modoNavegador = !dentroDoTauri;

// ------------------------------------------------------------------ falso

const TAMANHOS: Record<TipoNo, [number, number]> = {
  agente: [420, 320],
  nota: [260, 200],
  arquivos: [280, 360],
  portal: [480, 360],
  forma: [200, 120],
};

const mem = {
  workspaces: [] as Workspace[],
  nos: [] as No[],
  cabos: [] as Cabo[],
  sessoes: [] as Sessao[],
  mensagens: [] as Mensagem[],
  acoes: [] as ChamadaFerramenta[],
  regras: [] as RegraAprovacao[],
  papeis: [] as Papel[],
  times: [] as Partitura[],
  rascunhos: [] as Ensaio[],
  /** Arquivos por rascunho, no falso: id do ensaio -> caminho -> conteúdo. */
  arquivosDoRascunho: new Map<string, Map<string, string>>(),
  /** Sistema de arquivos de mentira: caminho relativo -> conteúdo. */
  arquivos: new Map<string, string>(),
};

/**
 * Espelho de `papeis::embutidos()`. Só nome, autonomia e uma frase do prompt:
 * o modo navegador precisa de papéis para a interface ter o que mostrar, não
 * dos prompts inteiros — quem os usa de verdade é o Rust.
 */
const PAPEIS_EMBUTIDOS: Array<[string, Autonomia, string]> = [
  ["Pesquisador", "cauteloso", "Acha e entende material. Não escreve arquivo nem nota."],
  ["Redator", "padrao", "Transforma material bruto em texto que uma pessoa leia sem esforço."],
  ["Revisor", "cauteloso", "Acha o que está errado antes que chegue a quem pediu."],
  ["Analista", "solto", "Planilha, conversão, conferência de número. Roda comando com aprovação."],
  ["Organizador", "padrao", "Monta o time e reparte o trabalho — não faz o trabalho."],
];

function semearPapeis() {
  if (mem.papeis.length) return;
  for (const [nome, autonomia, prompt] of PAPEIS_EMBUTIDOS) {
    mem.papeis.push({
      id: id(),
      nome,
      prompt,
      ferramentas: nome === "Organizador" ? ["recrutar", "dispensar", "enviar_para"] : [],
      autonomia,
      modelo: null,
      embutido: true,
      criado_em: agora(),
    });
  }
}

/** A pasta de exemplo do modo navegador, para a árvore ter o que mostrar. */
function semearArquivos() {
  if (mem.arquivos.size) return;
  mem.arquivos.set("contratos/minuta.docx", "documento binário de mentira");
  mem.arquivos.set("contratos/anexo-i.pdf", "pdf de mentira");
  mem.arquivos.set("planilhas/orçamento.xlsx", "planilha de mentira");
  mem.arquivos.set("Briefing.md", "# Briefing\n\nMemória compartilhada entre os agentes ligados.\n");
}

/** Espelho de `barramento::FERRAMENTAS_QUE_PEDEM_LICENCA`. */
const PEDEM_LICENCA = ["Write", "Edit", "NotebookEdit", "Bash", "WebFetch"];

/**
 * Espelho de `barramento::pede_licenca`: as nativas mais as do §6 que gravam.
 * Se `escrever_nota` escapasse daqui, o modo navegador mostraria uma gravação
 * passando sem card — e mentiria sobre o que o app de verdade faz.
 */
function pedeLicencaFalso(ferramenta: string): boolean {
  return (
    PEDEM_LICENCA.includes(ferramenta) ||
    FERRAMENTAS_MCP_QUE_GRAVAM.some((f) => nomeCompletoMcp(f) === ferramenta)
  );
}

/** Cards abertos: id da chamada -> quem destrava o turno quando o usuário clica. */
const cardsAbertos = new Map<string, (d: Decisao) => void>();

const id = () => crypto.randomUUID();
const agora = () => Date.now();

// --------------------------------------------------- eventos do modo falso

type Ouvinte = (payload: unknown) => void;
const ouvintes = new Map<string, Set<Ouvinte>>();

function escutarFalso(evento: string, aoReceber: Ouvinte): () => void {
  const conjunto = ouvintes.get(evento) ?? new Set<Ouvinte>();
  conjunto.add(aoReceber);
  ouvintes.set(evento, conjunto);
  return () => conjunto.delete(aoReceber);
}

function emitirFalso(evento: string, payload: unknown) {
  // Mesma cópia do resto do falso: o evento de verdade é serializado, e sem
  // imitar isso o front acaba mutando o objeto do "backend" sem perceber.
  for (const f of ouvintes.get(evento) ?? []) f(structuredClone(payload));
}

// ------------------------------------------------- turno falso do M1

/** Turnos em andamento, para o cancelamento ter o que interromper. */
const turnos = new Map<string, { parar: boolean }>();

/**
 * Espelho em TypeScript do `Roteiro::demonstracao` do núcleo. Existe porque o
 * modo navegador não tem Rust por baixo, e sem ele não dá para desenvolver nem
 * testar a face conversa fora do Tauri.
 *
 * Fonte da verdade dos preços é `nucleo/src/modelo.rs`. Os números abaixo são
 * os de Opus 5 — US$5 por milhão de entrada, US$25 de saída — e o total já
 * calculado, para não existir uma segunda tabela de preços aqui.
 */
function roteiroDemonstracao(pergunta: string): { atrasoMs: number; eventos: EventoAgente[] } {
  const curto = pergunta.trim().split("\n")[0] ?? "";
  const resumo = curto.length > 40 ? `${curto.slice(0, 40).trimEnd()}…` : curto;
  // Um id por chamada, como o Claude de verdade faz (`toolu_…`). Reaproveitar
  // o id entre turnos fazia a segunda decisão cair na linha do turno anterior,
  // que já estava decidida — e o agente ficava esperando para sempre.
  const t = id().slice(0, 8);
  return {
    atrasoMs: 90,
    eventos: [
      {
        tipo: "sessao_iniciada",
        id_externo: `falso_${id().slice(0, 8)}`,
        modelo: "claude-opus-5",
        ferramentas: ["ler_arquivo", "listar_arquivos"],
      },
      { tipo: "raciocinando", resumo: "Procurando o documento na pasta do workspace." },
      // Nomes de ferramenta iguais aos do Claude Code de verdade. O falso não
      // serve para nada se falar uma língua que o adaptador real não fala.
      {
        tipo: "ferramenta_pedida",
        id: `fer_${t}_1`,
        nome: "Read",
        argumentos: { file_path: "contrato-v3.docx" },
      },
      {
        tipo: "ferramenta_concluida",
        id: `fer_${t}_1`,
        resultado: { bytes: 48213, truncado: false },
        erro: null,
      },
      { tipo: "texto_parcial", delta: "Li o material" },
      { tipo: "texto_parcial", delta: " sobre o documento." },
      // Esta pede licença: é o que o card do M2 existe para interceptar.
      {
        tipo: "ferramenta_pedida",
        id: `fer_${t}_2`,
        nome: "Write",
        argumentos: {
          file_path: "resumo-do-contrato.md",
          content: "# Resumo\n\nCláusula de reajuste cita índice extinto em 2023.\n",
        },
      },
      {
        tipo: "ferramenta_concluida",
        id: `fer_${t}_2`,
        resultado: { bytes: 74 },
        erro: null,
      },
      // A ponte do M3: só acontece se houver um cabo `fala_com` para um nó com
      // este nome. Sem cabo, o nó não existe — e aqui isso é literal, como no
      // núcleo: `pontearFalso` devolve `null` e a chamada falha.
      {
        tipo: "ferramenta_pedida",
        id: `fer_${t}_3`,
        nome: nomeCompletoMcp("enviar_para"),
        argumentos: { no: "Redator", mensagem: "Revise o resumo e confira o índice." },
      },
      {
        tipo: "ferramenta_concluida",
        id: `fer_${t}_3`,
        resultado: { resposta: "Conferido: o índice foi extinto em 2023." },
        erro: null,
      },
      {
        tipo: "turno_concluido",
        texto_final:
          `Li o material sobre "${resumo}". O ponto que salta é a cláusula de ` +
          `reajuste: ela cita um índice que não existe mais desde 2023.`,
        uso: {
          tokens_entrada: 1420,
          tokens_saida: 96,
          custo_usd: (1420 * 5 + 96 * 25) / 1_000_000,
        },
      },
    ],
  };
}

/** Espelho de `modelo::descrever_ferramenta`. */
function descreverFerramenta(
  ferramenta: string,
  argumentos: Record<string, unknown>,
): [string, string] {
  const campo = (c: string) => (typeof argumentos[c] === "string" ? (argumentos[c] as string) : "");
  const arquivo = (c: string) => campo(c).split(/[/\\]/).pop() ?? "";
  const tamanho = (b: number) => (b < 1024 ? `${b} B` : `${(b / 1024).toFixed(1)} kB`);
  switch (ferramenta) {
    case "Write": {
      const conteudo = campo("content");
      return [
        `Gravar ${arquivo("file_path")}`,
        `${conteudo.split("\n").length} linhas · ${tamanho(conteudo.length)}`,
      ];
    }
    case "Edit":
    case "NotebookEdit":
      return [`Alterar ${arquivo("file_path")}`, "trecho substituído no arquivo"];
    case "Bash":
      return ["Rodar um comando", campo("command").slice(0, 120)];
    default:
      return [`Usar ${ferramenta}`, JSON.stringify(argumentos).slice(0, 120)];
  }
}

function previaDoConteudo(argumentos: Record<string, unknown>): string | null {
  for (const chave of ["content", "new_string", "command"]) {
    const v = argumentos[chave];
    if (typeof v === "string") return v.length <= 600 ? v : `${v.slice(0, 600)}\n…`;
  }
  return null;
}

function sessaoFalsa(sessionId: string): Sessao {
  const s = mem.sessoes.find((k) => k.id === sessionId);
  if (!s) throw { codigo: "nao_encontrado", mensagem: "sessão não encontrada" };
  return s;
}

function mudarEstadoFalso(s: Sessao, estado: EstadoSessao) {
  s.estado = estado;
  s.ultimo_sinal_em = agora();
  emitirFalso("sessao:estado", {
    tipo: "sessao_estado",
    session_id: s.id,
    node_id: s.node_id,
    estado,
    pede_atencao:
      estado === "aguardando_aprovacao" || estado === "aguardando_humano" || estado === "erro",
  });
}

function gravarMensagemFalsa(
  sessionId: string,
  papel: PapelMensagem,
  conteudo: string,
  tokens = 0,
  custo = 0,
  origem: { node: string; trace: string } | null = null,
): Mensagem {
  const m: Mensagem = {
    id: id(),
    session_id: sessionId,
    papel,
    origem_node: origem?.node ?? null,
    conteudo,
    tokens,
    custo,
    trace_id: origem?.trace ?? null,
    criado_em: agora(),
  };
  mem.mensagens.push(m);
  return m;
}

/**
 * A ponte do M3 no modo navegador.
 *
 * Duplica o `Orquestrador::entregar` de propósito, como o resto deste arquivo
 * duplica o núcleo: sem isto, `npm run dev` mostraria um canvas onde os cabos
 * nunca acendem, e a única feature nova do marco seria invisível fora do app.
 * O que ele NÃO faz é a fila, os três limites e o escopo por cabo — isso é
 * regra, e regra mora no Rust, com teste.
 */
function pontearFalso(s: Sessao, alvoNome: string): string | null {
  const meu = mem.nos.find((n) => n.id === s.node_id);
  if (!meu) return null;
  const ligados = mem.cabos
    .filter((c) => c.tipo === "fala_com" && (c.de_node === meu.id || c.para_node === meu.id))
    .map((c) => (c.de_node === meu.id ? c.para_node : c.de_node));
  const alvo = mem.nos.find((n) => ligados.includes(n.id) && n.nome === alvoNome);
  if (!alvo) return null;

  const trace = `tr_${id().slice(0, 8)}`;
  emitirFalso("no:mensagem", {
    tipo: "no_mensagem",
    de_node: meu.id,
    para_node: alvo.id,
    trace_id: trace,
    tipo_mensagem: "pedido",
  });

  // O recado entra na conversa do OUTRO nó, com quem falou e em que cadeia.
  const sessaoAlvo = mem.sessoes.find((k) => k.node_id === alvo.id);
  if (sessaoAlvo) {
    gravarMensagemFalsa(
      sessaoAlvo.id,
      "no",
      "Revise o resumo do contrato e me diga se o índice citado ainda existe.",
      0,
      0,
      { node: meu.id, trace },
    );
    // A gravação vem ANTES do aviso de estado, na mesma ordem do
    // `Orquestrador::iniciar_turno` — é dessa ordem que a face conversa
    // depende para achar o recado quando relê o histórico.
    mudarEstadoFalso(sessaoAlvo, "pensando");

    // O outro nó levanta a mão ANTES de responder. É o caminho que o núcleo
    // percorre quando o destinatário chama `perguntar_humano` no meio de uma
    // entrega — ver `Orquestrador::esperar_resposta` —, e reproduzi-lo aqui é
    // o único jeito de a interface do aviso ser exercitada fora do app.
    //
    // Sem atraso nenhum, de propósito: no app o intervalo é o tempo de o
    // agente pensar, mas aqui um atraso vira corrida com o teste de fumaça —
    // e teste que passa nove vezes em dez é pior que teste vermelho.
    mudarEstadoFalso(sessaoAlvo, "aguardando_humano");
    emitirFalso("cadeia:espera-pessoa", {
      tipo: "cadeia_espera_pessoa",
      trace_id: trace,
      node_id: meu.id,
      perguntou_node: alvo.id,
      perguntou_nome: alvo.nome,
    });

    window.setTimeout(() => {
      gravarMensagemFalsa(sessaoAlvo.id, "agente", "Conferido: o índice foi extinto em 2023.");
      // Sair de `aguardando_humano` é o que apaga o aviso, no falso como no
      // app: quem limpa é o evento de estado, não um relógio.
      mudarEstadoFalso(sessaoAlvo, "ocioso");
    }, 4000);
  }
  return alvo.nome;
}

/** Roda o roteiro, um evento por vez, respeitando o cancelamento. */
async function rodarTurnoFalso(s: Sessao, pergunta: string) {
  const controle = { parar: false };
  turnos.set(s.id, controle);
  const { atrasoMs, eventos } = roteiroDemonstracao(pergunta);
  let acumulado = "";
  const negados = new Set<string>();

  for (const evento of eventos) {
    await new Promise((r) => setTimeout(r, atrasoMs));
    if (controle.parar) return;

    // Ferramenta negada precisa CHEGAR como erro à interface. A interface
    // monta o card de ação a partir do evento, não da memória daqui — mutar
    // só a memória deixaria o card verde para uma gravação que não aconteceu.
    const paraEmitir: EventoAgente =
      evento.tipo === "ferramenta_concluida" && negados.has(`${s.id}:${evento.id}`)
        ? { ...evento, resultado: null, erro: "Negado no Mutirão." }
        : evento;
    emitirFalso("sessao:evento", {
      tipo: "sessao_evento",
      session_id: s.id,
      evento: paraEmitir,
    });

    switch (evento.tipo) {
      case "sessao_iniciada":
        s.sessao_externa_id = evento.id_externo;
        break;
      case "texto_parcial":
        acumulado += evento.delta;
        break;
      case "ferramenta_pedida": {
        const idChamada = `${s.id}:${evento.id}`;
        const wsId = mem.nos.find((n) => n.id === s.node_id)?.workspace_id ?? "";
        const precisa = pedeLicencaFalso(evento.nome);
        const regra = mem.regras.find(
          (r) => r.workspace_id === wsId && r.ferramenta === evento.nome,
        );

        const acao: ChamadaFerramenta = {
          id: idChamada,
          session_id: s.id,
          ferramenta: evento.nome,
          argumentos: evento.argumentos,
          resultado: null,
          erro: null,
          aprovacao: !precisa ? "automatica" : regra ? "aprovada" : "pendente",
          decidido_por: precisa && regra ? `regra:${evento.nome}` : null,
          criado_em: agora(),
        };
        mem.acoes.push(acao);

        // A ponte acende o cabo aqui, no pedido — é quando o recado sai, não
        // quando a resposta volta.
        if (evento.nome === nomeCompletoMcp("enviar_para")) {
          const alvo = evento.argumentos.no;
          if (typeof alvo !== "string" || pontearFalso(s, alvo) === null) {
            acao.erro = `nó não encontrado: ${String(alvo)}`;
          }
        }

        if (acao.aprovacao === "pendente") {
          const [resumo, detalhe] = descreverFerramenta(evento.nome, evento.argumentos);
          mudarEstadoFalso(s, "aguardando_aprovacao");
          emitirFalso("aprovacao:pedida", {
            tipo: "aprovacao_pedida",
            pedido: {
              tool_call_id: idChamada,
              session_id: s.id,
              node_id: s.node_id,
              ferramenta: evento.nome,
              resumo,
              detalhe,
              previa: previaDoConteudo(evento.argumentos),
              criado_em: agora(),
            },
          });
          // O turno para aqui, de verdade, até alguém clicar — é o que faz o
          // card honesto: o arquivo não é gravado e desfeito, ele não chega
          // a ser gravado.
          const decisao = await new Promise<Decisao>((r) => cardsAbertos.set(idChamada, r));
          if (controle.parar) return;
          mudarEstadoFalso(s, "pensando");
          if (decisao === "negada") negados.add(idChamada);
        }
        break;
      }
      case "ferramenta_concluida": {
        const idChamada = `${s.id}:${evento.id}`;
        const acao = mem.acoes.find((a) => a.id === idChamada);
        if (acao) {
          if (negados.has(idChamada)) {
            acao.erro = "Negado no Mutirão.";
          } else {
            acao.resultado = evento.resultado;
            acao.erro = evento.erro;
          }
        }
        break;
      }
      case "turno_concluido": {
        const texto = evento.texto_final.trim() || acumulado;
        const tokens = evento.uso.tokens_entrada + evento.uso.tokens_saida;
        gravarMensagemFalsa(s.id, "agente", texto, tokens, evento.uso.custo_usd);
        s.custo_total += Number.isFinite(evento.uso.custo_usd) ? evento.uso.custo_usd : 0;
        mudarEstadoFalso(s, "ocioso");
        const no = mem.nos.find((n) => n.id === s.node_id);
        if (no) {
          const porNo = mem.sessoes
            .filter((k) => mem.nos.some((n) => n.id === k.node_id))
            .map((k) => ({ node_id: k.node_id, custo: k.custo_total }));
          emitirFalso("custo:atualizado", {
            tipo: "custo_atualizado",
            workspace_id: no.workspace_id,
            total: porNo.reduce((t, c) => t + c.custo, 0),
            por_no: porNo,
          });
        }
        turnos.delete(s.id);
        return;
      }
    }
  }
}

function semear() {
  if (mem.workspaces.length) return;
  const ws: Workspace = {
    id: id(),
    nome: "Demonstração",
    pasta: "C:\\Users\\voce\\Mutirao\\demo",
    criado_em: agora(),
    ensaio_ativo: null,
    // O modo navegador não tem Git por baixo: os rascunhos existem em memória,
    // como o resto do falso. Um caminho de mentira aqui é mais honesto que
    // `null`, que diria "esta máquina não tem histórico" — e diria errado.
    repo: "(memória)",
    viewport: { x: 80, y: 60, zoom: 0.9 },
  };
  mem.workspaces.push(ws);

  const criar = (tipo: TipoNo, nome: string, x: number, y: number): No => {
    const [w, h] = TAMANHOS[tipo];
    const n: No = {
      id: id(),
      workspace_id: ws.id,
      ensaio_id: null,
      tipo,
      nome,
      x,
      y,
      w,
      h,
      z: mem.nos.length + 1,
      config: {},
      role_id: null,
      recrutado_por: null,
      criado_em: agora(),
      alterado_em: agora(),
    };
    mem.nos.push(n);
    return n;
  };

  const pesquisa = criar("agente", "Pesquisador", 60, 80);
  const redator = criar("agente", "Redator", 560, 80);
  const briefing = criar("nota", "Briefing", 320, 470);
  criar("arquivos", "Pasta do projeto", 640, 470);

  mem.cabos.push(
    cabo(ws.id, pesquisa.id, redator.id, "fala_com"),
    cabo(ws.id, pesquisa.id, briefing.id, "escreve_nota"),
    cabo(ws.id, redator.id, briefing.id, "le_nota"),
  );
}

/**
 * O que muda ao publicar, no falso. Compara a cópia do rascunho com a pasta.
 *
 * Conflito aqui é "os dois lados mexeram no mesmo arquivo" — o Rust usa o
 * `merge-tree` do Git, que sabe juntar texto quando dá; o falso não sabe, e
 * chama de conflito qualquer coincidência. É mais pessimista que o de verdade,
 * o que é o lado certo de errar numa maquete.
 */
function previaFalsa(ensaioId: string): PreviaPublicacao {
  const doRascunho = mem.arquivosDoRascunho.get(ensaioId) ?? new Map<string, string>();
  const alteracoes: PreviaPublicacao["alteracoes"] = [];
  const conflitos: string[] = [];

  for (const [caminho, conteudo] of doRascunho) {
    const naPasta = mem.arquivos.get(caminho);
    if (naPasta === conteudo) continue;
    alteracoes.push({ caminho, como: naPasta === undefined ? "criado" : "alterado" });
    if (naPasta !== undefined) conflitos.push(caminho);
  }
  for (const caminho of mem.arquivos.keys()) {
    if (!doRascunho.has(caminho)) alteracoes.push({ caminho, como: "apagado" });
  }
  return { ensaio_id: ensaioId, alteracoes, conflitos };
}

function cabo(ws: string, de: string, para: string, tipo: TipoCabo): Cabo {
  return { id: id(), workspace_id: ws, de_node: de, para_node: para, tipo, criado_em: agora() };
}

async function falso<T>(comando: string, a: Record<string, any>): Promise<T> {
  semear();
  semearPapeis();
  // O IPC de verdade serializa tudo que atravessa a fronteira, então o front
  // nunca compartilha objeto com o backend. O falso precisa imitar isso: sem
  // a cópia, o `push` daqui e o append no estado do React viram o MESMO
  // cabo contado duas vezes — foi exatamente esse o bug que apareceu no
  // teste de fumaça.
  const qualquer = (v: unknown) => structuredClone(v) as T;

  switch (comando) {
    case "listar_workspaces":
      return qualquer(mem.workspaces);

    // ----------------------------------------------------- papéis e times

    case "listar_papeis":
      return qualquer(mem.papeis);

    case "definir_papel_do_no": {
      const n = mem.nos.find((k) => k.id === a.nodeId);
      if (!n) throw { codigo: "nao_encontrado", mensagem: "nó não encontrado" };
      n.role_id = a.roleId ?? null;
      n.alterado_em = agora();
      return qualquer(n);
    }

    case "salvar_time": {
      const posicao = new Map(mem.nos.map((n, i) => [n.id, i]));
      const p: Partitura = {
        id: id(),
        workspace_id: a.workspaceId,
        nome: (a.nome as string).trim(),
        snapshot: {
          nos: mem.nos.map((n) => ({
            tipo: n.tipo,
            nome: n.nome,
            x: n.x,
            y: n.y,
            w: n.w,
            h: n.h,
            config: n.config,
            papel: mem.papeis.find((x) => x.id === n.role_id)?.nome ?? null,
          })),
          cabos: mem.cabos.flatMap((c) => {
            const de = posicao.get(c.de_node);
            const para = posicao.get(c.para_node);
            return de === undefined || para === undefined ? [] : [{ de, para, tipo: c.tipo }];
          }),
        },
        criado_em: agora(),
      };
      if (!p.nome) {
        throw { codigo: "invalido", mensagem: "o time precisa de um nome para você achar depois" };
      }
      // Mesmo nome atualiza, como no Rust: quem repete está atualizando o
      // time, não descobrindo um índice único.
      const anterior = mem.times.find(
        (t) => t.workspace_id === p.workspace_id && t.nome === p.nome,
      );
      if (anterior) {
        Object.assign(anterior, p, { id: anterior.id });
        return qualquer(anterior);
      }
      mem.times.push(p);
      return qualquer(p);
    }

    case "listar_times":
      return qualquer(mem.times.filter((t) => t.workspace_id === a.workspaceId));

    // ---------------------------------------------------------- rascunhos
    //
    // O modo navegador não tem Git por baixo. O falso guarda os rascunhos em
    // memória com as MESMAS regras visíveis: a pasta de verdade não muda até
    // publicar, conflito precisa de escolha, e descartar joga fora. O que ele
    // não faz é mesclar de verdade — isso é do Rust, e é lá que tem teste.

    case "listar_rascunhos":
      return qualquer(mem.rascunhos.filter((e) => e.workspace_id === a.workspaceId));

    case "criar_rascunho": {
      const e: Ensaio = {
        id: id(),
        workspace_id: a.workspaceId,
        nome: (a.nome as string).trim(),
        branch: "(memória)",
        caminho_worktree: "(memória)",
        base_commit: null,
        estado: "aberto",
        criado_em: agora(),
        alterado_em: agora(),
      };
      if (!e.nome) throw { codigo: "invalido", mensagem: "o rascunho precisa de um nome" };
      mem.rascunhos.push(e);
      // Nasce com uma cópia do que está na pasta agora.
      mem.arquivosDoRascunho.set(e.id, new Map(mem.arquivos));
      return qualquer(e);
    }

    case "trocar_rascunho": {
      const ws = mem.workspaces.find((w) => w.id === a.workspaceId);
      if (!ws) throw { codigo: "nao_encontrado", mensagem: "workspace não encontrado" };
      ws.ensaio_ativo = a.ensaioId ?? null;
      return qualquer(undefined);
    }

    case "descartar_rascunho": {
      const e = mem.rascunhos.find((x) => x.id === a.ensaioId);
      if (!e) throw { codigo: "nao_encontrado", mensagem: "rascunho não encontrado" };
      e.estado = "descartado";
      e.alterado_em = agora();
      mem.arquivosDoRascunho.delete(e.id);
      const ws = mem.workspaces.find((w) => w.id === e.workspace_id);
      if (ws?.ensaio_ativo === e.id) ws.ensaio_ativo = null;
      return qualquer(undefined);
    }

    case "prever_publicacao":
      return qualquer(previaFalsa(a.ensaioId as string));

    case "publicar_rascunho": {
      const e = mem.rascunhos.find((x) => x.id === a.ensaioId);
      if (!e) throw { codigo: "nao_encontrado", mensagem: "rascunho não encontrado" };
      const previa = previaFalsa(e.id);
      const escolhas = new Map((a.escolhas ?? []) as Array<[string, string]>);
      const semEscolha = previa.conflitos.find((c) => !escolhas.has(c));
      if (semEscolha) {
        throw {
          codigo: "invalido",
          mensagem: `"${semEscolha}" mudou dos dois lados e ninguém escolheu qual fica. Nada foi publicado.`,
        };
      }
      const doRascunho = mem.arquivosDoRascunho.get(e.id) ?? new Map();
      for (const m of previa.alteracoes) {
        if (escolhas.get(m.caminho) === "original") continue;
        const conteudo = doRascunho.get(m.caminho);
        if (conteudo === undefined) mem.arquivos.delete(m.caminho);
        else mem.arquivos.set(m.caminho, conteudo);
      }
      e.estado = "publicado";
      e.alterado_em = agora();
      const ws = mem.workspaces.find((w) => w.id === e.workspace_id);
      if (ws?.ensaio_ativo === e.id) ws.ensaio_ativo = null;
      mem.arquivosDoRascunho.delete(e.id);
      return qualquer({ ...previa, conflitos: [] });
    }

    case "remover_time": {
      mem.times = mem.times.filter((t) => t.id !== a.id);
      return qualquer(undefined);
    }

    case "abrir_time": {
      const p = mem.times.find((t) => t.id === a.partituraId);
      if (!p) throw { codigo: "nao_encontrado", mensagem: "time salvo não encontrado" };
      const usados = mem.nos.map((n) => n.nome.trim().toLowerCase());
      // Mesmo deslocamento do `partituras::deslocamento`: o time volta à
      // direita do que já existe, mantendo a FORMA que tinha. Empilhar em
      // diagonal perderia justamente o que a partitura guarda — quem está
      // perto de quem.
      const direita = mem.nos.length ? Math.max(...mem.nos.map((n) => n.x + n.w)) : 0;
      const esquerdaDoTime = Math.min(...p.snapshot.nos.map((n) => n.x));
      const dx = mem.nos.length ? direita + 80 - esquerdaDoTime : 0;
      const criados: No[] = p.snapshot.nos.map((salvo, i) => {
        let nome = salvo.nome;
        for (let k = 2; usados.includes(nome.trim().toLowerCase()); k++) {
          nome = `${salvo.nome} ${k}`;
        }
        usados.push(nome.trim().toLowerCase());
        const n: No = {
          id: id(),
          workspace_id: a.workspaceId,
          ensaio_id: null,
          tipo: salvo.tipo,
          nome,
          x: salvo.x + dx,
          y: salvo.y,
          w: salvo.w,
          h: salvo.h,
          z: Math.max(0, ...mem.nos.map((k) => k.z)) + 1 + i,
          config: salvo.config ?? {},
          role_id: mem.papeis.find((x) => x.nome === salvo.papel)?.id ?? null,
          recrutado_por: null,
          criado_em: agora(),
          alterado_em: agora(),
        };
        mem.nos.push(n);
        return n;
      });
      for (const c of p.snapshot.cabos) {
        const de = criados[c.de];
        const para = criados[c.para];
        if (de && para) mem.cabos.push(cabo(a.workspaceId, de.id, para.id, c.tipo));
      }
      return qualquer(criados);
    }

    case "abrir_workspace": {
      const ws = mem.workspaces.find((w) => w.id === a.workspaceId);
      if (!ws) throw { codigo: "nao_encontrado", mensagem: "workspace não encontrado" };
      return qualquer({
        workspace: ws,
        nos: [...mem.nos].sort((x, y) => x.z - y.z),
        cabos: mem.cabos,
      } satisfies EstadoCanvas);
    }

    case "salvar_viewport": {
      const ws = mem.workspaces.find((w) => w.id === a.workspaceId);
      if (ws) ws.viewport = { x: a.x, y: a.y, zoom: a.zoom };
      return qualquer(undefined);
    }

    case "criar_no": {
      const [w, h] = TAMANHOS[a.tipo as TipoNo];
      const n: No = {
        id: id(),
        workspace_id: a.workspaceId,
        ensaio_id: null,
        tipo: a.tipo,
        nome: (a.nome as string).trim() || "Nó",
        x: a.x,
        y: a.y,
        w,
        h,
        z: Math.max(0, ...mem.nos.map((k) => k.z)) + 1,
        config: {},
        role_id: null,
        recrutado_por: null,
        criado_em: agora(),
        alterado_em: agora(),
      };
      mem.nos.push(n);
      return qualquer(n);
    }

    case "mover_no": {
      const n = mem.nos.find((k) => k.id === a.id);
      if (n) Object.assign(n, { x: a.x, y: a.y, w: a.w, h: a.h, alterado_em: agora() });
      return qualquer(undefined);
    }

    case "renomear_no": {
      const n = mem.nos.find((k) => k.id === a.id);
      if (n) n.nome = a.nome;
      return qualquer(undefined);
    }

    case "trazer_para_frente": {
      const n = mem.nos.find((k) => k.id === a.id);
      const z = Math.max(0, ...mem.nos.map((k) => k.z)) + 1;
      if (n) n.z = z;
      return qualquer(z);
    }

    case "remover_no": {
      mem.nos = mem.nos.filter((k) => k.id !== a.id);
      mem.cabos = mem.cabos.filter((c) => c.de_node !== a.id && c.para_node !== a.id);
      // Espelha o ON DELETE CASCADE do esquema: sumir o nó e deixar a conversa
      // órfã aqui esconderia um bug que só apareceria no app de verdade.
      const orfas = mem.sessoes.filter((s) => s.node_id === a.id).map((s) => s.id);
      mem.sessoes = mem.sessoes.filter((s) => s.node_id !== a.id);
      mem.mensagens = mem.mensagens.filter((m) => !orfas.includes(m.session_id));
      mem.acoes = mem.acoes.filter((c) => !orfas.includes(c.session_id));
      return qualquer(undefined);
    }

    case "criar_cabo": {
      if (a.deNode === a.paraNode)
        throw { codigo: "invalido", mensagem: "um nó não se conecta a si mesmo" };
      const existe = mem.cabos.some(
        (c) => c.de_node === a.deNode && c.para_node === a.paraNode && c.tipo === a.tipo,
      );
      if (existe) throw { codigo: "invalido", mensagem: "esses nós já estão ligados desse jeito" };
      const c = cabo(a.workspaceId, a.deNode, a.paraNode, a.tipo);
      mem.cabos.push(c);
      return qualquer(c);
    }

    case "remover_cabo":
      mem.cabos = mem.cabos.filter((c) => c.id !== a.id);
      return qualquer(undefined);

    // ----------------------------------------------------------- sessões

    // O modo navegador não tem Rust por baixo, então também não tem CLI para
    // chamar: aqui é sempre o roteiro, e a barra diz isso.
    case "adaptador_em_uso":
      return qualquer({
        adaptador: "falso" as Adaptador,
        detalhe: "modo navegador — roteiro de demonstração, nada é gravado",
      });

    case "abrir_sessao": {
      const existente = mem.sessoes.find((s) => s.node_id === a.nodeId);
      if (existente) return qualquer(existente);
      const no = mem.nos.find((n) => n.id === a.nodeId);
      if (!no) throw { codigo: "nao_encontrado", mensagem: "nó não encontrado" };
      if (no.tipo !== "agente")
        throw { codigo: "invalido", mensagem: "só nó de agente abre sessão" };
      const s: Sessao = {
        id: id(),
        node_id: a.nodeId,
        adaptador: "falso",
        sessao_externa_id: null,
        estado: "ocioso",
        custo_total: 0,
        iniciada_em: agora(),
        ultimo_sinal_em: agora(),
      };
      mem.sessoes.push(s);
      return qualquer(s);
    }

    case "sessao_do_no":
      return qualquer(mem.sessoes.find((s) => s.node_id === a.nodeId) ?? null);

    case "enviar_mensagem": {
      const texto = String(a.texto ?? "").trim();
      if (!texto)
        throw { codigo: "invalido", mensagem: "não dá para mandar mensagem vazia" };
      const s = sessaoFalsa(a.sessionId);
      if (s.estado !== "ocioso" && s.estado !== "erro") {
        throw {
          codigo: "invalido",
          mensagem: "esse nó ainda está no meio de um turno. Espere ou clique em parar.",
        };
      }
      gravarMensagemFalsa(s.id, "usuario", texto);
      mudarEstadoFalso(s, "pensando");
      void rodarTurnoFalso(s, texto);
      return qualquer(undefined);
    }

    case "cancelar_turno": {
      const s = sessaoFalsa(a.sessionId);
      const controle = turnos.get(s.id);
      if (controle) controle.parar = true;
      turnos.delete(s.id);
      // Card aberto segura o turno. Cancelar sem fechá-lo deixaria a promessa
      // pendurada para sempre e o card na tela sem dono.
      for (const acao of mem.acoes) {
        if (acao.session_id === s.id && acao.aprovacao === "pendente") {
          acao.aprovacao = "negada";
          acao.decidido_por = "turno cancelado";
          cardsAbertos.get(acao.id)?.("negada");
          cardsAbertos.delete(acao.id);
          emitirFalso("aprovacao:decidida", {
            tipo: "aprovacao_decidida",
            tool_call_id: acao.id,
            node_id: s.node_id,
            decisao: "negada",
            decidido_por: "turno cancelado",
          });
        }
      }
      if (s.estado === "ocioso") return qualquer(undefined);
      gravarMensagemFalsa(s.id, "sistema", "Turno interrompido por você.");
      mudarEstadoFalso(s, "ocioso");
      return qualquer(undefined);
    }

    case "historico": {
      const todas = mem.mensagens.filter((m) => m.session_id === a.sessionId);
      return qualquer(todas.slice(-Number(a.limite ?? 200)));
    }

    case "acoes_da_sessao":
      return qualquer(mem.acoes.filter((c) => c.session_id === a.sessionId));

    // ---------------------------------------------------------- arquivos

    case "listar_pasta": {
      semearArquivos();
      const sub = String(a.sub ?? "").replace(/\/$/, "");
      const prefixo = sub ? `${sub}/` : "";
      const vistos = new Map<string, ItemArquivo>();
      for (const [caminho, conteudo] of mem.arquivos) {
        if (!caminho.startsWith(prefixo)) continue;
        const resto = caminho.slice(prefixo.length);
        const barra = resto.indexOf("/");
        if (barra === -1) {
          vistos.set(resto, {
            caminho,
            nome: resto,
            pasta: false,
            tamanho: conteudo.length,
          });
        } else {
          const nome = resto.slice(0, barra);
          vistos.set(nome, {
            caminho: prefixo + nome,
            nome,
            pasta: true,
            tamanho: 0,
          });
        }
      }
      const itens = [...vistos.values()].sort(
        (x, y) =>
          Number(y.pasta) - Number(x.pasta) ||
          x.nome.toLowerCase().localeCompare(y.nome.toLowerCase()),
      );
      return qualquer(itens);
    }

    case "ler_nota": {
      semearArquivos();
      const no = mem.nos.find((n) => n.id === a.nodeId);
      if (!no) throw { codigo: "nao_encontrado", mensagem: "nó não encontrado" };
      if (no.tipo !== "nota") throw { codigo: "invalido", mensagem: "esse nó não é uma nota" };
      const arquivo = `${no.nome.replace(/[/\\:*?"<>|]/g, "-").trim() || "nota"}.md`;
      return qualquer({ arquivo, conteudo: mem.arquivos.get(arquivo) ?? "" });
    }

    case "escrever_nota": {
      const no = mem.nos.find((n) => n.id === a.nodeId);
      if (!no) throw { codigo: "nao_encontrado", mensagem: "nó não encontrado" };
      const arquivo = `${no.nome.replace(/[/\\:*?"<>|]/g, "-").trim() || "nota"}.md`;
      mem.arquivos.set(arquivo, String(a.conteudo ?? ""));
      return qualquer(undefined);
    }

    case "decidir_aprovacao": {
      const acao = mem.acoes.find((c) => c.id === a.toolCallId);
      if (!acao || acao.aprovacao !== "pendente")
        throw { codigo: "nao_encontrado", mensagem: "não há o que decidir agora" };
      const s = mem.sessoes.find((k) => k.id === acao.session_id);
      const wsId = mem.nos.find((n) => n.id === s?.node_id)?.workspace_id ?? "";

      if (a.lembrar && a.decisao === "aprovada") {
        if (!FERRAMENTAS_QUE_ACEITAM_REGRA.includes(acao.ferramenta)) {
          throw {
            codigo: "invalido",
            mensagem: `${acao.ferramenta} pergunta sempre: uma licença permanente para isso valeria pela máquina toda`,
          };
        }
        if (!mem.regras.some((r) => r.workspace_id === wsId && r.ferramenta === acao.ferramenta)) {
          mem.regras.push({
            id: id(),
            workspace_id: wsId,
            ferramenta: acao.ferramenta,
            criado_em: agora(),
          });
        }
      }

      acao.aprovacao = a.decisao;
      acao.decidido_por = "usuario";
      // O banco antes de soltar o agente: gravar depois deixaria o arquivo no
      // disco antes de existir a linha que o autoriza.
      cardsAbertos.get(acao.id)?.(a.decisao);
      cardsAbertos.delete(acao.id);
      emitirFalso("aprovacao:decidida", {
        tipo: "aprovacao_decidida",
        tool_call_id: acao.id,
        node_id: s?.node_id ?? "",
        decisao: a.decisao,
        decidido_por: "usuario",
      });
      return qualquer(undefined);
    }

    case "aprovacoes_pendentes": {
      const s = mem.sessoes.find((k) => k.id === a.sessionId);
      return qualquer(
        mem.acoes
          .filter((c) => c.session_id === a.sessionId && c.aprovacao === "pendente")
          .map((c) => {
            const [resumo, detalhe] = descreverFerramenta(c.ferramenta, c.argumentos);
            return {
              tool_call_id: c.id,
              session_id: c.session_id,
              node_id: s?.node_id ?? "",
              ferramenta: c.ferramenta,
              resumo,
              detalhe,
              previa: previaDoConteudo(c.argumentos),
              criado_em: c.criado_em,
            };
          }),
      );
    }

    case "listar_regras":
      return qualquer(mem.regras.filter((r) => r.workspace_id === a.workspaceId));

    case "revogar_regra":
      mem.regras = mem.regras.filter((r) => r.id !== a.id);
      return qualquer(undefined);

    case "custo_do_workspace": {
      const daqui = mem.nos.filter((n) => n.workspace_id === a.workspaceId).map((n) => n.id);
      const por_no = mem.sessoes
        .filter((s) => daqui.includes(s.node_id))
        .map((s) => ({ node_id: s.node_id, custo: s.custo_total }));
      return qualquer({ total: por_no.reduce((t, c) => t + c.custo, 0), por_no });
    }

    default:
      throw { codigo: "invalido", mensagem: `comando desconhecido: ${comando}` };
  }
}
