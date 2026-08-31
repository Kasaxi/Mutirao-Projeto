import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Cabos } from "./canvas/Cabos";
import { NoView } from "./canvas/NoView";
import {
  enquadrar,
  limitar,
  telaParaMundo,
  ZOOM_MAX,
  ZOOM_MIN,
  type Tela,
} from "./canvas/viewport";
import { adiar } from "./lib/adiar";
import { escutar, ipc, modoNavegador } from "./lib/ipc";
import {
  ehErroIpc,
  formatarCusto,
  type Adaptador,
  type EstadoCanvas,
  type EstadoSessao,
  type EventoCusto,
  type No,
  type TipoNo,
  type Viewport,
} from "./lib/tipos";

type Arrasto =
  | { tipo: "pan"; inicio: Tela; vpInicial: Viewport }
  | { tipo: "no"; id: string; inicio: Tela; origem: { x: number; y: number } }
  | { tipo: "tamanho"; id: string; inicio: Tela; origem: { w: number; h: number } }
  | { tipo: "cabo"; de: string; cursor: { x: number; y: number } };

const MIN_LARGURA = 160;
const MIN_ALTURA = 100;

export default function App() {
  const [estado, setEstado] = useState<EstadoCanvas | null>(null);
  const [vp, setVp] = useState<Viewport>({ x: 0, y: 0, zoom: 1 });
  const [selecionado, setSelecionado] = useState<string | null>(null);
  const [arrasto, setArrasto] = useState<Arrasto | null>(null);
  const [aviso, setAviso] = useState<string | null>(null);
  // Estado de sessão por nó, só para o cabeçalho desenhar o ponto de atenção.
  // Quem sabe da conversa é cada <Conversa>; aqui mora só o resumo.
  const [estadosSessao, setEstadosSessao] = useState<Record<string, EstadoSessao>>({});
  const [custoTotal, setCustoTotal] = useState(0);
  // Quem está de fato respondendo. Vem do backend, não de uma constante daqui:
  // é ele que sabe se achou o Claude Code na máquina.
  const [agente, setAgente] = useState<{ adaptador: Adaptador; detalhe: string } | null>(null);

  useEffect(() => {
    let vivo = true;
    ipc
      .adaptadorEmUso()
      .then((a) => vivo && setAgente(a))
      .catch(() => {});
    return () => {
      vivo = false;
    };
  }, []);

  const areaRef = useRef<HTMLDivElement>(null);

  // Espelhos do estado para os listeners de janela. Sem eles, cada handler
  // enxergaria o valor congelado do render em que foi criado.
  const estadoRef = useRef<EstadoCanvas | null>(null);
  const vpRef = useRef<Viewport>(vp);
  const arrastoRef = useRef<Arrasto | null>(null);

  useEffect(() => {
    estadoRef.current = estado;
  }, [estado]);
  useEffect(() => {
    vpRef.current = vp;
  }, [vp]);

  // ---------------------------------------------------------------- carga

  useEffect(() => {
    (async () => {
      try {
        let lista = await ipc.listarWorkspaces();
        if (lista.length === 0) {
          await ipc.criarWorkspace("Meu primeiro mutirão", "");
          lista = await ipc.listarWorkspaces();
        }
        const primeiro = lista[0];
        if (!primeiro) return;
        const e = await ipc.abrirWorkspace(primeiro.id);
        setEstado(e);
        setVp(e.workspace.viewport);
      } catch (err) {
        setAviso(mensagem(err));
      }
    })();
  }, []);

  const gravarViewport = useMemo(
    () =>
      adiar((wsId: string, v: Viewport) => {
        ipc.salvarViewport(wsId, v.x, v.y, v.zoom).catch(() => {
          /* viewport é conforto, não dado: falha aqui não merece alarde */
        });
      }, 400),
    [],
  );

  useEffect(() => {
    if (estado) gravarViewport(estado.workspace.id, vp);
  }, [vp, estado, gravarViewport]);

  const nosPorId = useMemo(() => new Map((estado?.nos ?? []).map((n) => [n.id, n])), [estado]);

  const registrarEstadoSessao = useCallback((nodeId: string, novo: EstadoSessao) => {
    // Devolver o mesmo objeto quando nada muda evita render em cascata: o
    // callback que dispara isto é recriado a cada render do App.
    setEstadosSessao((m) => (m[nodeId] === novo ? m : { ...m, [nodeId]: novo }));
  }, []);

  // O custo do workspace inteiro, para a barra. Chega por evento no fim de
  // cada turno; a leitura inicial cobre o que já foi gasto em sessões antigas.
  useEffect(() => {
    if (!estado) return;
    const ws = estado.workspace.id;
    let vivo = true;
    let parar: (() => void) | null = null;

    ipc
      .custoDoWorkspace(ws)
      .then((c) => vivo && setCustoTotal(c.total))
      .catch(() => {});

    escutar<EventoCusto>("custo:atualizado", (p) => {
      if (p.workspace_id === ws) setCustoTotal(p.total);
    }).then((f) => (vivo ? (parar = f) : f()));

    return () => {
      vivo = false;
      parar?.();
    };
  }, [estado]);

  // --------------------------------------------------------------- ações

  const patch = useCallback((id: string, campos: Partial<No>) => {
    setEstado((e) =>
      e ? { ...e, nos: e.nos.map((n) => (n.id === id ? { ...n, ...campos } : n)) } : e,
    );
  }, []);

  const adicionar = useCallback(
    async (tipo: TipoNo) => {
      const est = estadoRef.current;
      if (!est) return;
      const area = areaRef.current;
      const centro = telaParaMundo(
        { x: (area?.clientWidth ?? 1200) / 2, y: (area?.clientHeight ?? 800) / 2 },
        vpRef.current,
      );
      // empurrãozinho aleatório para dois nós seguidos não empilharem exatamente
      const jitter = () => (Math.random() - 0.5) * 60;
      try {
        const n = await ipc.criarNo(
          est.workspace.id,
          tipo,
          "",
          Math.round(centro.x - 150 + jitter()),
          Math.round(centro.y - 100 + jitter()),
        );
        setEstado((e) => (e ? { ...e, nos: [...e.nos, n] } : e));
        setSelecionado(n.id);
      } catch (err) {
        setAviso(mensagem(err));
      }
    },
    [],
  );

  const remover = useCallback(async () => {
    const est = estadoRef.current;
    if (!selecionado || !est) return;
    const ehCabo = est.cabos.some((c) => c.id === selecionado);
    try {
      if (ehCabo) {
        await ipc.removerCabo(selecionado);
        setEstado((e) => (e ? { ...e, cabos: e.cabos.filter((c) => c.id !== selecionado) } : e));
      } else {
        await ipc.removerNo(selecionado);
        setEstado((e) =>
          e
            ? {
                ...e,
                nos: e.nos.filter((n) => n.id !== selecionado),
                cabos: e.cabos.filter(
                  (c) => c.de_node !== selecionado && c.para_node !== selecionado,
                ),
              }
            : e,
        );
      }
      setSelecionado(null);
    } catch (err) {
      setAviso(mensagem(err));
    }
  }, [selecionado]);

  const enquadrarTudo = useCallback(() => {
    const est = estadoRef.current;
    const area = areaRef.current;
    if (!est || !area) return;
    setVp(enquadrar(est.nos, area.clientWidth, area.clientHeight));
  }, []);

  // ---------------------------------------------------------- interações

  const posicaoNaArea = useCallback((e: { clientX: number; clientY: number }): Tela => {
    const r = areaRef.current?.getBoundingClientRect();
    return { x: e.clientX - (r?.left ?? 0), y: e.clientY - (r?.top ?? 0) };
  }, []);

  // Um gesto de arrasto registra os listeners UMA vez, no pointerdown, e os
  // remove no pointerup. A tentação é fazer isso num useEffect com o arrasto
  // na dependência — mas aí cada movimento recria o par de listeners, e o
  // pointerup chega a rodar duas vezes (criava dois cabos em vez de um).
  // O estado do gesto mora num ref; o useState existe só para desenhar.

  const soltarRef = useRef<(e: PointerEvent) => void>(() => {});
  const moverRef = useRef<(e: PointerEvent) => void>(() => {});

  const mover = useCallback(
    (e: PointerEvent) => {
      const a = arrastoRef.current;
      if (!a) return;
      const p = posicaoNaArea(e);
      const zoom = vpRef.current.zoom;

      switch (a.tipo) {
        case "pan":
          setVp({
            ...a.vpInicial,
            x: a.vpInicial.x + (p.x - a.inicio.x),
            y: a.vpInicial.y + (p.y - a.inicio.y),
          });
          break;
        case "no":
          patch(a.id, {
            x: a.origem.x + (p.x - a.inicio.x) / zoom,
            y: a.origem.y + (p.y - a.inicio.y) / zoom,
          });
          break;
        case "tamanho":
          patch(a.id, {
            w: Math.max(MIN_LARGURA, a.origem.w + (p.x - a.inicio.x) / zoom),
            h: Math.max(MIN_ALTURA, a.origem.h + (p.y - a.inicio.y) / zoom),
          });
          break;
        case "cabo": {
          const atualizado: Arrasto = { ...a, cursor: telaParaMundo(p, vpRef.current) };
          arrastoRef.current = atualizado;
          setArrasto(atualizado);
          break;
        }
      }
    },
    [patch, posicaoNaArea],
  );

  const soltar = useCallback(async (e: PointerEvent) => {
    // Desarma primeiro: qualquer caminho abaixo pode aguardar IPC, e um
    // segundo pointerup no meio disso duplicaria o efeito.
    window.removeEventListener("pointermove", moverRef.current);
    window.removeEventListener("pointerup", soltarRef.current);
    const a = arrastoRef.current;
    arrastoRef.current = null;
    setArrasto(null);
    if (!a) return;

    if (a.tipo === "no" || a.tipo === "tamanho") {
      // só agora o banco fica sabendo — durante o arrasto seriam
      // dezenas de gravações por segundo, sem nenhum ganho
      const n = estadoRef.current?.nos.find((k) => k.id === a.id);
      if (n) {
        try {
          await ipc.moverNo(n.id, n.x, n.y, n.w, n.h);
        } catch (err) {
          setAviso(mensagem(err));
        }
      }
    }

    if (a.tipo === "cabo") {
      const alvo = document
        .elementFromPoint(e.clientX, e.clientY)
        ?.closest("[data-no-id]") as HTMLElement | null;
      const paraId = alvo?.dataset.noId;
      const est = estadoRef.current;
      if (paraId && est && paraId !== a.de) {
        const de = est.nos.find((n) => n.id === a.de);
        const para = est.nos.find((n) => n.id === paraId);
        // Regra: ligar em nota é leitura; ligar em agente é conversa.
        const tipo = para?.tipo === "nota" || de?.tipo === "nota" ? "le_nota" : "fala_com";
        try {
          const c = await ipc.criarCabo(est.workspace.id, a.de, paraId, tipo);
          setEstado((s) => (s ? { ...s, cabos: [...s.cabos, c] } : s));
        } catch (err) {
          setAviso(mensagem(err));
        }
      }
    }
  }, []);

  useEffect(() => {
    moverRef.current = mover;
    soltarRef.current = (e) => void soltar(e);
  }, [mover, soltar]);

  const iniciarArrasto = useCallback((a: Arrasto) => {
    arrastoRef.current = a;
    setArrasto(a);
    window.addEventListener("pointermove", moverRef.current);
    window.addEventListener("pointerup", soltarRef.current);
  }, []);

  useEffect(() => {
    // Se o componente sair no meio de um arrasto, não deixe listener órfão.
    return () => {
      window.removeEventListener("pointermove", moverRef.current);
      window.removeEventListener("pointerup", soltarRef.current);
    };
  }, []);

  const aoRolar = (e: React.WheelEvent) => {
    const p = posicaoNaArea(e);
    // Convenção de ferramenta de design: rolar move, ctrl/⌘ + rolar amplia.
    // O gesto de pinça do trackpad chega como wheel com ctrlKey ligado.
    if (e.ctrlKey || e.metaKey) {
      const fator = Math.exp(-e.deltaY * 0.0025);
      setVp((v) => {
        const zoom = limitar(v.zoom * fator, ZOOM_MIN, ZOOM_MAX);
        const k = zoom / v.zoom;
        return { zoom, x: p.x - (p.x - v.x) * k, y: p.y - (p.y - v.y) * k };
      });
    } else {
      setVp((v) => ({ ...v, x: v.x - e.deltaX, y: v.y - e.deltaY }));
    }
  };

  const aoApertarFundo = (e: React.PointerEvent) => {
    if (e.button !== 0 && e.button !== 1) return;
    setSelecionado(null);
    iniciarArrasto({ tipo: "pan", inicio: posicaoNaArea(e), vpInicial: vpRef.current });
  };

  useEffect(() => {
    const tecla = (e: KeyboardEvent) => {
      const alvo = e.target as HTMLElement | null;
      if (alvo && (alvo.tagName === "INPUT" || alvo.tagName === "TEXTAREA")) return;
      if (e.key === "Delete" || e.key === "Backspace") {
        e.preventDefault();
        void remover();
      }
      if (e.key === "Escape") setSelecionado(null);
      if (e.key === "0") enquadrarTudo();
    };
    window.addEventListener("keydown", tecla);
    return () => window.removeEventListener("keydown", tecla);
  }, [remover, enquadrarTudo]);

  // ------------------------------------------------------------- desenho

  const provisorio =
    arrasto?.tipo === "cabo"
      ? (() => {
          const de = nosPorId.get(arrasto.de);
          if (!de) return null;
          // sai da porta (lateral direita), igual ao cabo definitivo
          return { de: { x: de.x + de.w, y: de.y + de.h / 2 }, para: arrasto.cursor };
        })()
      : null;

  return (
    <div className="app">
      <header className="barra">
        <div className="marca">
          Mutirão
          <span className="versao">M2</span>
        </div>

        <div className="ferramentas">
          <button onClick={() => adicionar("agente")}>Agente</button>
          <button onClick={() => adicionar("nota")}>Nota</button>
          <button onClick={() => adicionar("arquivos")}>Arquivos</button>
          <button onClick={() => adicionar("portal")}>Portal</button>
          <span className="divisor" />
          <button onClick={enquadrarTudo} title="Tecla 0">
            Enquadrar
          </button>
          <button onClick={remover} disabled={!selecionado} title="Delete">
            Remover
          </button>
        </div>

        <div className="direita">
          {agente?.adaptador === "falso" && (
            <span
              className="selo alerta"
              title={`As respostas vêm de um roteiro, não de um modelo. ${agente.detalhe}`}
            >
              adaptador falso
            </span>
          )}
          {agente?.adaptador === "claude" && (
            <span className="selo ok" title={`Claude Code ${agente.detalhe}`}>
              claude code
            </span>
          )}
          {modoNavegador && (
            <span className="selo" title="Sem backend: nada é gravado em disco">
              modo navegador
            </span>
          )}
          <span className="custo-total" title="Custo acumulado deste workspace, em dólar">
            {formatarCusto(custoTotal)}
          </span>
          <span className="zoom">{Math.round(vp.zoom * 100)}%</span>
        </div>
      </header>

      <div
        ref={areaRef}
        className={`area${arrasto?.tipo === "pan" ? " arrastando" : ""}`}
        onWheel={aoRolar}
        onPointerDown={aoApertarFundo}
        style={{
          backgroundSize: `${24 * vp.zoom}px ${24 * vp.zoom}px`,
          backgroundPosition: `${vp.x}px ${vp.y}px`,
        }}
      >
        <div
          className="mundo"
          style={{ transform: `translate(${vp.x}px, ${vp.y}px) scale(${vp.zoom})` }}
        >
          <Cabos
            cabos={estado?.cabos ?? []}
            nos={nosPorId}
            selecionado={selecionado}
            aoSelecionar={setSelecionado}
            provisorio={provisorio}
          />

          {(estado?.nos ?? []).map((n) => (
            <NoView
              key={n.id}
              no={n}
              selecionado={selecionado === n.id}
              estadoSessao={estadosSessao[n.id]}
              aoMudarEstadoSessao={(e) => registrarEstadoSessao(n.id, e)}
              aoSelecionar={() => {
                setSelecionado(n.id);
                void ipc
                  .trazerParaFrente(n.id)
                  .then((z) => patch(n.id, { z }))
                  .catch(() => {});
              }}
              aoArrastar={(e) =>
                iniciarArrasto({
                  tipo: "no",
                  id: n.id,
                  inicio: posicaoNaArea(e),
                  origem: { x: n.x, y: n.y },
                })
              }
              aoRedimensionar={(e) =>
                iniciarArrasto({
                  tipo: "tamanho",
                  id: n.id,
                  inicio: posicaoNaArea(e),
                  origem: { w: n.w, h: n.h },
                })
              }
              aoLigar={(e) =>
                iniciarArrasto({
                  tipo: "cabo",
                  de: n.id,
                  cursor: telaParaMundo(posicaoNaArea(e), vpRef.current),
                })
              }
              aoRenomear={(nome) => {
                patch(n.id, { nome });
                ipc.renomearNo(n.id, nome).catch((err) => setAviso(mensagem(err)));
              }}
            />
          ))}
        </div>

        {estado && estado.nos.length === 0 && (
          <div className="vazio">
            <p>Canvas vazio.</p>
            <p className="fraco">Use a barra de cima para colocar o primeiro nó.</p>
          </div>
        )}
      </div>

      <footer className="rodape">
        <span>arrastar fundo: mover · ctrl + rolar: zoom · duplo clique no título: renomear</span>
        {aviso && (
          <span className="aviso" onClick={() => setAviso(null)} role="alert">
            {aviso} <b>×</b>
          </span>
        )}
      </footer>
    </div>
  );
}

function mensagem(err: unknown): string {
  if (ehErroIpc(err)) return err.mensagem;
  if (err instanceof Error) return err.message;
  return "Algo deu errado.";
}
