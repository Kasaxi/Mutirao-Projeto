// Espelho dos tipos do crate `nucleo`. Se algo aqui divergir do Rust, o
// teste `serializacao_dos_enums_bate_com_o_typescript` em nucleo/src/lib.rs
// quebra — é de propósito.

export type TipoNo = "agente" | "nota" | "arquivos" | "portal" | "forma";
export type TipoCabo = "fala_com" | "le_nota" | "escreve_nota";

export type EstadoSessao =
  | "ocioso"
  | "pensando"
  | "aguardando_aprovacao"
  | "aguardando_humano"
  | "aguardando_no"
  | "erro";

export interface Viewport {
  x: number;
  y: number;
  zoom: number;
}

export interface Workspace {
  id: string;
  nome: string;
  pasta: string;
  criado_em: number;
  ensaio_ativo: string | null;
  viewport: Viewport;
}

export interface No {
  id: string;
  workspace_id: string;
  ensaio_id: string | null;
  tipo: TipoNo;
  nome: string;
  x: number;
  y: number;
  w: number;
  h: number;
  z: number;
  config: Record<string, unknown>;
  criado_em: number;
  alterado_em: number;
}

export interface Cabo {
  id: string;
  workspace_id: string;
  de_node: string;
  para_node: string;
  tipo: TipoCabo;
  criado_em: number;
}

export interface EstadoCanvas {
  workspace: Workspace;
  nos: No[];
  cabos: Cabo[];
}

/** Formato único de erro vindo do Rust. Ver src-tauri/src/erro.rs */
export interface ErroIpc {
  codigo:
    | "banco"
    | "json"
    | "io"
    | "nao_encontrado"
    | "invalido"
    | "fora_do_escopo";
  mensagem: string;
}

export function ehErroIpc(e: unknown): e is ErroIpc {
  return typeof e === "object" && e !== null && "codigo" in e && "mensagem" in e;
}

/** Rótulos de interface por tipo de nó. */
export const ROTULO_NO: Record<TipoNo, string> = {
  agente: "Agente",
  nota: "Nota",
  arquivos: "Arquivos",
  portal: "Portal",
  forma: "Forma",
};

// =========================================================== M1: sessões ===

export type Adaptador = "claude" | "codex" | "pty" | "falso";
export type PapelMensagem = "usuario" | "agente" | "sistema" | "no";
export type Aprovacao = "automatica" | "pendente" | "aprovada" | "negada";

/**
 * Repare no que não está aqui: `token`. O segredo que o servidor MCP usa para
 * saber qual nó está chamando nunca atravessa a fronteira — ver
 * ESPECIFICACAO.md §4 e o teste `o_token_do_mcp_nunca_sai_no_json_da_sessao`.
 */
export interface Sessao {
  id: string;
  node_id: string;
  adaptador: Adaptador;
  sessao_externa_id: string | null;
  estado: EstadoSessao;
  custo_total: number;
  iniciada_em: number;
  ultimo_sinal_em: number;
}

export interface Mensagem {
  id: string;
  session_id: string;
  papel: PapelMensagem;
  origem_node: string | null;
  conteudo: string;
  tokens: number;
  custo: number;
  trace_id: string | null;
  criado_em: number;
}

export interface ChamadaFerramenta {
  id: string;
  session_id: string;
  ferramenta: string;
  argumentos: Record<string, unknown>;
  resultado: unknown | null;
  erro: string | null;
  aprovacao: Aprovacao;
  decidido_por: string | null;
  criado_em: number;
}

/** Entrada e saída separadas: elas custam preços diferentes. */
export interface Uso {
  tokens_entrada: number;
  tokens_saida: number;
  /** Em dólar — é a moeda em que a API cobra. `NaN` quando o modelo é desconhecido. */
  custo_usd: number;
}

/** União discriminada por `tipo`, espelhando `#[serde(tag = "tipo")]` no Rust. */
export type EventoAgente =
  | { tipo: "sessao_iniciada"; id_externo: string; modelo: string; ferramentas: string[] }
  | { tipo: "texto_parcial"; delta: string }
  | { tipo: "raciocinando"; resumo: string }
  | { tipo: "ferramenta_pedida"; id: string; nome: string; argumentos: Record<string, unknown> }
  | { tipo: "ferramenta_concluida"; id: string; resultado: unknown | null; erro: string | null }
  | { tipo: "turno_concluido"; texto_final: string; uso: Uso }
  | { tipo: "precisa_humano"; pergunta: string }
  | { tipo: "erro"; mensagem: string; recuperavel: boolean };

export interface CustoDoNo {
  node_id: string;
  custo: number;
}

/** Payloads dos eventos do núcleo. Nomes em ESPECIFICACAO.md §3. */
export interface EventoSessao {
  tipo: "sessao_evento";
  session_id: string;
  evento: EventoAgente;
}

export interface EventoEstado {
  tipo: "sessao_estado";
  session_id: string;
  node_id: string;
  estado: EstadoSessao;
  pede_atencao: boolean;
}

export interface EventoCusto {
  tipo: "custo_atualizado";
  workspace_id: string;
  total: number;
  por_no: CustoDoNo[];
}

export const ROTULO_ESTADO: Record<EstadoSessao, string> = {
  ocioso: "pronto",
  pensando: "pensando",
  aguardando_aprovacao: "esperando você aprovar",
  aguardando_humano: "esperando sua resposta",
  aguardando_no: "esperando outro nó",
  erro: "deu problema",
};

/** Só estes acendem o ponto no cabeçalho. Pensando não é pedido de socorro. */
export function pedeAtencao(e: EstadoSessao): boolean {
  return e === "aguardando_aprovacao" || e === "aguardando_humano" || e === "erro";
}

const MOEDA = new Intl.NumberFormat("pt-BR", {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 2,
  // Um turno barato custa frações de centavo. Arredondar para dois zeraria
  // tudo e o painel de custo viraria enfeite.
  maximumFractionDigits: 4,
});

/**
 * Custo em dólar, que é como a API cobra. Converter para real exige uma
 * cotação, e cotação chumbada envelhece mal — fica para quando houver de onde
 * buscá-la. Modelo sem preço na tabela vira "—", nunca zero: zero mentiria.
 */
export function formatarCusto(usd: number): string {
  if (!Number.isFinite(usd)) return "—";
  return MOEDA.format(usd);
}
