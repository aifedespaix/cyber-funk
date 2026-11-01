// La traîne de la voiture sur les façades : les fenêtres allumées gardent un peu de la lumière de la voiture là où elle
// est passée, puis la rendent en quelques secondes.
//
// Ce texte est ajouté au bout de `facade.wesl` (voir `FACADE_SOURCE` dans `material.rs`) : il lit `facade.trail` et
// `facade.car_tint`. Aucune lumière dynamique, aucun état par immeuble : seize points en uniformes, rien d'autre.
//
// Sécurité photosensible (règle 7 de `CLAUDE.md`, WCAG 2.3.1) : chaque point s'éteint en 6 s (`TRAIL_LIFETIME_S` côté
// Rust) et la lueur s'étale sur 40 m (`TRAIL_FALLOFF_M`) ; à 60 m/s une façade la voit monter et retomber en plusieurs
// secondes, sans front. Ne pas réduire ces deux bornes.

const TRAIL_POINTS: i32 = 16;
// Demi-largeur de la lueur autour d'un point, en mètres.
const TRAIL_FALLOFF_M: f32 = 40.0;
// Part de la couleur de la voiture dans la teinte de la fenêtre.
const TRAIL_TINT_SHARE: f32 = 0.25;
// Lumière ajoutée à une fenêtre allumée au cœur de la traîne (la fenêtre au repos vaut environ 0,55).
const TRAIL_GAIN: f32 = 0.9;

// Part de lueur au point `p` (repère rendu) : le maximum, sur les points, de l'intensité restante par un fondu gaussien de
// la distance. L'altitude compte pour moitié : la lueur monte le long d'une tour sans l'envahir.
fn trail_glow(p: vec3<f32>) -> f32 {
    var best = 0.0;
    for (var i = 0; i < TRAIL_POINTS; i = i + 1) {
        let t = facade.trail[i];
        let d = (p - t.xyz) * vec3<f32>(1.0, 0.5, 1.0);
        let x = length(d) / TRAIL_FALLOFF_M;
        best = max(best, t.w * exp(-x * x));
    }
    return best;
}

// Lumière que la traîne ajoute à une fenêtre allumée : sa couleur, teintée d'un quart par celle de la voiture.
fn trail_window_light(p: vec3<f32>, window_color: vec3<f32>) -> vec3<f32> {
    let strength = facade.car_tint.w;
    if strength <= 0.0 {
        return vec3<f32>(0.0);
    }
    return mix(window_color, facade.car_tint.rgb, TRAIL_TINT_SHARE) * (trail_glow(p) * strength * TRAIL_GAIN);
}
