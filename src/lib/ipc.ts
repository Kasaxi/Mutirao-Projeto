import type {
  Adaptador,
  Cabo,
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
  PedidoAprovacao,
  RegraAprovacao,
  Sessao,
  TipoCabo,
  TipoNo,
  Workspace,
} from "./tipos";
import { FERRAMENTAS_QUE_ACEITAM_REGRA } from "./tipos";

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
  /** Sistema de arquivos de mentira: caminho relativo -> conteúdo. */
  arquivos: new Map<string, string>(),
};

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
): Mensagem {
  const m: Mensagem = {
    id: id(),
    session_id: sessionId,
    papel,
    origem_node: null,
    conteudo,
    tokens,
    custo,
    trace_id: null,
    criado_em: agora(),
  };
  mem.mensagens.push(m);
  return m;
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
        const precisa = PEDEM_LICENCA.includes(evento.nome);
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

function cabo(ws: string, de: string, para: string, tipo: TipoCabo): Cabo {
  return { id: id(), workspace_id: ws, de_node: de, para_node: para, tipo, criado_em: agora() };
}

async function falso<T>(comando: string, a: Record<string, any>): Promise<T> {
  semear();
  // O IPC de verdade serializa tudo que atravessa a fronteira, então o front
  // nunca compartilha objeto com o backend. O falso precisa imitar isso: sem
  // a cópia, o `push` daqui e o append no estado do React viram o MESMO
  // cabo contado duas vezes — foi exatamente esse o bug que apareceu no
  // teste de fumaça.
  const qualquer = (v: unknown) => structuredClone(v) as T;

  switch (comando) {
    case "listar_workspaces":
      return qualquer(mem.workspaces);

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
