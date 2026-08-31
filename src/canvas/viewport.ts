import type { Viewport } from "../lib/tipos";

/** Ponto em pixels de tela (relativo ao elemento do canvas). */
export interface Tela {
  x: number;
  y: number;
}
/** Ponto em unidades de mundo (o que vai para o banco). */
export interface Mundo {
  x: number;
  y: number;
}

export const ZOOM_MIN = 0.15;
export const ZOOM_MAX = 4;

export function telaParaMundo(p: Tela, vp: Viewport): Mundo {
  return { x: (p.x - vp.x) / vp.zoom, y: (p.y - vp.y) / vp.zoom };
}

export function mundoParaTela(p: Mundo, vp: Viewport): Tela {
  return { x: p.x * vp.zoom + vp.x, y: p.y * vp.zoom + vp.y };
}

/**
 * Zoom ancorado no cursor: o ponto do mundo sob o ponteiro tem que continuar
 * exatamente sob o ponteiro depois do zoom. É o detalhe que separa um canvas
 * que parece bom de um que dá enjoo.
 */
export function zoomEm(vp: Viewport, cursor: Tela, fator: number): Viewport {
  const zoom = limitar(vp.zoom * fator, ZOOM_MIN, ZOOM_MAX);
  const k = zoom / vp.zoom;
  return {
    zoom,
    x: cursor.x - (cursor.x - vp.x) * k,
    y: cursor.y - (cursor.y - vp.y) * k,
  };
}

export function limitar(v: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, v));
}

/** Enquadra todos os nós na área visível, com folga. */
export function enquadrar(
  caixas: { x: number; y: number; w: number; h: number }[],
  larguraTela: number,
  alturaTela: number,
  folga = 80,
): Viewport {
  if (caixas.length === 0) return { x: 0, y: 0, zoom: 1 };
  let x1 = Infinity, y1 = Infinity, x2 = -Infinity, y2 = -Infinity;
  for (const c of caixas) {
    x1 = Math.min(x1, c.x);
    y1 = Math.min(y1, c.y);
    x2 = Math.max(x2, c.x + c.w);
    y2 = Math.max(y2, c.y + c.h);
  }
  const larg = Math.max(1, x2 - x1);
  const alt = Math.max(1, y2 - y1);
  const zoom = limitar(
    Math.min((larguraTela - folga * 2) / larg, (alturaTela - folga * 2) / alt),
    ZOOM_MIN,
    1.2,
  );
  return {
    zoom,
    x: (larguraTela - larg * zoom) / 2 - x1 * zoom,
    y: (alturaTela - alt * zoom) / 2 - y1 * zoom,
  };
}
