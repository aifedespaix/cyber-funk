// Gerbe d'étincelles du drop (`Points` de `fx.ts`, `size: 0.18`, `sizeAttenuation`, couleurs par sommet) : des carrés tournés
// vers la caméra, de côté fixe en mètres. Les quatre sommets d'une étincelle sont à son centre (relatif au point du drop, qui
// est la position de l'entité) ; le vertex shader les écarte selon les axes de la vue. La couleur, éclat accordé compris, est
// portée par le sommet.

import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::mesh_functions::get_world_from_local;
import bevy_pbr::render::view_transformations::position_world_to_clip;

// Demi-côté d'une étincelle (m) : `size: 0.18` de Sillage.
const HALF_M: f32 = 0.09;

struct SparkVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    // Coin du quad, 0 ou 1.
    @location(2) corner: vec2<f32>,
    @location(5) color: vec4<f32>,
}

struct SparkVarying {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
}

@vertex
fn vertex(v: SparkVertex) -> SparkVarying {
    let center = (get_world_from_local(v.instance_index) * vec4<f32>(v.position, 1.0)).xyz;
    let right = view.world_from_view[0].xyz;
    let up = view.world_from_view[1].xyz;
    let c = (v.corner * 2.0 - 1.0) * HALF_M;
    var out: SparkVarying;
    out.clip = position_world_to_clip(center + right * c.x + up * c.y);
    out.color = v.color.rgb;
    return out;
}

@fragment
fn fragment(in: SparkVarying) -> @location(0) vec4<f32> {
    // Carré plein ; alpha nul : Bevy mélange `AlphaMode::Add` en alpha prémultiplié, la couleur s'ajoute telle quelle.
    return vec4<f32>(in.color, 0.0);
}
