// Matériau des véhicules volants : coque sombre qui accroche les reflets, pièces d'accent et bandes néon à la teinte de
// l'instance, vitres, phares et gyrophares. Une seule fonction pour tous les modèles.
//
// Portage de `vehicle-material.ts` de Sillage (GLSL → WGSL/WESL), constantes et formules à l'identique.
//
// Données par sommet : la surface (`aKind`) est le canal rouge de la couleur du maillage.
// Données par instance : le `MeshTag` de chaque véhicule remplace `instanceColor` :
//   bits  0..7   rouge de la teinte (sRGB, 8 bits)
//   bits  8..15  vert
//   bits 16..23  bleu
//   bits 24..31  déphasage tiré à la naissance (`aFlash`, réservé)
//
// Écart avec Sillage (voir la PR) : le pouls des néons et l'éclat des gyrophares ne sont plus calculés ici à partir
// du temps et du kick, mais lus dans `anim` : ce sont les niveaux que rend le limiteur de flashs (règle 7, WCAG 2.3.1).
// Les gyrophares battent ensemble pour tous les véhicules.

import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::mesh_functions::{get_world_from_local, get_tag, mesh_normal_local_to_world};
import bevy_pbr::render::view_transformations::position_world_to_clip;

// Même ordre de champs que `VehicleParams` (Rust, `ShaderType`). Tout en vec4 : aucune surprise d'alignement.
struct Vehicle {
    // Palette de la scène (lumière linéaire, w inutilisé).
    ink: vec4<f32>,
    ice: vec4<f32>,
    azure: vec4<f32>,
    alarm: vec4<f32>,
    hull: vec4<f32>,
    hull_dark: vec4<f32>,
    haze: vec4<f32>,
    mist: vec4<f32>,
    // x : pouls des néons (0..1, rendu par le limiteur), y : gyrophare gauche (0..1), z : gyrophare droit (0..1).
    anim: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> vehicle: Vehicle;

// Altitude de la nappe de brouillard sous la route, en mètres.
const MIST_Y: f32 = -50.0;

const KIND_ACCENT: i32 = 2;
const KIND_NEON: i32 = 3;
const KIND_GLASS: i32 = 4;
const KIND_HEAD: i32 = 5;
const KIND_BEACON_LEFT: i32 = 6;

// `smoothstep(edge0, edge1, x)` de GLSL accepte edge0 > edge1 ; WGSL l'interdit.
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
    @location(1) normal: vec3<f32>,
    @location(5) color: vec4<f32>,
}

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world: vec3<f32>,
    @location(2) tint: vec3<f32>,
    @location(3) @interpolate(flat) kind: f32,
}

@vertex
fn vertex(v: Vertex) -> VertexOut {
    var out: VertexOut;
    let world_from_local = get_world_from_local(v.instance_index);
    let world = world_from_local * vec4<f32>(v.position, 1.0);
    out.clip = position_world_to_clip(world.xyz);
    out.world_normal = mesh_normal_local_to_world(v.normal, v.instance_index);
    out.world = world.xyz;
    let tag = get_tag(v.instance_index);
    let r = f32(tag & 255u) / 255.0;
    let g = f32((tag >> 8u) & 255u) / 255.0;
    let b = f32((tag >> 16u) & 255u) / 255.0;
    out.tint = vec3<f32>(srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b));
    out.kind = v.color.x;
    return out;
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let eye = view.world_position - in.world;
    let v = normalize(eye);
    let light = normalize(vec3<f32>(-0.4, 0.8, 0.5));
    let lambert = max(dot(n, light), 0.0);
    let fres = pow(1.0 - max(dot(n, v), 0.0), 3.0);
    let spec = pow(max(dot(reflect(-v, n), light), 0.0), 22.0);
    let k = i32(in.kind + 0.5);
    let pulse = vehicle.anim.x;
    let ice = vehicle.ice.rgb;
    var col: vec3<f32>;

    if k <= 1 {
        // Coque : sombre, un reflet net, la teinte du néon en liseré.
        var base = vehicle.hull.rgb;
        if k == 1 {
            base = vehicle.hull_dark.rgb;
        }
        col = base * (0.4 + 0.95 * lambert) + ice * spec * 0.55 + in.tint * (0.1 + fres * 0.5);
    } else if k == KIND_ACCENT {
        col = in.tint * (1.5 + pulse * 0.8);
    } else if k == KIND_NEON {
        col = in.tint * (2.5 + pulse * 1.2);
    } else if k == KIND_GLASS {
        col = vehicle.ink.rgb * 0.5 + vehicle.azure.rgb * 0.3 * fres + ice * spec * 1.4;
    } else if k == KIND_HEAD {
        col = ice * 3.4;
    } else {
        // Gyrophare : la moitié gauche (rouge) et la moitié droite (bleue) s'allument à tour de rôle, au rythme que
        // le limiteur de flashs accorde.
        var tint = vehicle.azure.rgb;
        var level = vehicle.anim.z;
        if k == KIND_BEACON_LEFT {
            tint = vehicle.alarm.rgb;
            level = vehicle.anim.y;
        }
        col = tint * (0.25 + level * 4.2);
    }

    let fog = smoothstep(90.0, 380.0, length(eye));
    let low = sstep(-4.0, MIST_Y + 2.0, in.world.y);
    col = mix(col, vehicle.mist.rgb, low);
    return vec4<f32>(mix(col, vehicle.ink.rgb + vehicle.haze.rgb * 0.05, fog), 1.0);
}
