import { Color, SRGBColorSpace } from "three";

export const SCENE_COLOR_NAMES = ["ink", "velvet", "mint", "azure", "violet", "rose", "amber", "ice", "alarm", "lime", "sun"] as const;

export type SceneColorName = (typeof SCENE_COLOR_NAMES)[number];
export type ScenePalette = Record<SceneColorName, Color>;

/**
 * Lit les tokens `--scene-*` (packages/tokens/src/scene.css), tels que les voit `root` (le thème choisi s'y applique), pour que la scène
 * ne contienne aucune couleur en dur. Les valeurs sont converties en espace
 * de travail linéaire par three.js.
 */
export function readScenePalette(root: Element = document.documentElement): ScenePalette {
  const style = getComputedStyle(root);
  const entries = SCENE_COLOR_NAMES.map((name) => {
    const raw = style.getPropertyValue(`--scene-${name}`).trim();
    if (!raw) throw new Error(`Token --scene-${name} introuvable`);
    return [name, new Color().setStyle(raw, SRGBColorSpace)] as const;
  });
  return Object.fromEntries(entries) as ScenePalette;
}

/** Cycle lumineux utilisé pour répartir les teintes : cyan dominant, un peu de violet, de corail et de glace. */
export function prismaticCycle(palette: ScenePalette): Color[] {
  return [palette.mint, palette.azure, palette.violet, palette.mint, palette.rose, palette.ice];
}
