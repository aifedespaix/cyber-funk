// Anneaux d'une géante de Neon City : bandes concentriques de densités variées, avec une division nette.
//
// Portage de `ringMaterial` (`engine/planets.ts`). `vP = position.xz` de Sillage est lu dans le repère LOCAL de l'anneau,
// géométrie déjà basculée (`rotateX`) : le vertex shader transmet donc la position locale, comme le faisait three.js.

import bevy_pbr::render::mesh_functions::get_world_from_local;
import bevy_pbr::render::view_transformations::position_world_to_clip;

// Même ordre de champs que `RingParams` (Rust, `ShaderType`).
struct Ring {
    tint: vec4<f32>,
    // x : rayon extérieur (m), y : pouls, z : fondu du ciel (`uReveal`).
    geo: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> ring: Ring;

struct RingVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
}

struct RingVarying {
    @builtin(position) clip: vec4<f32>,
    @location(0) p: vec2<f32>,
}

@vertex
fn vertex(v: RingVertex) -> RingVarying {
    let world = get_world_from_local(v.instance_index) * vec4<f32>(v.position, 1.0);
    var out: RingVarying;
    out.clip = position_world_to_clip(world.xyz);
    out.p = v.position.xz;
    return out;
}

@fragment
fn fragment(in: RingVarying) -> @location(0) vec4<f32> {
    let outer = ring.geo.x;
    let pulse = ring.geo.y;
    let reveal = ring.geo.z;
    let r = length(in.p) / outer;
    // Anneaux concentriques de densités variées, avec une division nette.
    let bands = 0.45 + 0.35 * sin(r * 160.0) * sin(r * 47.0 + 1.3) + 0.2 * sin(r * 310.0);
    let gap = smoothstep(0.0, 0.015, abs(r - 0.72));
    let a = clamp(bands, 0.0, 1.0) * gap * 0.55;
    return vec4<f32>(ring.tint.rgb * (0.55 + pulse * 0.25) * a * reveal, a * reveal);
}
