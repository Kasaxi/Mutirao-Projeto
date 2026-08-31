import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { escutar, ipc } from "../lib/ipc";
import {
  ehErroIpc,
  formatarCusto,
  ROTULO_ESTADO,
  type ChamadaFerramenta,
  type EstadoSessao,
  type EventoAgente,
  type EventoEstado,
  type EventoSessao,
  type Mensagem,
  type No,
  type Sessao,
} from "../lib/tipos";

/**
 * A face conversa: a mesma sessão que o terminal mostraria crua, mas em
 * bolhas, com as ações do agente viradas cards em vez de linha de log.
 *
 * O componente é dono do histórico do próprio nó. Ele não recebe mensagem por
 * prop: assina os eventos do núcleo filtrando pelo id da sessão. Assim dois
 * agentes conversando ao mesmo tempo não repintam um ao outro.
 */

interface Props {
  no: No;
  /** Sobe o estado para o cabeçalho do nó desenhar o ponto de atenção. */
  aoMudarEstado?: (estado: EstadoSessao) => void;
}

type Item =
  | { chave: string; em: number; tipo: "mensagem"; mensagem: Mensagem }
  | { chave: string; em: number; tipo: "acao"; acao: ChamadaFerramenta };

export function Conversa({ no, aoMudarEstado }: Props) {
  const [sessao, setSessao] = useState<Sessao | null>(null);
  const [estado, setEstado] = useState<EstadoSessao>("ocioso");
  const [mensagens, setMensagens] = useState<Mensagem[]>([]);
  const [acoes, setAcoes] = useState<ChamadaFerramenta[]>([]);
  const [parcial, setParcial] = useState("");
  const [pensamento, setPensamento] = useState("");
  const [custo, setCusto] = useState(0);
  const [rascunho, setRascunho] = useState("");
  const [erro, setErro] = useState<string | null>(null);
  const [cru, setCru] = useState(false);
  const [eventosCrus, setEventosCrus] = useState<string[]>([]);

  const fim = useRef<HTMLDivElement>(null);
  // Espelho do texto que vai chegando. O fim do turno precisa lê-lo na hora,
  // e o estado do React só estaria atualizado no render seguinte.
  const parcialRef = useRef("");

  const limparParcial = useCallback(() => {
    parcialRef.current = "";
    setParcial("");
    setPensamento("");
  }, []);

  // ---------------------------------------------------------------- carga

  useEffect(() => {
    let vivo = true;
    (async () => {
      try {
        const s = await ipc.abrirSessao(no.id);
        if (!vivo) return;
        setSessao(s);
        setEstado(s.estado);
        setCusto(s.custo_total);
        const [h, a] = await Promise.all([
          ipc.historico(s.id),
          ipc.acoesDaSessao(s.id),
        ]);
        if (!vivo) return;
        setMensagens(h);
        setAcoes(a);
      } catch (e) {
        if (vivo) setErro(mensagemDeErro(e));
      }
    })();
    return () => {
      vivo = false;
    };
  }, [no.id]);

  // O callback vem do App como closure nova a cada render. Guardá-lo num ref
  // é o que impede o efeito abaixo de rodar em todo render — e, com ele, o
  // vaivém de estado que faria o nó repintar sem parar.
  const aoMudarRef = useRef(aoMudarEstado);
  useEffect(() => {
    aoMudarRef.current = aoMudarEstado;
  });
  useEffect(() => {
    aoMudarRef.current?.(estado);
  }, [estado]);

  // -------------------------------------------------------------- eventos

  const recarregar = useCallback(async (sessionId: string) => {
    try {
      const [h, a] = await Promise.all([
        ipc.historico(sessionId),
        ipc.acoesDaSessao(sessionId),
      ]);
      setMensagens(h);
      setAcoes(a);
    } catch {
      /* o histórico volta na próxima abertura do nó; não vale um alarme */
    }
  }, []);

  useEffect(() => {
    if (!sessao) return;
    const idSessao = sessao.id;

    // `escutar` é assíncrono. Se o nó sair de cena antes de a assinatura
    // ficar pronta, cancelar na hora — senão sobra um ouvinte vivo por
    // remontagem, e o mesmo delta de texto entra duas vezes na bolha.
    let vivo = true;
    const paradas: Array<() => void> = [];
    const registrar = (parar: () => void) => {
      if (vivo) paradas.push(parar);
      else parar();
    };

    escutar<EventoSessao>("sessao:evento", (p) => {
      if (p.session_id !== idSessao) return;
      setEventosCrus((v) => [...v.slice(-199), JSON.stringify(p.evento)]);
      aplicar(p.evento);
    }).then(registrar);

    escutar<EventoEstado>("sessao:estado", (p) => {
      if (p.session_id !== idSessao) return;
      setEstado(p.estado);
      if (p.estado !== "pensando") setPensamento("");
    }).then(registrar);

    /** Mensagem montada a partir do evento, sem passar pelo banco. */
    function acrescentar(
      papel: Mensagem["papel"],
      conteudo: string,
      extra?: { tokens: number; custo: number },
    ) {
      if (!conteudo.trim()) return;
      setMensagens((v) => [
        ...v,
        {
          id: `evento_${Date.now()}_${v.length}`,
          session_id: idSessao,
          papel,
          origem_node: null,
          conteudo,
          tokens: extra?.tokens ?? 0,
          custo: extra?.custo ?? 0,
          trace_id: null,
          criado_em: Date.now(),
        },
      ]);
    }

    function aplicar(evento: EventoAgente) {
      switch (evento.tipo) {
        case "raciocinando":
          setPensamento(evento.resumo);
          break;
        case "texto_parcial":
          // O ref acompanha o estado para o fim do turno poder ler o texto
          // acumulado sem depender de um render ter acontecido.
          parcialRef.current += evento.delta;
          setParcial(parcialRef.current);
          break;
        case "ferramenta_pedida":
          setAcoes((v) => [
            ...v,
            {
              id: `${idSessao}:${evento.id}`,
              session_id: idSessao,
              ferramenta: evento.nome,
              argumentos: evento.argumentos,
              resultado: null,
              erro: null,
              aprovacao: "automatica",
              decidido_por: null,
              criado_em: Date.now(),
            },
          ]);
          break;
        case "ferramenta_concluida":
          setAcoes((v) =>
            v.map((a) =>
              a.id === `${idSessao}:${evento.id}`
                ? { ...a, resultado: evento.resultado, erro: evento.erro }
                : a,
            ),
          );
          break;
        // Os três casos abaixo montam a mensagem a partir do próprio evento em
        // vez de reler o histórico. Reler parece mais simples e é uma corrida:
        // o evento sai antes de o núcleo terminar de gravar, e a releitura
        // chega a tempo de não achar nada. O que o evento carrega já basta —
        // a releitura da montagem reconcilia depois.
        case "turno_concluido": {
          const custoTurno = Number.isFinite(evento.uso.custo_usd) ? evento.uso.custo_usd : 0;
          acrescentar("agente", evento.texto_final.trim() || parcialRef.current, {
            tokens: evento.uso.tokens_entrada + evento.uso.tokens_saida,
            custo: custoTurno,
          });
          limparParcial();
          setCusto((c) => c + custoTurno);
          break;
        }
        case "erro":
          limparParcial();
          acrescentar("sistema", evento.mensagem);
          break;
        case "precisa_humano":
          limparParcial();
          acrescentar("sistema", evento.pergunta);
          break;
        case "sessao_iniciada":
          break;
      }
    }

    return () => {
      vivo = false;
      for (const parar of paradas) parar();
    };
  }, [sessao, limparParcial]);

  // --------------------------------------------------------------- rolagem

  const itens = useMemo<Item[]>(() => {
    const lista: Item[] = [
      ...mensagens.map((m) => ({
        chave: m.id,
        em: m.criado_em,
        tipo: "mensagem" as const,
        mensagem: m,
      })),
      ...acoes.map((a) => ({
        chave: a.id,
        em: a.criado_em,
        tipo: "acao" as const,
        acao: a,
      })),
    ];
    return lista.sort((x, y) => x.em - y.em);
  }, [mensagens, acoes]);

  useEffect(() => {
    fim.current?.scrollIntoView({ block: "end" });
  }, [itens.length, parcial, pensamento]);

  // ----------------------------------------------------------------- ações

  const enviar = useCallback(async () => {
    const texto = rascunho.trim();
    if (!sessao || !texto) return;
    setRascunho("");
    setErro(null);
    // Otimista: a bolha do usuário aparece no ato. O núcleo grava a mesma
    // mensagem e o recarregar do fim do turno reconcilia.
    setMensagens((v) => [
      ...v,
      {
        id: `local_${Date.now()}`,
        session_id: sessao.id,
        papel: "usuario",
        origem_node: null,
        conteudo: texto,
        tokens: 0,
        custo: 0,
        trace_id: null,
        criado_em: Date.now(),
      },
    ]);
    try {
      await ipc.enviarMensagem(sessao.id, texto);
    } catch (e) {
      setErro(mensagemDeErro(e));
      void recarregar(sessao.id);
    }
  }, [rascunho, sessao, recarregar]);

  const parar = useCallback(async () => {
    if (!sessao) return;
    try {
      await ipc.cancelarTurno(sessao.id);
      setParcial("");
      void recarregar(sessao.id);
    } catch (e) {
      setErro(mensagemDeErro(e));
    }
  }, [sessao, recarregar]);

  // --------------------------------------------------------------- desenho

  const trabalhando = estado === "pensando";
  const podeEnviar = estado === "ocioso" || estado === "erro";

  if (erro && !sessao) {
    return <div className="conversa-erro">{erro}</div>;
  }

  return (
    <div className="conversa-viva">
      <div className="conversa-abas">
        <button
          className={!cru ? "aba ativa" : "aba"}
          onClick={() => setCru(false)}
          type="button"
        >
          conversa
        </button>
        <button className={cru ? "aba ativa" : "aba"} onClick={() => setCru(true)} type="button">
          terminal
        </button>
        <span className="conversa-estado" title={`Estado: ${estado}`}>
          {ROTULO_ESTADO[estado]}
        </span>
      </div>

      {cru ? (
        <div className="conversa-cru" onWheel={rolarDentro}>
          {eventosCrus.length === 0 ? (
            <p className="fraco">
              A mesma sessão, sem enfeite. Os eventos aparecem aqui a partir do próximo turno —
              o fluxo cru não é gravado, só o que ele produz.
            </p>
          ) : (
            eventosCrus.map((linha, i) => (
              <div className="cru-linha" key={i}>
                {linha}
              </div>
            ))
          )}
          <div ref={fim} />
        </div>
      ) : (
        <div className="conversa-lista" onWheel={rolarDentro}>
          {itens.length === 0 && !parcial && (
            <p className="fraco">Pronto para começar. Escreva aí embaixo.</p>
          )}

          {itens.map((item) =>
            item.tipo === "mensagem" ? (
              <Bolha key={item.chave} mensagem={item.mensagem} />
            ) : (
              <CardAcao key={item.chave} acao={item.acao} />
            ),
          )}

          {pensamento && <div className="pensamento">{pensamento}</div>}
          {parcial && <div className="bolha agente escrevendo">{parcial}</div>}
          <div ref={fim} />
        </div>
      )}

      <div className="conversa-rodape">
        <textarea
          className="conversa-campo"
          rows={1}
          placeholder={podeEnviar ? "escreva aqui…" : "aguarde o turno terminar…"}
          value={rascunho}
          disabled={!podeEnviar}
          onChange={(e) => setRascunho(e.target.value)}
          onPointerDown={(e) => e.stopPropagation()}
          onKeyDown={(e) => {
            // Enter manda, Shift+Enter quebra linha. É o que todo mundo espera
            // de um campo de conversa.
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void enviar();
            }
            e.stopPropagation();
          }}
        />
        <span className="conversa-custo" title="Custo acumulado desta sessão, em dólar">
          {formatarCusto(custo)}
        </span>
        {trabalhando ? (
          <button className="conversa-botao parar" onClick={parar} type="button">
            parar
          </button>
        ) : (
          <button
            className="conversa-botao"
            onClick={() => void enviar()}
            disabled={!podeEnviar || !rascunho.trim()}
            type="button"
          >
            enviar
          </button>
        )}
      </div>

      {erro && (
        <div className="conversa-erro" role="alert" onClick={() => setErro(null)}>
          {erro}
        </div>
      )}
    </div>
  );
}

