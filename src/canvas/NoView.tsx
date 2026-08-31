import { useEffect, useRef, useState } from "react";
import { ROTULO_NO, type No } from "../lib/tipos";

interface Props {
  no: No;
  selecionado: boolean;
  aoSelecionar: () => void;
  aoArrastar: (e: React.PointerEvent) => void;
  aoRedimensionar: (e: React.PointerEvent) => void;
  aoLigar: (e: React.PointerEvent) => void;
  aoRenomear: (nome: string) => void;
}

export function NoView({
  no,
  selecionado,
  aoSelecionar,
  aoArrastar,
  aoRedimensionar,
  aoLigar,
  aoRenomear,
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
      </div>

      <div className="no-corpo">
        <Corpo no={no} />
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
 * Conteúdo por tipo. No M0 são maquetes: mostram o lugar que a coisa vai
 * ocupar e a que marco ela pertence. Nada aqui finge estar funcionando.
 */
function Corpo({ no }: { no: No }) {
  switch (no.tipo) {
    case "agente":
      return (
        <div className="maquete conversa">
          <div className="bolha agente">Pronto para começar.</div>
          <div className="bolha usuario">…</div>
          <span className="marco">Face conversa · M1</span>
        </div>
      );
    case "nota":
      return (
        <div className="maquete nota">
          <p># Briefing</p>
          <p className="fraco">Memória compartilhada entre os agentes ligados.</p>
          <span className="marco">Markdown no disco · M2</span>
        </div>
      );
    case "arquivos":
      return (
        <div className="maquete arvore">
          <p>📁 contratos/</p>
          <p className="recuo">📄 minuta.docx</p>
          <p className="recuo">📄 anexo-i.pdf</p>
          <p>📁 planilhas/</p>
          <span className="marco">Árvore real · M2</span>
        </div>
      );
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
