// Socle des shaders des immeubles-vitrines (J7-08) : les uniformes d'image, le bruit, le brouillard de la ville et le
// vertex d'un plan de façade. Ce texte est placé en tête de chaque `landmark_*.wesl` (les imports doivent venir avant le
// reste) : un fichier par rendu, un seul bloc d'uniformes pour tous.
//
// Portage de `landmarks/overlay.ts` et `glsl.ts` de Sillage (`FRAME_GLSL`, `NOISE_GLSL`, `CITY_GLSL`, `PLANE_VERTEX`).
//
// Repères : `local` est la position dans le repère du maillage (`vP` de Sillage), `cam` la caméra dans ce même repère
// (`vCam`) : les façades s'en servent pour la parallaxe. Le repère de la pièce est une rotation à échelle uniforme, son
// inverse est donc la transposée divisée par le carré de l'échelle (WGSL n'a pas `inverse`).
//
// `uv` vers le haut : le maillage de la pièce est construit avec v = 1 en haut (voir `MeshData::quad`), comme `vUv`.

import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::mesh_functions::get_world_from_local;
import bevy_pbr::render::view_transformations::position_world_to_clip;

// Même ordre de champs que `LandmarkParams` (Rust, `ShaderType`). Tout en vec4.
struct Params {
    // Palette de la scène, dans l'ordre de `SceneColor::ALL` (lumière linéaire) : voir les constantes C_*.
    palette: array<vec4<f32>, 11>,
    // Teinte d'ambiance de la pièce (`uMood`).
    mood: vec4<f32>,
    // x : temps (s), y : pouls borné par le limiteur (`uPulse`), z : basses (`uBass`), w : médiums (`uMid`).
    frame: vec4<f32>,
    // x : aigus (`uHigh`), y : temps musical continu (`uBeat`), z : intensité (`uIntensity`), w : phase (`uPhase`).
    audio: vec4<f32>,
    // Paramètres propres au rendu : voir l'en-tête de chaque shader.
    a: vec4<f32>,
    b: vec4<f32>,
    c: vec4<f32>,
    d: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> p: Params;
// La texture de la pièce (atlas du mot ERROR, grille de l'arcade), ou le repli de Bevy.
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var map: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var map_sampler: sampler;

// Rangs de la palette (même ordre que `SceneColor::ALL` ; un test Rust compare).
const C_INK: u32 = 0u;
const C_VELVET: u32 = 1u;
const C_MINT: u32 = 2u;
const C_AZURE: u32 = 3u;
const C_VIOLET: u32 = 4u;
const C_ROSE: u32 = 5u;
const C_AMBER: u32 = 6u;
const C_ICE: u32 = 7u;
const C_ALARM: u32 = 8u;
const C_LIME: u32 = 9u;
const C_SUN: u32 = 10u;

fn color(index: u32) -> vec3<f32> {
    return p.palette[index].rgb;
}

// `smoothstep(edge0, edge1, x)` de GLSL accepte edge0 > edge1 ; WGSL l'interdit. 1 - smoothstep(e1, e0, x) donne la même
// courbe.
fn sstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge0 < edge1 {
        return smoothstep(edge0, edge1, x);
    }
    return 1.0 - smoothstep(edge1, edge0, x);
}

// `mod(x, y)` de GLSL (x - y * floor(x / y)), différent de `%` en WGSL pour les négatifs.
fn gl_mod(x: f32, y: f32) -> f32 {
    return x - y * floor(x / y);
}

fn h11(x: f32) -> f32 {
    return fract(sin(x * 127.1) * 43758.5453);
}

fn h21(q: vec2<f32>) -> f32 {
    var r = fract(q * vec2<f32>(123.34, 456.21));
    r += dot(r, r + 45.32);
    return fract(r.x * r.y);
}

fn vnoise(q: vec2<f32>) -> f32 {
    let i = floor(q);
    var f = fract(q);
    f = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(h21(i), h21(i + vec2<f32>(1.0, 0.0)), f.x),
        mix(h21(i + vec2<f32>(0.0, 1.0)), h21(i + vec2<f32>(1.0, 1.0)), f.x),
        f.y,
    );
}

fn fbm(q_in: vec2<f32>) -> f32 {
    var q = q_in;
    var a = 0.5;
    var s = 0.0;
    for (var i = 0; i < 4; i++) {
        s += a * vnoise(q);
        q = q * 2.03 + 7.1;
        a *= 0.5;
    }
    return s;
}

// Comme les gratte-ciel : les structures « poussent » du sol entre 300 et 440 m (aucune apparition brutale), et le
// brouillard les avale au loin.
fn city_vista() -> f32 {
    return 1.0 + 1.6 * smoothstep(35.0, 140.0, view.world_position.y);
}

fn city_grow(world: vec3<f32>) -> f32 {
    let v = city_vista();
    return 1.0 - smoothstep(300.0 * v, 440.0 * v, distance(world.xz, view.world_position.xz));
}

fn city_fog(col: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let f = smoothstep(90.0 * city_vista(), 380.0 * city_vista(), distance(world, view.world_position));
    return mix(col, color(C_INK) + color(C_VIOLET) * 0.05, f);
}

fn city_fade(world: vec3<f32>) -> f32 {
    return 1.0 - smoothstep(110.0 * city_vista(), 400.0 * city_vista(), distance(world, view.world_position));
}

// Position monde -> repère de la pièce, pour une matrice « rotation × échelle uniforme ».
fn to_local(m: mat4x4<f32>, world: vec3<f32>) -> vec3<f32> {
    let rel = world - m[3].xyz;
    let s2 = max(dot(m[0].xyz, m[0].xyz), 1.0e-6);
    return vec3<f32>(dot(m[0].xyz, rel), dot(m[1].xyz, rel), dot(m[2].xyz, rel)) / s2;
}

struct PlaneIn {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

struct PlaneOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world: vec3<f32>,
    @location(2) local: vec3<f32>,
    @location(3) cam: vec3<f32>,
    @location(4) normal: vec3<f32>,
}

// Vertex d'un plan de façade (`PLANE_VERTEX`) : repère local pour la parallaxe, croissance du sol, brouillard.
fn plane_vertex(v: PlaneIn) -> PlaneOut {
    let m = get_world_from_local(v.instance_index);
    var out: PlaneOut;
    out.uv = v.uv;
    out.local = v.position;
    out.cam = to_local(m, view.world_position);
    out.normal = v.normal;
    var wp = m * vec4<f32>(v.position, 1.0);
    wp.y *= city_grow(wp.xyz);
    out.world = wp.xyz;
    out.clip = position_world_to_clip(wp.xyz);
    return out;
}
