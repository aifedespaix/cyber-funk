/** Hash déterministe dans [0, 1) : même graine, même résultat, d'une image à l'autre. */
export function hash(seed: number, salt: number): number {
  const x = Math.sin(seed * 127.1 + salt * 311.7) * 43758.5453;
  return x - Math.floor(x);
}
