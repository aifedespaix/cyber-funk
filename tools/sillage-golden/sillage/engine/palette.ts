/**
 * BOUCHON, pas une copie de Sillage : seuls les types dont `sections.ts` et `live-palette.ts` (bouchon) ont besoin.
 * Les noms reprennent `engine/palette.ts` de Sillage.
 */
import type { Color } from "three";

export const SCENE_COLOR_NAMES = ["ink", "velvet", "mint", "azure", "violet", "rose", "amber", "ice", "alarm", "lime", "sun"] as const;

export type SceneColorName = (typeof SCENE_COLOR_NAMES)[number];
export type ScenePalette = Record<SceneColorName, Color>;
