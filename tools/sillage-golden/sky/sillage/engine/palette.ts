// BOUCHON (pas une copie de Sillage) : `sky.ts` et `planets.ts` n'ont besoin que du type de la palette et de
// l'objet `Color`. Chaque rôle de couleur est un `Color` numéroté dans l'ordre de `SCENE_COLOR_NAMES`.
import { Color } from "three";

export const SCENE_COLOR_NAMES = ["ink", "velvet", "mint", "azure", "violet", "rose", "amber", "ice", "alarm", "lime", "sun"] as const;

export type SceneColorName = (typeof SCENE_COLOR_NAMES)[number];
export type ScenePalette = Record<SceneColorName, Color>;

export function numberedPalette(): ScenePalette {
    return Object.fromEntries(SCENE_COLOR_NAMES.map((name, i) => [name, new Color(i)])) as ScenePalette;
}
