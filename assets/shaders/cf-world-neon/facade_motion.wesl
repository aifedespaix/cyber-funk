// Les mouvements de lumière d'une façade : le facteur multiplicatif de la lumière d'un immeuble selon son `MotionKind`
// (`temperament.rs`).
//
// Ce texte est ajouté au bout de `facade.wesl` (voir `FACADE_SOURCE` dans `material.rs`) : il lit le bloc `facade` de ce
// fichier. Indices de mouvement (ceux de `MotionKind`) :
//   0 immobile, 1 vague montante, 2 vague descendante, 3 dérive lente, 4 balayage de projecteur, 5 pulsation.
//
// Sécurité photosensible (règle 7 de `CLAUDE.md`, WCAG 2.3.1) : `facade.motion.x` est la voie « rapide » déjà passée par
// le limiteur (3 départs par seconde au plus, fondus d'au moins `MIN_FADE`), `facade.motion.y` la part de mouvement rapide
// autorisée (0 sans spectacle, flashs coupés ou respiration). Ce qui reste quand elle vaut 0 est lent (cycles de 4 s et
// plus) et lisse, sans front : il ne compte pas pour un flash. Aucun mouvement ne dépasse +50 % de la lumière.

// Périodes, en secondes : toutes divisent 2 400 s, la durée au bout de laquelle le temps du shader reboucle, donc aucun
// mouvement ne saute au rebouclage.
const DRIFT_PERIOD_S: f32 = 24.0;
const SWEEP_PERIOD_S: f32 = 12.0;
const BREATH_PERIOD_S: f32 = 4.0;

// Bande de lumière sans front, de demi-largeur `width` autour d'une distance `d` nulle.
fn motion_band(d: f32, width: f32) -> f32 {
    let x = d / width;
    return exp(-x * x);
}

// Facteur de lumière d'un immeuble. `u` : abscisse le long de la façade (m), `h` : altitude (m), `beat` : temps musical
// continu, `sweep` : voie rapide limitée (0..1). Immobile : exactement 1.
fn motion_gain(kind: f32, h: f32, u: f32, seed: f32, time: f32, beat: f32, sweep: f32) -> f32 {
    if kind < 0.5 {
        return 1.0;
    }
    let fast = facade.motion.y;
    let tau = 6.2831853;
    if kind < 2.5 {
        // Vagues : une bande qui monte (1) ou descend (2) de 190 m en 8 temps ; l'enveloppe la fait naître et mourir hors
        // de la vue, sans retour brusque. Le temps musical avance en continu (voir `FrameInputs::beat`).
        let phase = fract(beat / 8.0 + seed * 0.0371);
        let travel = select(1.0 - phase, phase, kind < 1.5) * 190.0;
        let envelope = smoothstep(0.0, 0.1, phase) * (1.0 - smoothstep(0.9, 1.0, phase));
        let band = motion_band(h - travel, 14.0) * envelope;
        return 1.0 + fast * (0.15 + 0.35 * sweep) * band;
    }
    if kind < 3.5 {
        // Dérive : des bandes lentes qui glissent le long de la rue, un cycle de 24 s, plus ou moins 15 %.
        let slide = sin(tau * (u / 40.0 - time / DRIFT_PERIOD_S + seed * 0.0371));
        return 1.0 + 0.15 * slide;
    }
    if kind < 4.5 {
        // Projecteur : un faisceau oblique qui balaie la façade en 12 s (bords de 1,8 s), un peu plus vif avec la voie
        // rapide limitée.
        let q = fract(u / 60.0 + h / 200.0 - time / SWEEP_PERIOD_S + seed * 0.0371);
        let band = smoothstep(0.0, 0.15, q) * (1.0 - smoothstep(0.15, 0.35, q));
        return 1.0 + (0.25 + 0.4 * fast * sweep) * band;
    }
    // Pulsation : une respiration lente de 4 s, plus le niveau limité de la voie rapide.
    let breath = 0.5 + 0.5 * sin(tau * (time / BREATH_PERIOD_S + seed * 0.0371));
    return 1.0 + 0.08 * breath + 0.4 * fast * sweep;
}
