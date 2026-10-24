// BOUCHON (pas une copie de Sillage) : `sky.ts` s'abonne aux fondus de palette pour refaire les couleurs cuites des
// étoiles. Le script ne fait jamais de fondu : l'abonnement ne fait rien.
import type { ScenePalette } from "./palette";

export function watchPalette(_palette: ScenePalette, _listener: (palette: ScenePalette, settled: boolean) => void): () => void {
    return () => {};
}
