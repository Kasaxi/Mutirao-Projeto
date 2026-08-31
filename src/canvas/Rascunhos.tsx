import { useCallback, useEffect, useState } from "react";
import { ipc } from "../lib/ipc";
import {
  ehErroIpc,
  ROTULO_ENSAIO,
  type Ensaio,
  type LadoDoConflito,
  type PreviaPublicacao,
} from "../lib/tipos";

/**
 * A barra de rascunhos e a tela de publicar.
 *
 * **Nenhuma palavra de Git aparece aqui.** Não é estilo: é a `Decisão 3` do
 * `ARQUITETURA.md`, e ela vale até nas mensagens de erro. O usuário vê
 * "Rascunho 2", "Publicar" e "o que muda"; branch, commit e merge ficam do
 * lado do Rust.
 *
 * A tela de publicar mostra **antes**, nunca desfaz depois. É o mesmo padrão
 * do card de aprovação do M2, e pelo mesmo motivo: publicar reescreve arquivos
 * na pasta de alguém, e "desfazer" só existe para quem sabe o que procurar.
 */

interface Props {
  workspaceId: string;
  /** `null` = trabalhando na pasta de verdade. */
  ativo: string | null;
  /** Sem histórico não há rascunho — máquina sem Git, ou workspace antigo. */
  temHistorico: boolean;
  aoMudar: () => void;
  aoAvisar: (texto: string) => void;
}

export function Rascunhos({ workspaceId, ativo, temHistorico, aoMudar, aoAvisar }: Props) {
  const [lista, setLista] = useState<Ensaio[]>([]);
  const [publicando, setPublicando] = useState<{ ensaio: Ensaio; previa: PreviaPublicacao } | null>(
    null,
  );

  const recarregar = useCallback(async () => {
    try {
      setLista(await ipc.listarRascunhos(workspaceId));
    } catch {
      /* a lista volta na próxima abertura; não vale um alarme */
    }
  }, [workspaceId]);

  useEffect(() => {
    void recarregar();
  }, [recarregar, ativo]);

  const abertos = lista.filter((e) => e.estado === "aberto");
  const emFoco = lista.find((e) => e.id === ativo) ?? null;

  const criar = useCallback(async () => {
    const nome = window.prompt("Nome do rascunho:", `Rascunho ${abertos.length + 1}`);
    if (!nome?.trim()) return;
    try {
      const novo = await ipc.criarRascunho(workspaceId, nome.trim());
      // Criar e já entrar nele: quem cria um rascunho quer trabalhar nele, e
      // um passo a mais só existiria para o usuário esquecer de dar.
      await ipc.trocarRascunho(workspaceId, novo.id);
      aoMudar();
      aoAvisar(`Você está no rascunho "${novo.nome}". A pasta de verdade não muda até publicar.`);
    } catch (e) {
      aoAvisar(mensagem(e));
    }
  }, [workspaceId, abertos.length, aoMudar, aoAvisar]);

  const trocar = useCallback(
    async (id: string | null) => {
      try {
        await ipc.trocarRascunho(workspaceId, id);
        aoMudar();
        // Dizer que os agentes foram reiniciados é obrigatório: eles PARAM, e
        // um agente que para sem explicação parece defeito.
        aoAvisar(
          id
            ? "Rascunho trocado. Os agentes reiniciam para trabalhar na cópia nova."
            : "De volta à pasta de verdade. Os agentes reiniciam.",
        );
      } catch (e) {
        aoAvisar(mensagem(e));
      }
    },
    [workspaceId, aoMudar, aoAvisar],
  );

  const preparar = useCallback(
    async (ensaio: Ensaio) => {
      try {
        setPublicando({ ensaio, previa: await ipc.preverPublicacao(ensaio.id) });
      } catch (e) {
        aoAvisar(mensagem(e));
      }
    },
    [aoAvisar],
  );

  const descartar = useCallback(
    async (ensaio: Ensaio) => {
      // Descartar apaga trabalho. Perguntar é o mínimo — e a pergunta diz o
      // que se perde, não "tem certeza?".
      if (!window.confirm(`Jogar fora "${ensaio.nome}"? O que foi feito nele some.`)) return;
      try {
        await ipc.descartarRascunho(ensaio.id);
        await recarregar();
        aoMudar();
      } catch (e) {
        aoAvisar(mensagem(e));
      }
    },
    [recarregar, aoMudar, aoAvisar],
  );

  if (!temHistorico) {
    return (
      <span className="rascunhos-sem" title="O Mutirão usa o Git para guardar rascunhos">
        sem rascunhos nesta máquina
      </span>
    );
  }

  return (
    <>
      <select
        className={`rascunho-atual${emFoco ? " ativo" : ""}`}
        value={ativo ?? ""}
        title={
          emFoco
            ? `Você está em "${emFoco.nome}". A pasta de verdade não muda até publicar.`
            : "Você está trabalhando na pasta de verdade."
        }
        onChange={(e) => void trocar(e.target.value || null)}
      >
        <option value="">pasta de verdade</option>
        {abertos.map((e) => (
          <option key={e.id} value={e.id}>
            {e.nome}
          </option>
        ))}
      </select>

      <button onClick={() => void criar()} title="Uma cópia isolada para experimentar">
        Novo
      </button>

      {emFoco && (
        <>
          <button className="publicar" onClick={() => void preparar(emFoco)}>
            Publicar…
          </button>
          <button onClick={() => void descartar(emFoco)} title="Joga fora o que foi feito aqui">
            Descartar
          </button>
        </>
      )}

      {publicando && (
        <TelaPublicar
          ensaio={publicando.ensaio}
          previa={publicando.previa}
          aoFechar={() => setPublicando(null)}
          aoPublicar={async (escolhas) => {
            try {
              const feito = await ipc.publicarRascunho(publicando.ensaio.id, escolhas);
              setPublicando(null);
              await recarregar();
              aoMudar();
              aoAvisar(
                `"${publicando.ensaio.nome}" publicado: ${feito.alteracoes.length} arquivo(s) na pasta de verdade.`,
              );
            } catch (e) {
              aoAvisar(mensagem(e));
            }
          }}
        />
      )}
    </>
  );
}

