// BOUCHON de `building-material.ts` (pas une copie) : `scenery.ts` et `bridges.ts` n'en prennent que ces constantes et un
// matériau dont ils écrivent les uniformes. Valeurs reprises de l'original (`FLOOR_HEIGHT = 2.4`, `BASE_DEPTH = FLOOR_HEIGHT * 160`).
export const FLOOR_HEIGHT = 2.4;
export const MIST_Y = -50;
export const BASE_DEPTH = FLOOR_HEIGHT * 160;

export function createBuildingMaterial(): { uniforms: Record<string, { value: number }>; dispose(): void } {
    const uniforms: Record<string, { value: number }> = {};
    for (const name of ["uTime", "uPulse", "uBass", "uMid", "uHigh", "uBeat"]) uniforms[name] = { value: 0 };
    return { uniforms, dispose() {} };
}
