import type { Cabo, EstadoCanvas, No, TipoCabo, TipoNo, Workspace } from "./tipos";

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
};

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
};

const id = () => crypto.randomUUID();
const agora = () => Date.now();

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

    default:
      throw { codigo: "invalido", mensagem: `comando desconhecido: ${comando}` };
  }
}