/**
 * O que muda, antes de mudar.
 *
 * O `ESPECIFICACAO.md §7` desenhou esta tela com "6 arquivos alterados, 1
 * conflito" e a escolha lado a lado para o que conflita. Binário não faz
 * merge — escolhe-se um lado —, e texto que os dois mexeram também precisa de
 * escolha aqui, porque marcador de conflito dentro do documento de alguém não
 * é resultado, é estrago.
 */
function TelaPublicar({
  ensaio,
  previa,
  aoFechar,
  aoPublicar,
}: {
  ensaio: Ensaio;
  previa: PreviaPublicacao;
  aoFechar: () => void;
  aoPublicar: (escolhas: Array<[string, LadoDoConflito]>) => void;
}) {
  const [escolhas, setEscolhas] = useState<Record<string, LadoDoConflito>>({});
  const faltam = previa.conflitos.filter((c) => !escolhas[c]);

  return (
    <div className="publicar-fundo" role="dialog" aria-label={`Publicar ${ensaio.nome}`}>
      <div className="publicar-caixa">
        <h2 className="publicar-titulo">Publicar “{ensaio.nome}”</h2>

        {previa.alteracoes.length === 0 ? (
          <p className="fraco">
            Nada mudou neste rascunho. Publicar não faria diferença nenhuma.
          </p>
        ) : (
          <>
            <p className="publicar-resumo">
              {previa.alteracoes.length} arquivo(s) vão para a pasta de verdade
              {previa.conflitos.length > 0 && `, ${previa.conflitos.length} com conflito`}.
            </p>
            <ul className="publicar-lista">
              {previa.alteracoes.map((m) => {
                const conflita = previa.conflitos.includes(m.caminho);
                return (
                  <li key={m.caminho} className={conflita ? "conflito" : ""}>
                    <span className="publicar-como">{conflita ? "⚠" : "✓"}</span>
                    <span className="publicar-caminho">{m.caminho}</span>
                    {conflita ? (
                      <span className="publicar-escolha">
                        {/* Sem opção pré-marcada: escolher por alguém é a
                            forma mais fácil de perder trabalho sem aviso. */}
                        <label>
                          <input
                            type="radio"
                            name={`c-${m.caminho}`}
                            checked={escolhas[m.caminho] === "rascunho"}
                            onChange={() =>
                              setEscolhas((v) => ({ ...v, [m.caminho]: "rascunho" }))
                            }
                          />
                          a do rascunho
                        </label>
                        <label>
                          <input
                            type="radio"
                            name={`c-${m.caminho}`}
                            checked={escolhas[m.caminho] === "original"}
                            onChange={() =>
                              setEscolhas((v) => ({ ...v, [m.caminho]: "original" }))
                            }
                          />
                          a que já estava
                        </label>
                      </span>
                    ) : (
                      <span className="publicar-como-texto">{m.como}</span>
                    )}
                  </li>
                );
              })}
            </ul>
          </>
        )}

        <div className="publicar-botoes">
          <button onClick={aoFechar}>Cancelar</button>
          <span className="espaco" />
          {faltam.length > 0 && (
            <span className="publicar-falta">
              escolha um lado em {faltam.length} arquivo(s)
            </span>
          )}
          <button
            className="conversa-botao aprovar"
            disabled={previa.alteracoes.length === 0 || faltam.length > 0}
            onClick={() =>
              aoPublicar(Object.entries(escolhas) as Array<[string, LadoDoConflito]>)
            }
          >
            Publicar
          </button>
        </div>
      </div>
    </div>
  );
}

function mensagem(e: unknown): string {
  if (ehErroIpc(e)) return e.mensagem;
  if (e instanceof Error) return e.message;
  return "Algo deu errado.";
}

export { ROTULO_ENSAIO };
