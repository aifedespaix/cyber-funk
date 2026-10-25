// Surface d'un astre qui tombe : texture de planète, jour et nuit, face qui fend l'air chauffée, liseré de Fresnel.
//
// Portage de `surfaceMaterial` (`engine/meteor-materials.ts`), mêmes constantes. Le vertex shader bosselle la sphère : un
// bloc irrégulier, jusqu'à 17 % au-delà du rayon (l'`Aabb` de l'entité en tient compte).

import bevy_pbr::render::mesh_functions::get_world_from_local;
import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::view_transformations::position_world_to_clip;

// Même ordre de champs que `SurfaceParams` (Rust, `ShaderType`).
struct Surface {
    // Teinte de chaleur (`uTint`).
    tint: vec4<f32>,
    // xyz : direction du vol (`uDir`), unitaire.
    dir: vec4<f32>,
    // x : graine du bosselage (`uSeed`).
    seed: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> surface: Surface;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var map: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var map_sampler: sampler;

// Direction de la lumière du « jour » sur les astres (`METEOR_LIGHT` de Sillage, normalisée : (-0,38 ; 0,35 ; 0,86)).
const LIGHT: vec3<f32> = vec3<f32>(-0.3629, 0.3342, 0.8212);

struct SurfaceVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

struct SurfaceVarying {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) world: vec3<f32>,
}

@vertex
fn vertex(v: SurfaceVertex) -> SurfaceVarying {
    let seed = surface.seed.x;
    // Bosselage : la sphère devient un bloc irrégulier.
    let d = sin(v.position.x * 3.1 + seed) * sin(v.position.y * 2.7 + seed * 1.7) * sin(v.position.z * 3.3 + seed * 0.6);
    let p = v.position * (1.0 + 0.17 * d);
    let world_from_local = get_world_from_local(v.instance_index);
    let world = world_from_local * vec4<f32>(p, 1.0);
    var out: SurfaceVarying;
    out.clip = position_world_to_clip(world.xyz);
    out.uv = v.uv;
    out.normal = normalize((world_from_local * vec4<f32>(v.normal, 0.0)).xyz);
    out.world = world.xyz;
    return out;
}

@fragment
fn fragment(in: SurfaceVarying) -> @location(0) vec4<f32> {
    let albedo = textureSample(map, map_sampler, in.uv).rgb;
    let n = normalize(in.normal);
    let v = normalize(view.world_position - in.world);
    let day = smoothstep(-0.15, 0.6, dot(n, LIGHT));
    var col = albedo * (0.14 + 0.95 * day);
    // La face qui fend l'air chauffe ; un liseré de Fresnel finit le contour.
    let front = max(dot(n, surface.dir.xyz), 0.0);
    let fres = pow(1.0 - max(dot(n, v), 0.0), 3.0);
    col += surface.tint.rgb * (pow(front, 2.0) * 1.1 + fres * 1.3);
    return vec4<f32>(col, 1.0);
}
