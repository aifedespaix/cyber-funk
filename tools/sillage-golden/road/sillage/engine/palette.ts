/** BOUCHON, pas une copie : `bridges.ts` n'importe que `prismaticCycle` et le type `ScenePalette`, jamais appelés ici. */
export type ScenePalette = Record<string, unknown>;
export function prismaticCycle(_palette: ScenePalette): unknown[] {
    return [];
}