/**
 * Rolar dentro da conversa só rouba o gesto do canvas quando há mesmo o que
 * rolar naquela direção. A primeira versão engolia todo `wheel`, e o efeito
 * era desagradável: passar o cursor por cima de um agente vazio travava o
 * canvas sem explicação. No fim de curso o gesto volta a ser do canvas, que é
 * como o resto das ferramentas de quadro se comporta.
 *
 * Ctrl/⌘ + rolar nunca é capturado: o zoom continua sendo do canvas em
 * qualquer lugar da tela.
 */
function rolarDentro(e: React.WheelEvent<HTMLDivElement>) {
  if (e.ctrlKey || e.metaKey) return;
  const el = e.currentTarget;
  const sobra = el.scrollHeight - el.clientHeight - el.scrollTop;
  const podeDescer = e.deltaY > 0 && sobra > 1;
  const podeSubir = e.deltaY < 0 && el.scrollTop > 0;
  if (podeDescer || podeSubir) e.stopPropagation();
}

function Bolha({ mensagem }: { mensagem: Mensagem }) {
  if (mensagem.papel === "sistema") {
    return <div className="aviso-sistema">{mensagem.conteudo}</div>;
  }
  const meu = mensagem.papel === "usuario";
  return (
    <div className={`bolha ${meu ? "usuario" : "agente"}`}>
      {mensagem.conteudo}
      {!meu && mensagem.tokens > 0 && (
        <span className="bolha-custo" title={`${mensagem.tokens} tokens`}>
          {formatarCusto(mensagem.custo)}
        </span>
      )}
    </div>
  );
}

