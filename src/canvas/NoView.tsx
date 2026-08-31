import { useEffect, useRef, useState } from "react";
import {
  pedeAtencao,
  ROTULO_ESTADO,
  ROTULO_NO,
  type EstadoSessao,
  type No,
  type Papel,
} from "../lib/tipos";
import { Arquivos } from "./Arquivos";
import { Conversa } from "./Conversa";
import { Nota } from "./Nota";

interface Props {
  no: No;
  selecionado: boolean;
  /** Só para nó de agente. `undefined` = ainda não abriu sessão. */
  estadoSessao?: EstadoSessao;
  aoSelecionar: () => void;
  aoArrastar: (e: React.PointerEvent) => void;
  aoRedimensionar: (e: React.PointerEvent) => void;
  aoLigar: (e: React.PointerEvent) => void;
  aoRenomear: (nome: string) => void;
  aoMudarEstadoSessao?: (estado: EstadoSessao) => void;
  /** Nome de cada nó, para a bolha vinda de outro nó dizer de quem ela é. */
  nomesDosNos?: Record<string, string>;
  /** A biblioteca inteira, para o seletor de papel do cabeçalho. */
  papeis?: Papel[];
  aoTrocarPapel?: (roleId: string | null) => void;
}

export function NoView({
  no,
  selecionado,
  estadoSessao,
  aoSelecionar,
  aoArrastar,
  aoRedimensionar,
  aoLigar,
  aoRenomear,
  aoMudarEstadoSessao,
  nomesDosNos,
  papeis,
  aoTrocarPapel,
}: Props) {
  const [editando, setEditando] = useState(false);

  return (
    <div
      data-no-id={no.id}
      className={`no no-${no.tipo}${selecionado ? " selecionado" : ""}`}
      style={{ left: no.x, top: no.y, width: no.w, height: no.h }}
      // stopPropagation aqui é o que impede o canvas de entender um clique no
      // nó como clique no fundo (que limparia a seleção e começaria a mover a cena).
      onPointerDown={(e) => {
        e.stopPropagation();
        aoSelecionar();
      }}
    >
      <div
        className="no-cabecalho"
        // O cabeçalho para a propagação para arrastar, então precisa selecionar
        // por conta própria: sem isto, arrastar um nó não o selecionava.
        onPointerDown={(e) => {
          e.stopPropagation();
          aoSelecionar();
          aoArrastar(e);
        }}
        onDoubleClick={() => setEditando(true)}
      >
        <span className="no-tipo">{ROTULO_NO[no.tipo]}</span>
        {editando ? (
          <CampoNome
            valor={no.nome}
            aoConfirmar={(v) => {
              setEditando(false);
              if (v !== no.nome) aoRenomear(v);
            }}
          />
        ) : (
          <span className="no-nome" title="Dois cliques para renomear">
            {no.nome}
          </span>
        )}

        {no.tipo === "agente" && papeis && aoTrocarPapel && (
          <SeletorPapel
            papeis={papeis}
            atual={no.role_id}
            recrutado={no.recrutado_por !== null}
            aoTrocar={aoTrocarPapel}
          />
        )}

        {estadoSessao && (
          <span
            className={`sinal ${estadoSessao}${pedeAtencao(estadoSessao) ? " atencao" : ""}`}
            title={ROTULO_ESTADO[estadoSessao]}
            aria-label={`Estado: ${ROTULO_ESTADO[estadoSessao]}`}
          />
        )}
      </div>

      <div className="no-corpo">
        <Corpo no={no} aoMudarEstadoSessao={aoMudarEstadoSessao} nomesDosNos={nomesDosNos} />
      </div>

      {/* porta de ligação — arrastar daqui até outro nó cria um cabo */}
      <button
        className="porta"
        title="Arraste até outro nó para ligar"
        aria-label="Ligar a outro nó"
        onPointerDown={(e) => {
          e.stopPropagation();
          aoLigar(e);
        }}
      />

      <div
        className="alca-tamanho"
        role="presentation"
        onPointerDown={(e) => {
          e.stopPropagation();
          aoRedimensionar(e);
        }}
      />
    </div>
  );
}

