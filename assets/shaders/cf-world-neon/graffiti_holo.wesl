// Projecteur holographique : une barre émettrice et, au-dessus, un texte néon qui flotte dans l'air (atlas holographique
// cuit à l'ouverture, `graffiti/atlas.rs`). Additif, sans écriture de profondeur : l'immeuble qui passe devant masque le
// texte, la brume l'atténue.
//
// Le même shader dessine les deux pièces d'un projecteur : `tint.w` = 1 pour la barre, 0 pour le panneau. Le panneau est un
// carré vertical qui va du dessus de la barre au haut des lettres : le haut (part `shape.x` de sa hauteur) porte la cellule
// de l'atlas, le bas porte le faisceau, un dégradé qui s'évase de la barre vers les lettres.
//
// Règle 7 de `CLAUDE.md` : rien ne clignote ici. `state.y` (le niveau) est celui que le limiteur de flashs a accordé côté CPU
// (voir `graffiti/holo_systems.rs`). Toute l'instabilité (tranches décalées, aberration chromatique, lueur) est
// proportionnelle à ce niveau et ne change qu'à `TICK_HZ` par seconde au plus ; à 0 (flashs coupés, respiration, panne du
// limiteur) le texte est stable. Seules les lignes de balayage bougent, lentement et sans à-coup.
//
// Mélange « alpha prémultiplié » : une sortie à alpha nul s'additionne à la scène.

import bevy_pbr::render::forward_io::VertexOutput;
import bevy_pbr::render::mesh_view_bindings::view;

// Changements de tranches par seconde au plus (WCAG 2.3.1 : trois flashs par seconde).
const TICK_HZ: f32 = 3.0;
// Nombre de tranches horizontales du texte.
const SLICES: f32 = 16.0;
// Part de tranches décalées et décalage maximal (part de la largeur) à plein niveau.
const HIT_SHARE: f32 = 0.35;
const JUMP_MAX: f32 = 0.06;
// Écart des canaux rouge et bleu (part de la largeur) : fixe, plus un supplément proportionnel au niveau.
const SHIFT_BASE: f32 = 0.0015;
const SHIFT_LEVEL: f32 = 0.004;
// Émissif du cœur des lettres (> 1 : le bloom existant s'en empare) et de la barre.
const CORE_GAIN: f32 = 3.0;
const BAR_GAIN: f32 = 2.2;
// Part de lumière en plus de la barre et des lettres à plein niveau (la modulation reste bornée par le limiteur).
const LEVEL_GAIN: f32 = 0.35;
// Largeur du front du balayage d'apparition.
const SWEEP_EDGE: f32 = 0.08;

// Même ordre de champs que `HoloParams` (Rust, `ShaderType`).
struct Holo {
    // rgb : teinte (lumière linéaire), w : 1 pour la barre, 0 pour le panneau du texte.
    tint: vec4<f32>,
    // x : apparition 0..1, y : niveau du limiteur 0..1, z : temps (s), w : graine 0..1.
    state: vec4<f32>,
    // x : part de la hauteur du panneau portée par la cellule (en haut), y : largeur de la barre / largeur du panneau, z : force
    // du halo, w : force du faisceau.
    shape: vec4<f32>,
    // Rectangle du masque net dans l'atlas : u0, v0, largeur, hauteur (0..1) ; w : couche.
    sharp: vec4<f32>,
    // Idem pour le halo flou.
    halo: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> holo: Holo;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var atlas: texture_2d_array<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var atlas_sampler: sampler;

fn holo_h11(x: f32) -> f32 {
    return fract(sin(x * 127.1) * 43758.5453);
}

// Lit un masque de l'atlas à la position `a` (0..1 dans la cellule), 0 hors de la cellule.
fn holo_mask(rect: vec4<f32>, a: vec2<f32>) -> f32 {
    if a.x < 0.0 || a.x > 1.0 {
        return 0.0;
    }
    let p = rect.xy + a * rect.zw;
    return textureSampleLevel(atlas, atlas_sampler, p, i32(rect.w), 0.0).r;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let appear = holo.state.x;
    let level = holo.state.y;
    let t = holo.state.z;
    let tint = holo.tint.rgb;
    let is_bar = holo.tint.w > 0.5;
    let uv = in.uv;
    let haze = 1.0 - 0.8 * smoothstep(120.0, 380.0, distance(in.world_position.xyz, view.world_position));

    // Apparition : un balayage de bas en haut (la barre, elle, s'allume en fondu).
    var reveal = appear;
    if !is_bar {
        reveal = smoothstep(0.0, SWEEP_EDGE, appear * (1.0 + SWEEP_EDGE) - (1.0 - uv.y));
    }
    if reveal <= 0.0 {
        return vec4<f32>(0.0);
    }

    if is_bar {
        // Quelques motifs lents le long de la barre ; la lumière suit le niveau, bornée par le limiteur.
        let streak = 0.9 + 0.1 * sin(uv.x * 24.0 - t * 1.5 + holo.state.w * 6.28);
        return vec4<f32>(tint * BAR_GAIN * streak * (1.0 + LEVEL_GAIN * level) * reveal * haze, 0.0);
    }

    var col = vec3<f32>(0.0);
    let letters = holo.shape.x;
    if uv.y < letters {
        let a = vec2<f32>(uv.x, uv.y / letters);
        // Tranches décalées : au plus TICK_HZ changements par seconde, nuls à niveau 0.
        let slice = floor(a.y * SLICES);
        let tick = floor(t * TICK_HZ);
        let hit = step(1.0 - HIT_SHARE * level, holo_h11(slice * 7.3 + tick + holo.state.w * 31.0));
        let jump = (holo_h11(slice + tick * 1.7 + holo.state.w * 17.0) - 0.5) * JUMP_MAX * level * hit;
        let shift = SHIFT_BASE + SHIFT_LEVEL * level;
        var core = vec3<f32>(0.0);
        for (var c = 0; c < 3; c++) {
            core[c] = holo_mask(holo.sharp, vec2<f32>(a.x + jump + (f32(c) - 1.0) * shift, a.y));
        }
        // Qualité réduite : `shape.z` = 0 coupe le halo (valeur uniforme, la lecture est sautée).
        var glow = 0.0;
        if holo.shape.z > 0.0 {
            glow = holo_mask(holo.halo, vec2<f32>(a.x + jump, a.y));
        }
        // Lignes de balayage : lentes et continues, sans à-coup.
        let scan = 0.9 + 0.1 * sin(a.y * 160.0 - t * 2.0);
        let hot = mix(tint, vec3<f32>(1.0), 0.45);
        col = (hot * core.g + tint * (core.r + core.b) * 0.5) * CORE_GAIN * scan;
        col += tint * glow * holo.shape.z;
        col *= 1.0 + LEVEL_GAIN * level;
    } else {
        // Faisceau : plus étroit et plus vif près de la barre, il s'évase et s'éteint vers les lettres.
        let k = (uv.y - letters) / max(1.0 - letters, 0.001);
        let half = mix(0.5, holo.shape.y * 0.5, k);
        let edge = 1.0 - smoothstep(half * 0.6, half, abs(uv.x - 0.5));
        col = tint * holo.shape.w * k * k * edge;
    }
    return vec4<f32>(col * reveal * haze, 0.0);
}
