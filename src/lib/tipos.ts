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
