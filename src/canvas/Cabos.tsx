import type { Cabo, No, TipoCabo } from "../lib/tipos";

const COR: Record<TipoCabo, string> = {
  fala_com: "var(--cabo-fala)",
  le_nota: "var(--cabo-le)",
  escreve_nota: "var(--cabo-escreve)",
};

/**
 * Onde o cabo encosta em cada nó. Ligar centro a centro parece certo no
 * papel e some na tela: o traço fica escondido embaixo dos próprios nós.
 * Encostamos nas laterais, escolhendo o lado pela posição relativa.
 */
function ancoras(a: No, b: No) {
  const aDireita = b.x + b.w / 2 >= a.x + a.w / 2;
  return {
    de: { x: aDireita ? a.x + a.w : a.x, y: a.y + a.h / 2 },
    para: { x: aDireita ? b.x : b.x + b.w, y: b.y + b.h / 2 },
  };
}

/**
 * Curva de Bézier horizontal entre dois nós. A alça cresce com a distância,
 * até um teto — sem teto, nós distantes viram espaguete.
 */
export function caminho(a: { x: number; y: number }, b: { x: number; y: number }): string {
  const dx = Math.abs(b.x - a.x);
  const alca = Math.min(Math.max(dx * 0.45, 40), 220);
  const sentido = b.x >= a.x ? 1 : -1;
  return `M ${a.x} ${a.y} C ${a.x + alca * sentido} ${a.y}, ${b.x - alca * sentido} ${b.y}, ${b.x} ${b.y}`;
}

/**
 * Um recado viajando por um cabo. `de`/`para` são ids de nó no sentido do
 * recado — que pode ser o contrário do sentido em que o cabo foi desenhado,
 * porque um cabo `fala_com` vale nos dois sentidos.
 */
export interface Recado {
  chave: string;
  de: string;
  para: string;
  tipo: "pedido" | "aviso";
}

interface Props {
  cabos: Cabo[];
  nos: Map<string, No>;
  selecionado: string | null;
  aoSelecionar: (id: string) => void;
  provisorio: { de: { x: number; y: number }; para: { x: number; y: number } } | null;
  /** Recados em trânsito agora. Cada um acende o cabo por alguns segundos. */
  recados?: Recado[];
}

export function Cabos({ cabos, nos, selecionado, aoSelecionar, provisorio, recados }: Props) {
  const emTransito = new Map<string, Recado>();
  for (const r of recados ?? []) {
    emTransito.set(`${r.de}|${r.para}`, r);
    emTransito.set(`${r.para}|${r.de}`, r);
  }

  return (
    <svg className="cabos" width="1" height="1" aria-hidden="true">
      {cabos.map((c) => {
        const a = nos.get(c.de_node);
        const b = nos.get(c.para_node);
        if (!a || !b) return null;
        const { de, para } = ancoras(a, b);
        const d = caminho(de, para);
        const ativo = selecionado === c.id;
        const recado = emTransito.get(`${c.de_node}|${c.para_node}`);
        return (
          <g
            key={c.id}
            className={`cabo${ativo ? " ativo" : ""}${recado ? " falando" : ""}`}
            data-cabo-id={c.id}
            data-de={c.de_node}
            data-para={c.para_node}
            data-falando={recado ? recado.tipo : undefined}
          >
            {/* trilho invisível e grosso: dá área de clique sem engrossar o desenho */}
            <path
              d={d}
              stroke="transparent"
              strokeWidth={16}
              fill="none"
              style={{ pointerEvents: "stroke", cursor: "pointer" }}
              onPointerDown={(e) => {
                e.stopPropagation();
                aoSelecionar(c.id);
              }}
            />
            <path
              d={d}
              stroke={COR[c.tipo]}
              strokeWidth={ativo ? 2.5 : 1.5}
              strokeDasharray={c.tipo === "fala_com" ? undefined : "6 5"}
              fill="none"
            />

            {/* O recado em trânsito: um traço curto que corre pelo cabo, no
                sentido do recado. Sem isto a ponte é mágica — dois nós
                conversam e a única pista é o texto aparecendo em outro lugar.
                O tracejado que anda é o mesmo truque do cabo de rede piscando:
                diz "passou coisa por aqui" sem pedir atenção. */}
            {recado && (
              <path
                className="cabo-recado"
                d={recado.de === c.de_node ? d : caminho(para, de)}
                stroke={COR[c.tipo]}
                strokeWidth={3}
                strokeDasharray="10 90"
                strokeLinecap="round"
                fill="none"
              />
            )}
          </g>
        );
      })}

      {provisorio && (
        <path
          d={caminho(provisorio.de, provisorio.para)}
          stroke="var(--acento)"
          strokeWidth={2}
          strokeDasharray="4 4"
          fill="none"
        />
      )}
    </svg>
  );
}
