// Les fenêtres à vie propre : quelques fenêtres, toujours les mêmes, ont leur manière à elles de vivre. Trois
// comportements fixes, tirés du hachage de la cellule et jamais de l'image : une télé bleutée qui papillote (lent et
// lisse), une fenêtre qui s'éteint puis se rallume tout doucement, un néon qui grésille. Une fenêtre qui grésille
// grésille donc toujours de la même façon quand on revient la voir.
//
// Ce texte est ajouté au bout de `facade.wesl` (voir `FACADE_SOURCE` dans `material.rs`) et réutilise `h21`.
//
// Sécurité photosensible (règle 7 de `CLAUDE.md`, WCAG 2.3.1) : seule une part `spark_rate` des cellules est concernée (0
// pour le tempérament 0 : le rendu d'avant, exactement). Le néon est le seul comportement rapide : il est multiplié par
// `allow` (`facade.motion.y`, la part rapide autorisée, déjà limitée en pente ; 0 sans flashs ou en respiration). Il ne
// fait qu'une extinction par seconde au plus (donc 1 flash/s, sous le plafond de 3), aux bords de 0,12 s (au moins
// `MIN_FADE`), et ne baisse jamais la lumière de plus de 60 %. Les autres sont lents (cycles de 5 s et plus, bords de
// 2,4 s). Rien au loin : l'appelant ne l'applique pas aux immeubles `far`.

// Périodes, en secondes : toutes divisent 2 400 s, la durée au bout de laquelle le temps du shader reboucle.
const TV_PERIOD_S: f32 = 8.0;
const TV_FINE_PERIOD_S: f32 = 5.0;
const REST_PERIOD_S: f32 = 120.0;
const BUZZ_PERIOD_S: f32 = 4.0;

// Parts des comportements parmi les cellules concernées (le reste est le néon).
const TV_SHARE: f32 = 0.4;
const REST_SHARE: f32 = 0.3;

// Extinction lente : éteinte de 40 s à 60 s dans le cycle de 120 s, bords de 2,4 s.
const REST_START_S: f32 = 40.0;
const REST_END_S: f32 = 60.0;
const REST_FADE_S: f32 = 2.4;

// Néon : trois creux par cycle de 4 s (un par seconde au plus), profondeur max, bord en s.
const BUZZ_DEPTH: f32 = 0.6;
const BUZZ_FADE_S: f32 = 0.12;

// Creux lisse entre `start` et `end` (s) dans le cycle, de bords `fade`.
fn life_dip(q: f32, start: f32, end: f32, fade: f32) -> f32 {
    return smoothstep(start, start + fade, q) * (1.0 - smoothstep(end, end + fade, q));
}

// Facteur (≤ 1) appliqué à la lumière d'une fenêtre. 1 pour les cellules sans comportement propre. `allow` : part de
// mouvement rapide autorisée (0 : le néon ne grésille pas).
fn window_life(cell: vec2<f32>, seed: f32, time: f32, spark_rate: f32, allow: f32) -> f32 {
    let pick = h21(cell + vec2<f32>(seed * 0.53 + 101.3, seed * 0.19));
    if pick >= spark_rate {
        return 1.0;
    }
    let kind = h21(cell + vec2<f32>(17.7, seed * 0.41 + 3.3));
    let phase = h21(cell + vec2<f32>(29.1 + seed * 0.07, 8.8));
    let tau = 6.2831853;
    if kind < TV_SHARE {
        // Télé : deux ondes lentes superposées, jamais de front.
        let slow = 0.5 + 0.5 * sin(tau * (time / TV_PERIOD_S + phase));
        let fine = 0.5 + 0.5 * sin(tau * (time / TV_FINE_PERIOD_S + phase * 3.0));
        return 1.0 - 0.3 * slow - 0.12 * fine;
    }
    if kind < TV_SHARE + REST_SHARE {
        let q = fract(time / REST_PERIOD_S + phase) * REST_PERIOD_S;
        return 1.0 - life_dip(q, REST_START_S, REST_END_S, REST_FADE_S);
    }
    let q = fract(time / BUZZ_PERIOD_S + phase) * BUZZ_PERIOD_S;
    let d = life_dip(q, 0.2, 0.5, BUZZ_FADE_S) + life_dip(q, 1.4, 1.7, BUZZ_FADE_S) + life_dip(q, 2.6, 2.9, BUZZ_FADE_S);
    return 1.0 - allow * BUZZ_DEPTH * min(d, 1.0);
}
