// Champ d'étoiles de Neon City : taille en pixels, scintillement, éclat additionnel sur les kicks, croix de diffraction
// sur les plus grosses.
//
// Portage de `createStars` (`engine/sky-materials.ts`). Sillage dessinait des `Points` ; ici chaque étoile est un quad dont
// les quatre sommets sont au centre de l'étoile, et le vertex shader les écarte de `taille / 2` pixels (l'équivalent d'un
// `gl_PointSize`). Les teintes viennent de la palette courante, choisie par l'indice porté par le sommet : pas de couleurs
// cuites à refaire quand le thème change.

import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::mesh_functions::get_world_from_local;
import bevy_pbr::render::view_transformations::position_world_to_clip;

// Même ordre de champs que `StarsParams` (Rust, `ShaderType`).
struct Stars {
    // glace, glace, azur, menthe, ambre, corail.
    tints: array<vec4<f32>, 6>,
    // x : temps (s), y : pouls, z : fondu du ciel (`uReveal`).
    frame: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> stars: Stars;

// Hauteur de référence de l'image, en pixels : la taille des étoiles de Sillage (1 à 8,5 px) est celle d'un écran de
// 1 080 px de haut ; elle suit la résolution du rendu au lieu de rester à 1 px en 4K.
const REFERENCE_HEIGHT: f32 = 1080.0;

struct StarVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    // Coin du quad, 0 ou 1.
    @location(2) corner: vec2<f32>,
    // x : taille en pixels (`aSize`), y : phase (`aPhase`).
    @location(3) size_phase: vec2<f32>,
    // x : indice de teinte, y : atténuation de la couche.
    @location(5) attr: vec4<f32>,
}

struct StarVarying {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
    @location(2) big: f32,
    @location(3) color: vec3<f32>,
}

fn sstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge0 < edge1 {
        return smoothstep(edge0, edge1, x);
    }
    return 1.0 - smoothstep(edge1, edge0, x);
}

@vertex
fn vertex(v: StarVertex) -> StarVarying {
    let time = stars.frame.x;
    let pulse = stars.frame.y;
    let size = v.size_phase.x;
    let phase = v.size_phase.y;
    let world = get_world_from_local(v.instance_index) * vec4<f32>(v.position, 1.0);
    var clip = position_world_to_clip(world.xyz);

    let twinkle = 0.65 + 0.35 * sin(time * (0.6 + phase * 2.0) + phase * 60.0);
    var out: StarVarying;
    out.alpha = twinkle * (0.55 + pulse * 0.35 * step(0.6, phase));
    out.big = smoothstep(3.0, 6.0, size);
    let side_px = size * (view.viewport.w / REFERENCE_HEIGHT) * (1.0 + pulse * 0.25 * out.big);
    // Un demi-côté de `side_px / 2` pixels, en coordonnées d'écran normalisées (2 unités = la largeur de la vue).
    clip = vec4<f32>(clip.xy + (v.corner * 2.0 - 1.0) * side_px / view.viewport.zw * clip.w, clip.zw);
    out.clip = clip;
    out.uv = v.corner;
    let tint = stars.tints[u32(v.attr.x + 0.5)].rgb;
    out.color = tint * v.attr.y;
    return out;
}

@fragment
fn fragment(in: StarVarying) -> @location(0) vec4<f32> {
    let reveal = stars.frame.z;
    let p = in.uv - 0.5;
    let r = length(p);
    var core = sstep(0.5, 0.0, r);
    core *= core;
    // Les plus grosses étoiles portent une croix de diffraction.
    let cross_arms = (sstep(0.03, 0.0, abs(p.x)) + sstep(0.03, 0.0, abs(p.y))) * sstep(0.5, 0.0, r) * in.big;
    let a = (core + cross_arms * 0.6) * in.alpha;
    return vec4<f32>(in.color * a * 2.0 * reveal, a * reveal);
}
