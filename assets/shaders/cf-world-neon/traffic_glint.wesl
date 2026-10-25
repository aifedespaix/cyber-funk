// Points lumineux additifs : gyrophares, drones, feux de position, lampadaires. Un seul dessin pour tous.
//
// Portage de `glints.ts` : les `gl_PointSize` de three.js deviennent des carrés toujours face à la caméra, dont la taille
// à l'écran suit la même formule (`taille × pixels par mètre / profondeur`, bornée à 120 px).
//
// Un sommet par coin : `position` est le centre du point (relatif à l'entité), `color.rgb` sa couleur déjà multipliée
// par l'intensité, `color.a` son diamètre en mètres, `uv` le coin (−1 ou 1 sur chaque axe).

import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::mesh_functions::get_world_from_local;
import bevy_pbr::render::view_transformations::position_world_to_clip;

const MAX_POINT_PX: f32 = 120.0;

fn sstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge0 < edge1 {
        return smoothstep(edge0, edge1, x);
    }
    return 1.0 - smoothstep(edge1, edge0, x);
}

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(2) corner: vec2<f32>,
    @location(5) color: vec4<f32>,
}

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) color: vec3<f32>,
}

@vertex
fn vertex(v: Vertex) -> VertexOut {
    var out: VertexOut;
    let world = get_world_from_local(v.instance_index) * vec4<f32>(v.position, 1.0);
    var clip = position_world_to_clip(world.xyz);
    // Pixels par mètre à un mètre de distance : `viewportHeight / (2 tan(fov / 2))`.
    let scale = view.clip_from_view[1][1] * view.viewport.w * 0.5;
    let diameter_px = clamp(v.color.a * scale / max(clip.w, 1.0), 0.0, MAX_POINT_PX);
    let ndc = v.corner * (diameter_px / view.viewport.zw);
    clip = vec4<f32>(clip.xy + ndc * clip.w, clip.z, clip.w);
    out.clip = clip;
    out.corner = v.corner;
    out.color = v.color.rgb;
    return out;
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let d = length(in.corner);
    let a = exp(-d * d * 5.0) + sstep(1.0, 0.0, d) * 0.15;
    // Lumière additive : `rgb × alpha` avec un alpha de 0 (voir `traffic_trail.wesl`).
    return vec4<f32>(in.color * a * a, 0.0);
}
