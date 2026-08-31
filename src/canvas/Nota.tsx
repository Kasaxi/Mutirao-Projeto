import { useEffect, useMemo, useRef, useState } from "react";
import { adiar } from "../lib/adiar";
import { ipc } from "../lib/ipc";
import { ehErroIpc, type No } from "../lib/tipos";

/**
 * A nota é um `.md` na pasta do workspace, não um campo do banco.
 *
 * A diferença importa: o usuário abre no editor dele, manda por e-mail,
 * versiona, e o agente lê o mesmo arquivo. "Memória do app" seria uma coisa
 * que só existe enquanto o app existir.
 */
export function Nota({ no }: { no: No }) {
  const [texto, setTexto] = useState("");
  const [arquivo, setArquivo] = useState("");
  const [carregando, setCarregando] = useState(true);
  const [erro, setErro] = useState<string | null>(null);
  const [gravando, setGravando] = useState(false);

  useEffect(() => {
    let vivo = true;
    setCarregando(true);
    ipc
      .lerNota(no.id)
      .then((n) => {
        if (!vivo) return;
        setTexto(n.conteudo);
        setArquivo(n.arquivo);
      })
      .catch((e) => vivo && setErro(mensagem(e)))
      .finally(() => vivo && setCarregando(false));
    return () => {
      vivo = false;
    };
  }, [no.id]);

  // Grava por pausa, não por tecla. Escrever no disco a cada caractere é
  // desperdício e, com o agente lendo o mesmo arquivo, é também uma fonte de
  // leitura pela metade.
  const gravar = useMemo(
    () =>
      adiar((id: string, conteudo: string) => {
        setGravando(true);
        ipc
          .escreverNota(id, conteudo)
          .catch((e) => setErro(mensagem(e)))
          .finally(() => setGravando(false));
      }, 600),
    [],
  );

  const primeiroRender = useRef(true);
  useEffect(() => {
    // Não regravar o que acabou de ser lido do disco.
    if (primeiroRender.current) {
      primeiroRender.current = false;
      return;
    }
    if (!carregando) gravar(no.id, texto);
  }, [texto, carregando, no.id, gravar]);

  return (
    <div className="nota-viva">
      <textarea
        className="nota-campo"
        value={carregando ? "" : texto}
        placeholder={carregando ? "abrindo…" : "# Título\n\nEscreva aqui. Vira arquivo na pasta."}
        spellCheck={false}
        onChange={(e) => setTexto(e.target.value)}
        onPointerDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}
        onWheel={(e) => {
          if (e.ctrlKey || e.metaKey) return;
          const el = e.currentTarget;
          const sobra = el.scrollHeight - el.clientHeight - el.scrollTop;
          if ((e.deltaY > 0 && sobra > 1) || (e.deltaY < 0 && el.scrollTop > 0)) {
            e.stopPropagation();
          }
        }}
      />
      <div className="nota-rodape">
        <span className="nota-arquivo" title="Arquivo na pasta do workspace">
          {arquivo || "—"}
        </span>
        {gravando && <span className="fraco">gravando…</span>}
        {erro && (
          <span className="aviso" onClick={() => setErro(null)}>
            {erro}
          </span>
        )}
      </div>
    </div>
  );
}

function mensagem(e: unknown): string {
  if (ehErroIpc(e)) return e.mensagem;
  if (e instanceof Error) return e.message;
  return "Algo deu errado.";
}
