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
  /** O rascunho em foco. `null` = trabalhando na pasta de verdade. */
  ensaio_ativo: string | null;
  /** Onde mora o histórico. `null` = sem rascunhos nesta máquina. */
  repo: string | null;
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
  /** Papel deste agente. `null` = agente sem papel, como todo nó até o M4. */
  role_id: string | null;
  /** Quem recrutou. `null` = foi uma pessoa que criou este nó. */
  recrutado_por: string | null;
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

// -------------------------------------------------------------- arquivos

export interface ItemArquivo {
  /** Relativo à pasta do workspace, sempre com `/`, mesmo no Windows. */
  caminho: string;
  nome: string;
  pasta: boolean;
  tamanho: number;
}

export interface Nota {
  arquivo: string;
  conteudo: string;
}

// ------------------------------------------------------------- aprovação

export type Decisao = "aprovada" | "negada";

export interface RegraAprovacao {
  id: string;
  workspace_id: string;
  ferramenta: string;
  criado_em: number;
}

/** O que o card de aprovação mostra. Vem mastigado do núcleo. */
export interface PedidoAprovacao {
  tool_call_id: string;
  session_id: string;
  node_id: string;
  ferramenta: string;
  resumo: string;
  detalhe: string;
  previa: string | null;
  criado_em: number;
}

export interface EventoAprovacaoPedida {
  tipo: "aprovacao_pedida";
  pedido: PedidoAprovacao;
}

export interface EventoAprovacaoDecidida {
  tipo: "aprovacao_decidida";
  tool_call_id: string;
  node_id: string;
  decisao: Decisao;
  decidido_por: string;
}

// ============================================================ M3: a ponte ===

export type TipoMensagem = "pedido" | "aviso";

/**
 * Um nó falou com outro. É o que faz a ponte ser visível em vez de mágica: o
 * cabo acende no sentido do recado, e some sozinho.
 *
 * O campo é `tipo_mensagem`, e não `tipo`, porque `tipo` é o discriminante do
 * próprio envelope — ver o comentário em `modelo.rs`.
 */
export interface EventoNoMensagem {
  tipo: "no_mensagem";
  de_node: string;
  para_node: string;
  trace_id: string;
  tipo_mensagem: TipoMensagem;
}

/**
 * Uma cadeia acabou por limite, não por conclusão. Sempre chega ao usuário: o
 * `ARQUITETURA.md §6` é explícito em que estourar um limite avisa em vez de
 * queimar crédito em silêncio.
 */
export interface EventoCadeiaEncerrada {
  tipo: "cadeia_encerrada";
  trace_id: string;
  node_id: string;
  motivo: string;
}

/** A cadeia parou numa pergunta, e quem destrava é a pessoa. Não é erro. */
export interface EventoCadeiaEsperaPessoa {
  tipo: "cadeia_espera_pessoa";
  trace_id: string;
  /** Quem está parado esperando. */
  node_id: string;
  /** Quem levantou a mão — é neste nó que a resposta tem de ser dada. */
  perguntou_node: string;
  perguntou_nome: string;
}

// ============================================================ M4: papéis ===

/**
 * Quanto o papel pode fazer sozinho.
 *
 * A autonomia escolhe o **conjunto de ferramentas**, nunca se o card aparece.
 * Um papel `solto` grava com card igual a um `padrao`; ele só alcança mais
 * coisa. Ver `papeis.rs` e o `ARQUITETURA.md §8`.
 */
export type Autonomia = "cauteloso" | "padrao" | "solto";

export interface Papel {
  id: string;
  nome: string;
  prompt: string;
  ferramentas: string[];
  autonomia: Autonomia;
  modelo: string | null;
  /** Veio com o app. Dá para editar, não dá para apagar. */
  embutido: boolean;
  criado_em: number;
  /** Servidores MCP externos, sem os segredos deles. */
  mcp?: ServidorMcp[];
}

export const ROTULO_AUTONOMIA: Record<Autonomia, string> = {
  cauteloso: "só lê e conversa",
  padrao: "grava, com aprovação",
  solto: "grava e roda comando, com aprovação",
};

/**
 * Um nó dentro de uma partitura. Espelha `modelo::NoSalvo`.
 *
 * Sem id de propósito: o id pertence ao canvas onde o nó vive, e reabrir cria
 * nós novos. O papel vai pelo **nome**, para a partitura abrir noutra máquina
 * onde o mesmo papel tem outro id.
 */
