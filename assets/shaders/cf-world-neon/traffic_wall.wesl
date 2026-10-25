// Murs de lumière des motos : des rubans verticaux translucides, plus lumineux sur leurs arêtes haute et basse, qui
// s'éteignent au loin. Portage de `walls.ts`.
//
// Par sommet : `color.rgb` la teinte de la moto, `uv.x` l'opacité (âge), `uv.y` la hauteur (0 : pied, 1 : haut du mur).

import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::mesh_functions::get_world_from_local;
import bevy_pbr::render::view_transformations::position_world_to_clip;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(2) fade: vec2<f32>,
    @location(5) color: vec4<f32>,
}

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) fade: vec2<f32>,
    @location(2) world: vec3<f32>,
}

@vertex
fn vertex(v: Vertex) -> VertexOut {
    var out: VertexOut;
    let world = get_world_from_local(v.instance_index) * vec4<f32>(v.position, 1.0);
    out.clip = position_world_to_clip(world.xyz);
    out.color = v.color.rgb;
    out.fade = v.fade;
    out.world = world.xyz;
    return out;
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    // Un mur translucide, plus lumineux sur ses arêtes haute et basse, qui s'éteint au loin.
    let edge = pow(abs(in.fade.y * 2.0 - 1.0), 3.0);
    let far = 1.0 - smoothstep(160.0, 420.0, distance(in.world, view.world_position));
    let k = (0.3 + 0.9 * edge) * in.fade.x * far;
    // Lumière additive : `rgb × alpha` avec un alpha de 0 (voir `traffic_trail.wesl`).
    return vec4<f32>(in.color * k * 2.2 * k, 0.0);
}
