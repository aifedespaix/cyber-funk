// Les lumières additives d'un astre et de son explosion : queue, halo de chaleur, anneau de brume, éclair, onde de choc.
//
// Portage des cinq matériaux additifs de `engine/meteor-materials.ts`. Un seul shader, le type de lumière est dans
// l'uniforme (`kind`) : la branche est uniforme sur tout le quad, elle ne coûte rien.
//
// `level` remplace le `uFade` de Sillage : c'est toujours un niveau **accordé** (voir `meteors/flash.rs`), jamais une
// demande. Pour l'éclair il vaut au plus `MAX_FLASH_PEAK`.

import bevy_pbr::render::forward_io::VertexOutput;

// Même ordre de champs que `FxParams` (Rust, `ShaderType`).
struct Fx {
    tint: vec4<f32>,
    // x : niveau (`uFade`, `uK`), y : rayon de l'onde (`uR`), z : type de lumière.
    p: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> fx: Fx;

const KIND_TRAIL: u32 = 0u;
const KIND_GLOW: u32 = 1u;
const KIND_SPLASH: u32 = 2u;
const KIND_FLASH: u32 = 3u;

// `smoothstep` de GLSL accepte des bords décroissants (edge0 > edge1) ; WGSL non.
fn sstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge0 < edge1 {
        return smoothstep(edge0, edge1, x);
    }
    return 1.0 - smoothstep(edge1, edge0, x);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let tint = fx.tint.rgb;
    let level = fx.p.x;
    let kind = u32(fx.p.z + 0.5);
    let d = length(in.uv - vec2<f32>(0.5)) * 2.0;

    if kind == KIND_TRAIL {
        // x = 0 à la tête (côté astre), 1 au bout ; la queue s'affine en s'éloignant.
        let along = 1.0 - in.uv.x;
        let spread = mix(0.5, 0.08, in.uv.x);
        let width = sstep(spread, 0.0, abs(in.uv.y - 0.5));
        let a = pow(along, 1.6) * width * level;
        // Le cœur de la queue blanchit près de la tête : un métal qui brûle.
        let col = mix(tint, vec3<f32>(1.0), pow(along, 5.0) * 0.55);
        return vec4<f32>(col * a * 1.6, a);
    }
    if kind == KIND_GLOW {
        // Halo de chaleur centré sur l'astre : il déborde de la sphère sur toute sa circonférence.
        let a = pow(max(1.0 - d, 0.0), 2.4) * level;
        return vec4<f32>(tint * 1.5, a);
    }
    if kind == KIND_SPLASH {
        // Anneau de brume à l'impact dans la nappe de brouillard.
        let ring = sstep(0.18, 0.0, abs(d - 0.7)) + sstep(1.0, 0.0, d) * 0.25;
        let a = ring * level * (1.0 - smoothstep(0.85, 1.0, d));
        return vec4<f32>(tint * a * 1.8, a);
    }
    if kind == KIND_FLASH {
        // Éclair de l'explosion : cœur blanc, halo dans la teinte de l'astre.
        let core = exp(-d * d * 7.0);
        let halo = pow(max(1.0 - d, 0.0), 2.0);
        let col = mix(tint, vec3<f32>(1.0), core) * (core * 3.2 + halo * 1.3);
        return vec4<f32>(col, (core + halo * 0.8) * level);
    }
    // Onde de choc : un anneau qui s'élargit et s'amincit (`p.y` : son rayon, 0 → 1).
    let r = fx.p.y;
    let ring = sstep(0.1 + 0.08 * (1.0 - r), 0.0, abs(d - r * 0.92));
    return vec4<f32>(mix(tint, vec3<f32>(1.0), 0.12) * 3.2, ring * level);
}
