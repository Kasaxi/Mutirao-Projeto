import type {
  Adaptador,
  Cabo,
  ChamadaFerramenta,
  CustoDoNo,
  EstadoCanvas,
  EstadoSessao,
  EventoAgente,
  Mensagem,
  No,
  PapelMensagem,
  Sessao,
  TipoCabo,
  TipoNo,
  Workspace,
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

  abrirSessao: (nodeId: string, adaptador: Adaptador) =>
    chamar<Sessao>("abrir_sessao", { nodeId, adaptador }),

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

/**
 * Qual adaptador está rodando de verdade. No M1 é sempre o falso: roteiro em
 * vez de modelo. A interface mostra isso na barra, porque uma maquete que não
 * se anuncia é uma mentira.
 */
export const ADAPTADOR_ATUAL: Adaptador = "falso";

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
};

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
      {
        tipo: "ferramenta_pedida",
        id: "fer_1",
        nome: "ler_arquivo",
        argumentos: { caminho: "contrato-v3.docx" },
      },
      {
        tipo: "ferramenta_concluida",
        id: "fer_1",
        resultado: { bytes: 48213, truncado: false },
        erro: null,
      },
      { tipo: "texto_parcial", delta: "Li o material" },
      { tipo: "texto_parcial", delta: " sobre o documento." },
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

  for (const evento of eventos) {
    await new Promise((r) => setTimeout(r, atrasoMs));
    if (controle.parar) return;

    emitirFalso("sessao:evento", { tipo: "sessao_evento", session_id: s.id, evento });

    switch (evento.tipo) {
      case "sessao_iniciada":
        s.sessao_externa_id = evento.id_externo;
        break;
      case "texto_parcial":
        acumulado += evento.delta;
        break;
      case "ferramenta_pedida":
        mem.acoes.push({
          id: `${s.id}:${evento.id}`,
          session_id: s.id,
          ferramenta: evento.nome,
          argumentos: evento.argumentos,
          resultado: null,
          erro: null,
          aprovacao: "automatica",
          decidido_por: null,
          criado_em: agora(),
        });
        break;
      case "ferramenta_concluida": {
        const acao = mem.acoes.find((a) => a.id === `${s.id}:${evento.id}`);
        if (acao) {
          acao.resultado = evento.resultado;
          acao.erro = evento.erro;
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
        adaptador: a.adaptador,
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
