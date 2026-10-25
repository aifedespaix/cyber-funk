// Traînée lumineuse des moteurs : deux plans en croix (horizontal et vertical) pour rester visible sous tous les angles,
// qui s'effilent vers l'arrière. Le plan s'étend de z = 0 (le véhicule) à z = 1 (le bout de la traînée).
//
// Portage de `createTrailMaterial` de `vehicle-material.ts`. La teinte vient du `MeshTag` du véhicule (voir
// `traffic_vehicle.wesl`). Lumière additive : `AlphaMode::Add` de Bevy mélange `src + (1 - src.a) × dst`
// (alpha prémultiplié), donc le fragment rend `rgb × alpha` avec un alpha de 0.

import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::mesh_functions::{get_world_from_local, get_tag};
import bevy_pbr::render::view_transformations::position_world_to_clip;

fn sstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge0 < edge1 {
        return smoothstep(edge0, edge1, x);
    }
    return 1.0 - smoothstep(edge1, edge0, x);
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
}

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) along: f32,
    @location(1) across: f32,
    @location(2) tint: vec3<f32>,
    @location(3) world: vec3<f32>,
}

@vertex
fn vertex(v: Vertex) -> VertexOut {
    var out: VertexOut;
    let world = get_world_from_local(v.instance_index) * vec4<f32>(v.position, 1.0);
    out.clip = position_world_to_clip(world.xyz);
    out.along = v.position.z;
    out.across = max(abs(v.position.x), abs(v.position.y)) * 2.0;
    out.world = world.xyz;
    let tag = get_tag(v.instance_index);
    out.tint = vec3<f32>(
        srgb_to_linear(f32(tag & 255u) / 255.0),
        srgb_to_linear(f32((tag >> 8u) & 255u) / 255.0),
        srgb_to_linear(f32((tag >> 16u) & 255u) / 255.0),
    );
    return out;
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    // `max(…, 0)` : l'interpolation peut dépasser 1 d'un ulp, et `pow` d'un négatif donne NaN, que le bloom étale.
    let body = pow(max(1.0 - in.along, 0.0), 1.7) * sstep(1.0, 0.1, in.across);
    let far = 1.0 - smoothstep(160.0, 420.0, distance(in.world, view.world_position));
    let a = body * far;
    return vec4<f32>(in.tint * a * 1.5 * a, 0.0);
}