export interface NoSalvo {
  tipo: TipoNo;
  nome: string;
  x: number;
  y: number;
  w: number;
  h: number;
  config: Record<string, unknown>;
  papel: string | null;
}

/** Um time salvo. Guarda quem trabalha e como está ligado, não a conversa. */
export interface Partitura {
  id: string;
  workspace_id: string;
  nome: string;
  snapshot: {
    nos: NoSalvo[];
    /** Pelos índices em `nos`, porque os ids não sobrevivem à travessia. */
    cabos: Array<{ de: number; para: number; tipo: TipoCabo }>;
  };
  criado_em: number;
}

/**
 * O canvas mudou por fora da interface — hoje só quando um agente recruta
 * outro. O front relê o workspace ao ver isto.
 */
// =========================================================== M5: rascunhos ===

/**
 * Um rascunho: uma cópia isolada da pasta em que o time trabalha sem mexer no
 * que está valendo.
 *
 * `branch` e `caminho_worktree` chegam do núcleo mas **não aparecem na tela**.
 * O usuário vê "Rascunho 2" e "Publicar"; a `Decisão 3` do `ARQUITETURA.md`
 * vale até nas mensagens de erro.
 */
export interface Ensaio {
  id: string;
  workspace_id: string;
  nome: string;
  branch: string;
  caminho_worktree: string;
  base_commit: string | null;
  estado: EstadoEnsaio;
  criado_em: number;
  alterado_em: number;
}

export type EstadoEnsaio = "aberto" | "publicado" | "descartado";

export const ROTULO_ENSAIO: Record<EstadoEnsaio, string> = {
  aberto: "em uso",
  publicado: "publicado",
  descartado: "descartado",
};

export type TipoMudanca = "criado" | "alterado" | "apagado" | "renomeado";

export interface MudancaArquivo {
  caminho: string;
  como: TipoMudanca;
}

/** O que a tela de publicar mostra ANTES do clique. */
export interface PreviaPublicacao {
  ensaio_id: string;
  alteracoes: MudancaArquivo[];
  /** Cada um precisa de uma escolha: publicar pela metade é pior que não publicar. */
  conflitos: string[];
}

export type LadoDoConflito = "original" | "rascunho";

/** Um servidor MCP de fora, ligado a um papel. Ver `ARQUITETURA.md §7`. */
export interface ServidorMcp {
  nome: string;
  url: string;
  /**
   * Os cabeçalhos **não voltam** do backend: a chave do CRM de alguém não
   * atravessa a fronteira. Vazio ao ler quer dizer "há segredo guardado lá",
   * não "não há segredo".
   */
  cabecalhos?: Array<[string, string]>;
}

export interface EventoCanvasMudou {
  tipo: "canvas_mudou";
  workspace_id: string;
  motivo: string;
}

/**
 * Destas o usuário pode dizer "não perguntar de novo nesta pasta". Espelha
 * `barramento::FERRAMENTAS_QUE_ACEITAM_REGRA` — liberar `Bash` de uma vez
 * seria entregar a máquina num clique que ninguém lembra depois.
 */
export const FERRAMENTAS_QUE_ACEITAM_REGRA = ["Write", "Edit", "NotebookEdit"];

/** Espelho de `ferramentas::SERVIDOR`. Prefixo do nome que o modelo vê. */
export const SERVIDOR_MCP = "mutirao";

/** Espelho de `ferramentas::FERRAMENTAS_QUE_GRAVAM`, sem o prefixo. */
export const FERRAMENTAS_MCP_QUE_GRAVAM = ["escrever_nota", "escrever_arquivo"];

export function nomeCompletoMcp(ferramenta: string): string {
  return `mcp__${SERVIDOR_MCP}__${ferramenta}`;
}

/** Espelho de `barramento::aceita_regra`: as nativas mais as do §6 que gravam. */
export function aceitaRegra(ferramenta: string): boolean {
  return (
    FERRAMENTAS_QUE_ACEITAM_REGRA.includes(ferramenta) ||
    FERRAMENTAS_MCP_QUE_GRAVAM.some((f) => nomeCompletoMcp(f) === ferramenta)
  );
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
