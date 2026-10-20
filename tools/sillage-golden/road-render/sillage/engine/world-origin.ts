/**
 * Précision en longue session : la voiture file vers -z sans jamais revenir, et `carZ` atteint vite -100 000 puis
 * -400 000 en fin de set. Three.js compose `modelViewMatrix` en double, mais les shaders qui reçoivent des positions
 * monde en float32 y perdent le centimètre (≈ 3 cm à 300 km) et leurs `fract` / `mod` / bruits se mettent à trembler.
 *
 * Remède local, shader par shader : l'« origine des motifs » est la position de la voiture arrondie à un multiple de
 * `ORIGIN_STEP`. Le CPU (en double) fournit `maillage - origine`, et le shader calcule ses motifs sur cette coordonnée
 * relative, toujours inférieure à quelques kilomètres. Rien ne change près de z = 0 (l'origine y vaut 0).
 *
 * `ORIGIN_STEP` est un multiple de 9, 24, 100, 8 et 5 : les motifs réguliers de la route (tirets, joints, bandes, gouttes)
 * se prolongent sans à-coup au changement d'origine. Seuls les bruits aléatoires (grain, flaques, brume) se redistribuent
 * d'un coup, environ toutes les quatorze minutes à pleine allure.
 */
export const ORIGIN_STEP = 28800;

/** Origine des motifs (z) la plus proche de `z`. */
export function originFor(z: number): number {
  return Math.round(z / ORIGIN_STEP) * ORIGIN_STEP + 0;
}
