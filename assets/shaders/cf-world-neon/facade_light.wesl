// Matière des façades (chantier C4 ; DA §1.1, §2.1, §2.2 ; P2 §1.2) : fenêtres hiérarchisées, tubes néon à cœur blanc,
// halo des tubes proches sur les murs, chanfrein lumineux de toit.
//
// Ce texte est ajouté au bout de `facade.wesl` (voir `FACADE_SOURCE` dans `material.rs`) : il lit `h21` de ce fichier et
// le bloc `scene` d'`atmosphere.wesl`. Miroir Rust : `facade/windows.rs` (un test compare les constantes).

// Albédo du béton (DA §2.2) : la part des lumières de scène que le corps d'une tour renvoie.
const FACADE_ALBEDO: f32 = 0.18;
// Fenêtres hiérarchisées (C4) : part des étages allumés au calme, ce que `Δ` y ajoute, et chance d'une fenêtre d'être
// allumée dans un étage allumé ou éteint. Miroir Rust : `facade/windows.rs`.
const WINDOW_SHARE_CALM: f32 = 0.18;
const WINDOW_SHARE_DELTA: f32 = 0.42;
const WINDOW_ON_FLOOR: f32 = 0.9;
const WINDOW_OFF_FLOOR: f32 = 0.03;
// Intensité log-normale d'une fenêtre : palier E1 et écart-type du logarithme.
const WINDOW_E1: f32 = 0.8;
const WINDOW_SIGMA: f32 = 0.6;

// Joints de banche du béton : un tous les 3 m.
const FACADE_JOINT_M: f32 = 3.0;
// Façades égaliseur : part des tours, et largeur du fondu au sommet d'une colonne (en part de la hauteur).
const METER_SHARE: f32 = 0.125;
const METER_SOFT: f32 = 0.02;

// Une fenêtre du vu-mètre est allumée si son étage (0 au pied, 1 au sommet) est sous le niveau de sa bande.
fn meter_lit(floor_level: f32, level: f32) -> f32 {
    return 1.0 - smoothstep(level - METER_SOFT, level + METER_SOFT, floor_level);
}

// Tubes néon (C4, DA §2.2) : rayon des arêtes et des bordures de toit (m), palier E2 au repos (en θ_b), part du halo.
const NEON_EDGE_RADIUS: f32 = 0.07;
const NEON_ROOF_RADIUS: f32 = 0.17;
const NEON_E2: f32 = 1.5;
const NEON_HALO: f32 = 0.35;

// Profil d'un tube à la distance `d` de son axe : `cœur = 1 − smoothstep(r/2, r, d)`, le cœur vire au blanc (la glace de
// la palette), le halo reste saturé et s'éteint en `exp(−d/4r)`. Le blanc ne couvre que le cœur.
fn neon_tube(c: vec3<f32>, white: vec3<f32>, d: f32, r: f32) -> vec3<f32> {
    let core = 1.0 - smoothstep(0.5 * r, r, d);
    return mix(c, white, core * core) * core + c * exp(-d / (4.0 * r)) * NEON_HALO;
}

// Part des étages allumés : 0,18 + 0,42·Δ, décalée par le tempérament de l'immeuble.
fn window_share(delta: f32, bias: f32) -> f32 {
    return clamp(WINDOW_SHARE_CALM + WINDOW_SHARE_DELTA * clamp(delta, 0.0, 1.0) + bias, 0.02, 0.95);
}

// Gain log-normal d'une fenêtre, `n` gaussien (somme de trois hachages, bornée à ±3).
fn window_gain(n: f32) -> f32 {
    return WINDOW_E1 * exp(WINDOW_SIGMA * clamp(n, -3.0, 3.0));
}

// Plafond d'une fenêtre : aucun canal ne dépasse `cap` (0,9·θ_b), la teinte est gardée.
fn window_cap(c: vec3<f32>, cap: f32) -> vec3<f32> {
    let top = max(c.r, max(c.g, c.b));
    return c * min(1.0, cap / max(top, 0.0001));
}

// Bande de caisse claire : un mur ne monte que d'un palier sous la musique, elle reste donc en E1 (≤ 0,7·θ_b).
const SNARE_BAND_E1: f32 = 0.7;

// Halo des tubes sur un mur (P2 §1.2) : longueur d'extinction ℓ et distance de demi-intensité d0, en mètres. Les valeurs
// de P2 (6 m et 1,2 m) valent pour une enseigne posée sur le mur ; les tubes du bloc sont dans la rue, à une quinzaine de
// mètres des façades, d'où des distances plus longues.
const HALO_LENGTH_M: f32 = 18.0;
const HALO_HALF_M: f32 = 5.0;

// Atténuation du halo à la distance `d` : `exp(−d/ℓ)/(1 + (d/d0)²)`.
fn halo_falloff(d: f32) -> f32 {
    let q = d / HALO_HALF_M;
    return exp(-d / HALO_LENGTH_M) / (1.0 + q * q);
}

// `L_halo = Σ c_i·exp(−d_i/ℓ)/(1 + (d_i/d0)²)·(0,7 + 0,3·grain)` pour les quatre tubes du bloc, du côté des faces qui les
// regardent. À multiplier par l'albédo du mur.
fn wall_halo(p: vec3<f32>, n: vec3<f32>, grain: f32) -> vec3<f32> {
    var acc = vec3<f32>(0.0);
    for (var i = 0; i < 4; i++) {
        let to = scene.tube_pos[i].xyz - p;
        let d = max(length(to), 0.01);
        let facing = 0.3 + 0.7 * max(dot(n, to / d), 0.0);
        acc += scene.tube_col[i].rgb * scene.tube_pos[i].w * halo_falloff(d) * facing;
    }
    return acc * (0.7 + 0.3 * clamp(grain, 0.0, 1.0));
}

// Chanfrein de toit : un immeuble sur trois, un filet de 1,5 pixel (au moins 6 cm) sous l'arête du sommet. `top_d` est la
// distance au sommet (m), `px_m` la taille d'un pixel sur la façade (m).
const CHAMFER_SHARE: f32 = 0.3333;
const CHAMFER_MIN_M: f32 = 0.06;

fn roof_chamfer(top_d: f32, px_m: f32, seed: f32) -> f32 {
    let w = max(1.5 * px_m, CHAMFER_MIN_M);
    return (1.0 - smoothstep(w, 2.0 * w, top_d)) * step(h21(vec2<f32>(seed * 0.43, 17.0)), CHAMFER_SHARE);
}
