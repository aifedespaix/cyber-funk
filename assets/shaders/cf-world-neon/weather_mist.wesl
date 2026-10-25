// La nappe de brouillard posée sous la route : elle cache le pied des tours et donne la sensation que la route flotte très
// haut au-dessus du vide.
//
// Portage du fragment shader de `engine/mist.ts` de Sillage, mêmes constantes. Le motif est calculé sur la position monde
// rendue (`in.world_position`, relative à l'origine flottante de `cf-render`) : c'est le `vWorld` de Sillage (« position du
// maillage moins l'origine des motifs »).
//
// Qualité adaptative : `frame.w` (0 ou 1) allège le bruit, une seule couche à trois octaves au lieu de deux couches à
// quatre. La branche est uniforme sur tout le plan : elle ne coûte rien.

import bevy_pbr::render::forward_io::VertexOutput;
import bevy_pbr::render::mesh_view_bindings::view;

// Même ordre de champs que `MistParams` (Rust, `ShaderType`). Tout en vec4.
struct Mist {
    ink: vec4<f32>,
    base: vec4<f32>,
    // Teinte d'ambiance (`uGlow`) et sa version sombre (`uTint`).
    glow: vec4<f32>,
    tint: vec4<f32>,
    // x : temps (s), y : pouls (`uPulse`), z : éclair d'orage accordé par le limiteur, w : 1 = bruit allégé.
    frame: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> mist: Mist;

fn h2(p_in: vec2<f32>) -> f32 {
    var p = fract(p_in * vec2<f32>(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

fn n2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    var f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(h2(i), h2(i + vec2<f32>(1.0, 0.0)), f.x),
        mix(h2(i + vec2<f32>(0.0, 1.0)), h2(i + vec2<f32>(1.0, 1.0)), f.x),
        f.y,
    );
}

fn fbm(p_in: vec2<f32>, octaves: i32) -> f32 {
    var p = p_in;
    var v = 0.0;
    var a = 0.5;
    for (var i = 0; i < octaves; i++) {
        v += a * n2(p);
        p = p * 2.07 + 5.3;
        a *= 0.5;
    }
    return v;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let time = mist.frame.x;
    // Deux couches de brume qui dérivent en sens contraire.
    let p = in.world_position.xz * 0.012;
    var cloud: f32;
    if mist.frame.w > 0.5 {
        cloud = fbm(p + vec2<f32>(time * 0.012, -time * 0.02), 3);
    } else {
        cloud = fbm(p + vec2<f32>(time * 0.012, -time * 0.02), 4) * 0.65
            + fbm(p * 2.3 - vec2<f32>(time * 0.02, 0.0), 4) * 0.35;
    }
    var col = mist.base.rgb * (0.7 + cloud * 1.1);
    col += mix(mist.glow.rgb, mist.tint.rgb, cloud) * smoothstep(0.5, 0.9, cloud) * (0.12 + mist.frame.y * 0.1);
    // Un éclair d'orage allume les nuages : le niveau est celui que le limiteur photosensible a accordé.
    col += mist.glow.rgb * cloud * mist.frame.z;

    let dist = length(in.world_position.xyz - view.world_position);
    let fog = smoothstep(120.0, 520.0, dist);
    return vec4<f32>(mix(col, mist.ink.rgb + mist.glow.rgb * 0.05, fog), 1.0);
}
