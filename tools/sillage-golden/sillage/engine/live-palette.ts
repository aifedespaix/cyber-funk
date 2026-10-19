/**
 * BOUCHON, pas une copie de Sillage : `sections.ts` importe `followed` pour ses trois couleurs d'ambiance.
 * Le vrai module lit les variables CSS du thème (DOM), absent ici, et la couleur n'entre pas dans les données de
 * référence. Seuls `beat.ts`, `sections.ts` et `show/build.ts` de ce dossier sont des copies conformes (vérifiées par
 * empreinte au lancement de `index.ts`).
 */
import { Color } from "three";
import type { SceneColorName, ScenePalette } from "./palette";

export function followed(_palette: ScenePalette, _name: SceneColorName): Color {
    return new Color();
}
