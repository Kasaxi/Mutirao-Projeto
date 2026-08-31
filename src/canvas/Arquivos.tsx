import { useCallback, useEffect, useState } from "react";
import { ipc } from "../lib/ipc";
import { ehErroIpc, type ItemArquivo, type No } from "../lib/tipos";

/**
 * A pasta do workspace, lida do disco de verdade.
 *
 * Navega por subpastas em vez de abrir a árvore inteira de uma vez: uma pasta
 * de obra tem milhares de arquivos, e listar tudo para mostrar dez é trabalho
 * jogado fora.
 */
export function Arquivos({ no }: { no: No }) {
  const [caminho, setCaminho] = useState("");
  const [itens, setItens] = useState<ItemArquivo[]>([]);
  const [erro, setErro] = useState<string | null>(null);

  const carregar = useCallback(
    (sub: string) => {
      ipc
        .listarPasta(no.workspace_id, sub)
        .then((v) => {
          setItens(v);
          setErro(null);
        })
        .catch((e) => setErro(mensagem(e)));
    },
    [no.workspace_id],
  );

  useEffect(() => carregar(caminho), [carregar, caminho]);

  const subir = () => setCaminho((c) => c.split("/").slice(0, -1).join("/"));

  return (
    <div className="arvore">
      <div className="arvore-caminho">
        {caminho ? (
          <button className="aba" type="button" onClick={subir} title="Voltar uma pasta">
            ← {caminho}
          </button>
        ) : (
          <span className="fraco">pasta do workspace</span>
        )}
        <button
          className="aba"
          type="button"
          onClick={() => carregar(caminho)}
          title="Reler a pasta"
        >
          reler
        </button>
      </div>

      <div
        className="arvore-lista"
        onWheel={(e) => {
          if (e.ctrlKey || e.metaKey) return;
          const el = e.currentTarget;
          const sobra = el.scrollHeight - el.clientHeight - el.scrollTop;
          if ((e.deltaY > 0 && sobra > 1) || (e.deltaY < 0 && el.scrollTop > 0)) {
            e.stopPropagation();
          }
        }}
      >
        {erro && <p className="aviso">{erro}</p>}
        {!erro && itens.length === 0 && <p className="fraco">Pasta vazia.</p>}
        {itens.map((i) =>
          i.pasta ? (
            <button
              key={i.caminho}
              className="arvore-item pasta"
              type="button"
              onClick={() => setCaminho(i.caminho)}
              onPointerDown={(e) => e.stopPropagation()}
            >
              <span className="arvore-icone">📁</span>
              <span className="arvore-nome">{i.nome}</span>
            </button>
          ) : (
            <div key={i.caminho} className="arvore-item">
              <span className="arvore-icone">📄</span>
              <span className="arvore-nome">{i.nome}</span>
              <span className="arvore-tamanho">{tamanho(i.tamanho)}</span>
            </div>
          ),
        )}
      </div>
    </div>
  );
}

function tamanho(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} kB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function mensagem(e: unknown): string {
  if (ehErroIpc(e)) return e.mensagem;
  if (e instanceof Error) return e.message;
  return "Algo deu errado.";
}