/**
 * Ação do agente como card, não como texto de log. A diferença importa: log é
 * para quem depura, card é para quem precisa saber o que mexeram no arquivo.
 */
function CardAcao({ acao }: { acao: ChamadaFerramenta }) {
  const emAndamento = acao.resultado === null && acao.erro === null;
  return (
    <div className={`card-acao${acao.erro ? " falhou" : ""}`}>
      <span className="acao-verbo">{verbo(acao.ferramenta)}</span>
      <span className="acao-alvo">{alvo(acao.argumentos)}</span>
      <span className="acao-estado">
        {acao.erro ? "falhou" : emAndamento ? "…" : "ok"}
      </span>
    </div>
  );
}

const VERBOS: Record<string, string> = {
  ler_arquivo: "leu",
  escrever_arquivo: "gravou",
  listar_arquivos: "listou",
  ler_nota: "leu a nota",
  escrever_nota: "escreveu na nota",
  enviar_para: "perguntou a",
  avisar: "avisou",
};

function verbo(ferramenta: string): string {
  return VERBOS[ferramenta] ?? ferramenta;
}

/** O argumento que o usuário reconhece: o caminho, o nome, o alvo. */
function alvo(argumentos: Record<string, unknown>): string {
  for (const chave of ["caminho", "nota", "no", "arquivo", "nome"]) {
    const v = argumentos[chave];
    if (typeof v === "string" && v) return v;
  }
  const primeiro = Object.values(argumentos).find((v) => typeof v === "string");
  return typeof primeiro === "string" ? primeiro : "";
}

function mensagemDeErro(e: unknown): string {
  if (ehErroIpc(e)) return e.mensagem;
  if (e instanceof Error) return e.message;
  return "Algo deu errado.";
}
