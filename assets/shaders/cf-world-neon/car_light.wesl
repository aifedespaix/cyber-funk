// Lumières additives de la voiture : volumes de phares, flaques sur la route, halos, fumée, traînée.
//
// Portage des `ShaderMaterial` de `car-lights.ts` (faisceau, flaque) et des `Sprite` / `MeshBasicMaterial` de
// `cyber/glow.ts` (halos, lueurs), de la fumée de `tires.ts` et de la bande de `trail.ts`. Un seul shader, un seul
// matériau Rust : `params.x` choisit le motif. Aucune vraie lumière : des volumes additifs, comme dans Sillage, que le
// bloom fait rayonner (les valeurs dépassent 1 en lumière linéaire).
//
// Tous les maillages portent les quatre attributs (position, normale, UV, couleur) : leur disposition ne change pas.

import bevy_pbr::render::{
    mesh_functions,
    view_transformations::position_world_to_clip,
    mesh_view_bindings::view,
};

// `color` : teinte (rgb, lumière linéaire) ; `color.w` : opacité (`uK`).
// `params.x` : motif, `params.y` : écart des deux lobes de la flaque (`uSplit`).
struct Light {
    color: vec4<f32>,
    params: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> light: Light;

const KIND_BEAM: f32 = 0.0;
const KIND_POOL: f32 = 1.0;
const KIND_HALO: f32 = 2.0;
const KIND_GLOW: f32 = 3.0;
const KIND_SMOKE: f32 = 4.0;
const KIND_RIBBON: f32 = 5.0;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(5) color: vec4<f32>,
}

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) view_dir: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
}

@vertex
fn vertex(v: Vertex) -> VertexOut {
    var out: VertexOut;
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    var world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(v.position, 1.0));
    if light.params.x == KIND_HALO {
        // `Sprite` de three.js : un carré toujours face à la caméra, de la taille de l'échelle du nœud.
        let center = world_from_local[3].xyz;
        let size = vec2<f32>(length(world_from_local[0].xyz), length(world_from_local[1].xyz));
        let right = view.world_from_view[0].xyz;
        let up = view.world_from_view[1].xyz;
        world_position = vec4<f32>(center + right * (v.position.x * size.x) + up * (v.position.y * size.y), 1.0);
    }
    out.position = position_world_to_clip(world_position.xyz);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(v.normal, v.instance_index);
    out.view_dir = view.world_position - world_position.xyz;
    out.uv = v.uv;
    out.color = v.color;
    return out;
}

// Le dégradé radial des halos : `createRadialGradient` de `glow.ts` (1 au centre, 0,45 au quart, 0 au bord).
fn radial(d: f32) -> f32 {
    if d < 0.25 {
        return mix(1.0, 0.45, d / 0.25);
    }
    return mix(0.45, 0.0, clamp((d - 0.25) / 0.75, 0.0, 1.0));
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let kind = light.params.x;
    let k = light.color.w;
    if kind == KIND_BEAM {
        // Volume lumineux : plus dense au centre qu'aux bords, et qui s'éteint avec la distance. `uv.y` : 0 à la pointe
        // du faisceau, 1 au bout.
        let edge = pow(abs(dot(normalize(in.world_normal), normalize(in.view_dir))), 1.6);
        let along = in.uv.y;
        let fade = pow(1.0 - along, 1.7) * smoothstep(0.0, 0.05, along);
        return vec4<f32>(light.color.rgb, edge * fade * k);
    }
    if kind == KIND_POOL {
        // Deux flaques de lumière côte à côte (une par optique) qui s'étirent et s'éteignent loin de la voiture.
        // `uv.y` : 0 contre la voiture, 1 au bout de la flaque.
        let u = in.uv.x * 2.0 - 1.0;
        let f = in.uv.y;
        let split = (abs(u) - light.params.y) * 3.4;
        let lobe = exp(-split * split);
        let reach = pow(1.0 - f, 1.5) * smoothstep(0.0, 0.06, f);
        return vec4<f32>(light.color.rgb, lobe * reach * k);
    }
    if kind == KIND_HALO || kind == KIND_GLOW {
        let d = length(in.uv - vec2<f32>(0.5)) * 2.0;
        return vec4<f32>(light.color.rgb, radial(d) * k);
    }
    if kind == KIND_SMOKE {
        // Disque flou : couleur et opacité de chaque particule viennent de la couleur du sommet.
        let d = length(in.uv - vec2<f32>(0.5)) * 2.0;
        let soft = pow(1.0 - smoothstep(0.0, 1.0, d), 1.5);
        return vec4<f32>(in.color.rgb, in.color.a * soft);
    }
    // KIND_RIBBON : la traînée. La couleur du sommet porte déjà l'opacité (lumière additive).
    return vec4<f32>(in.color.rgb * light.color.rgb, 1.0);
}
