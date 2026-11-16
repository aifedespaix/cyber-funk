// Surface de la route (chantier C4 ; P2 §1.1, DA §1.2 et §2.2) : hiérarchie des fenêtres reflétées, rugosité réactive,
// aberration des reflets, flaque de lumière sous le sujet, ondes de kick sur l'eau.
//
// Ce texte est ajouté au bout de `road.wesl` (voir `ROAD_SOURCE` dans `material.rs`) : il lit le bloc `scene`
// d'`atmosphere.wesl`. Miroir Rust : `road/surface.rs` (testé, constantes comparées).

// La hiérarchie des fenêtres que le sol reflète : mêmes valeurs que `WINDOW_*` de `facade_light.wesl`.
const ROAD_WINDOW_SHARE_CALM: f32 = 0.18;
const ROAD_WINDOW_SHARE_DELTA: f32 = 0.42;
const ROAD_WINDOW_ON_FLOOR: f32 = 0.9;
const ROAD_WINDOW_OFF_FLOOR: f32 = 0.03;
// Rugosité réactive (C4, P2 §1.1) : ce que le kick ajoute à l'étalement d'un reflet de flaque, et ce qu'un break retire
// au relief de l'eau (les flaques deviennent un miroir net).
const ROAD_ROUGH_KICK: f32 = 0.035;
const ROAD_CALM_MIRROR: f32 = 0.6;
// Aberration des reflets (C4) : écart angulaire entre le reflet rouge et le reflet bleu, le long de la route.
const ROAD_ABERRATION: f32 = 0.012;

// Étalement angulaire du reflet. `pudk` : part de flaque (0 asphalte, 1 flaque), `kick` : frémissement (0..1), `rain`.
fn road_roughness(pudk: f32, kick: f32, rain: f32) -> f32 {
    return mix(0.09, 0.006, pudk) + pudk * (ROAD_ROUGH_KICK * kick + 0.5 * ROAD_ROUGH_KICK * rain);
}

// Ce qui reste du relief de l'eau : le calme (0..1) aplanit les flaques, pas l'asphalte.
fn road_calm_relief(pudk: f32, calm: f32) -> f32 {
    return 1.0 - ROAD_CALM_MIRROR * clamp(calm, 0.0, 1.0) * pudk;
}

// Flaque de lumière sous le sujet : rayon (m) et gain.
const ROAD_POOL_SIGMA_M: f32 = 6.0;
const ROAD_POOL_GAIN: f32 = 0.12;
// Ondes de kick sur l'eau : vitesse du front (m/s), épaisseur (m), durée de vie (s), pente maximale.
const ROAD_KICK_SPEED: f32 = 14.0;
const ROAD_KICK_WIDTH: f32 = 0.9;
const ROAD_KICK_LIFE: f32 = 0.7;
const ROAD_KICK_SLOPE: f32 = 0.05;

// Pente ajoutée à la surface de l'eau par les quatre derniers kicks (âges dans `scene.kick_age`, source `scene.source`).
// La pente est la dérivée radiale d'une gaussienne : elle s'annule sur la crête et s'éteint avec l'âge.
fn kick_ripples(p: vec2<f32>) -> vec2<f32> {
    let to = p - scene.source.xz;
    let r = max(length(to), 0.001);
    var slope = 0.0;
    for (var i = 0; i < 4; i++) {
        let age = scene.kick_age[i];
        let x = (r - ROAD_KICK_SPEED * age) / ROAD_KICK_WIDTH;
        slope += -x * exp(-x * x) * exp(-age / ROAD_KICK_LIFE);
    }
    return (to / r) * slope * ROAD_KICK_SLOPE * scene.shell.w * 2.0;
}