function CampoNome({ valor, aoConfirmar }: { valor: string; aoConfirmar: (v: string) => void }) {
  const ref = useRef<HTMLInputElement>(null);
  const [v, setV] = useState(valor);

  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);

  return (
    <input
      ref={ref}
      className="no-nome-editando"
      value={v}
      onChange={(e) => setV(e.target.value)}
      onPointerDown={(e) => e.stopPropagation()}
      onBlur={() => aoConfirmar(v.trim() || valor)}
      onKeyDown={(e) => {
        if (e.key === "Enter") aoConfirmar(v.trim() || valor);
        if (e.key === "Escape") aoConfirmar(valor);
      }}
    />
  );
}

/**
 * Conteúdo por tipo. Onde ainda é maquete, ela diz a que marco pertence —
 * nada aqui finge estar funcionando. O agente saiu da maquete no M1.
 */
function Corpo({
  no,
  aoMudarEstadoSessao,
  nomesDosNos,
}: {
  no: No;
  aoMudarEstadoSessao?: (estado: EstadoSessao) => void;
  /** Nome de cada nó, para a bolha vinda de outro nó dizer de quem ela é. */
  nomesDosNos?: Record<string, string>;
}) {
  switch (no.tipo) {
    case "agente":
      return <Conversa no={no} aoMudarEstado={aoMudarEstadoSessao} nomesDosNos={nomesDosNos} />;
    case "nota":
      return <Nota no={no} />;
    case "arquivos":
      return <Arquivos no={no} />;
    case "portal":
      return (
        <div className="maquete portal">
          <div className="barra-url">localhost:3000</div>
          <span className="marco">WebView2 + CDP · M5</span>
        </div>
      );
    case "forma":
      return <div className="maquete forma" />;
  }
}

/**
 * O papel do nó, no cabeçalho e trocável ali mesmo.
 *
 * Um `<select>` e não um menu bonito: papel é escolha entre poucas opções
 * conhecidas, que é exatamente o que um select faz bem — e ele já vem com
 * teclado, leitor de tela e o comportamento que o sistema operacional dá.
 *
 * "sem papel" continua sendo uma opção de verdade. Todo nó criado até o M4
 * está assim, e forçar uma escolha na primeira abertura seria mudar o que o
 * usuário já tinha sem ele pedir.
 */
function SeletorPapel({
  papeis,
  atual,
  recrutado,
  aoTrocar,
}: {
  papeis: Papel[];
  atual: string | null;
  recrutado: boolean;
  aoTrocar: (roleId: string | null) => void;
}) {
  const papel = papeis.find((p) => p.id === atual);
  return (
    <select
      className={`no-papel${papel ? "" : " vazio"}`}
      value={atual ?? ""}
      title={
        papel
          ? `${papel.nome}: ${papel.prompt.split("\n")[0]}`
          : "Sem papel — prompt padrão e todas as ferramentas"
      }
      // O cabeçalho arrasta o nó; sem parar aqui, abrir o select viraria um
      // arrasto de um pixel e o menu fecharia na cara do usuário.
      onPointerDown={(e) => e.stopPropagation()}
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => aoTrocar(e.target.value || null)}
    >
      <option value="">sem papel</option>
      {papeis.map((p) => (
        <option key={p.id} value={p.id}>
          {p.nome}
        </option>
      ))}
      {/* Quem recrutou este nó foi outro agente. Vale dizer: um nó que
          apareceu sozinho no canvas é a coisa mais estranha do M4 até você
          entender de onde ele veio. */}
      {recrutado && <option disabled>— recrutado por outro agente —</option>}
    </select>
  );
}
