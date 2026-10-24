// Surface d'un astre de Neon City : texture, jour et nuit, liseré de Fresnel et lueur au terminateur.
//
// Portage de `surfaceMaterial` (`engine/planets.ts`), mêmes constantes. Les astres sont éclairés par une étoile hors champ,
// pas par les lumières de la scène.

import bevy_pbr::render::forward_io::VertexOutput;
import bevy_pbr::render::mesh_view_bindings::view;

// Même ordre de champs que `PlanetParams` (Rust, `ShaderType`).
struct Planet {
    // xyz : direction du soleil hors champ (`uLight`), unitaire.
    light: vec4<f32>,
    // Teinte de l'atmosphère (`uAtmo`).
    atmo: vec4<f32>,
    // x : pouls, y : fondu du ciel (`uReveal`).
    frame: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> planet: Planet;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var map: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var map_sampler: sampler;

fn sstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge0 < edge1 {
        return smoothstep(edge0, edge1, x);
    }
    return 1.0 - smoothstep(edge1, edge0, x);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let pulse = planet.frame.x;
    let reveal = planet.frame.y;
    let albedo = textureSample(map, map_sampler, in.uv).rgb;
    let n = normalize(in.world_normal);
    let v = normalize(view.world_position - in.world_position.xyz);
    let ndl = dot(n, planet.light.xyz);
    let day = smoothstep(-0.1, 0.55, ndl);
    var col = albedo * (0.09 + 1.0 * day);
    // Atmosphère : liseré de Fresnel, plus vif côté éclairé, et lueur au terminateur.
    let fres = pow(1.0 - max(dot(n, v), 0.0), 3.2);
    col += planet.atmo.rgb * fres * (0.18 + 1.1 * smoothstep(-0.35, 0.6, ndl)) * (1.0 + pulse * 0.5);
    col += planet.atmo.rgb * 0.12 * sstep(0.25, 0.0, abs(ndl)) * (1.0 - fres);
    return vec4<f32>(col * reveal, reveal);
}
