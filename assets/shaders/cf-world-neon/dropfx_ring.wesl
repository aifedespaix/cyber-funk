// Onde de choc du drop (`RingGeometry(0.7, 0.9)` de `fx.ts`) : un anneau posé au sol qui s'élargit.
//
// Le quad fait 32 m de côté (`d` = 1 au bord du quad, soit 16 m) ; Rust écrit le rayon du centre de l'anneau et sa demi-épaisseur
// en fraction de ce rayon : l'anneau de Sillage grandit de 1 à 17 fois et son épaisseur avec lui (0,1 × l'échelle).
//
// `level` est un niveau **accordé** (voir `dropfx/safety.rs`) : jamais plus que le plafond de la voie de flashs, nul quand les
// flashs sont coupés. Bevy mélange `AlphaMode::Add` en alpha prémultiplié : couleur déjà multipliée, alpha nul.

import bevy_pbr::render::forward_io::VertexOutput;

// Même ordre de champs que `RingParams` (Rust, `ShaderType`).
struct Ring {
    tint: vec4<f32>,
    // x : niveau accordé, y : rayon du centre de l'anneau, z : demi-épaisseur (tous deux en fraction du demi-côté du quad).
    p: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> ring: Ring;

// 0,55 d'opacité de Sillage pour un plafond de 0,4 : le plafond de la voie, rapporté à la luminosité de l'anneau.
const GAIN: f32 = 1.4;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let d = length(in.uv - vec2<f32>(0.5)) * 2.0;
    let half_width = max(ring.p.z, 0.0001);
    let edge = 1.0 - smoothstep(half_width * 0.5, half_width, abs(d - ring.p.y));
    return vec4<f32>(ring.tint.rgb * (edge * ring.p.x * GAIN), 0.0);
}
