/**
 * Agrupa chamadas em rajada e só executa a última, depois de `ms` parados.
 *
 * Usado para gravação: arrastar um nó dispara ~60 eventos por segundo, e
 * gravar todos derrubaria o SQLite sem necessidade. O banco recebe a posição
 * final, não o caminho percorrido.
 */
export function adiar<A extends unknown[]>(
  fn: (...args: A) => void,
  ms: number,
): ((...args: A) => void) & { agora: () => void; cancelar: () => void } {
  let t: ReturnType<typeof setTimeout> | null = null;
  let ultimos: A | null = null;

  const disparar = (...args: A) => {
    ultimos = args;
    if (t) clearTimeout(t);
    t = setTimeout(() => {
      t = null;
      if (ultimos) fn(...ultimos);
      ultimos = null;
    }, ms);
  };

  disparar.agora = () => {
    if (t) clearTimeout(t);
    t = null;
    if (ultimos) fn(...ultimos);
    ultimos = null;
  };

  disparar.cancelar = () => {
    if (t) clearTimeout(t);
    t = null;
    ultimos = null;
  };

  return disparar;
}
