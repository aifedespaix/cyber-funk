// Façade des immeubles de Neon City : fenêtres alignées par étage, étages éteints, ceinture lumineuse, vague de lumière
// qui monte à chaque temps, scintillement piloté par les aigus, ruissellement sous la pluie.
//
// Portage de `building-material.ts` de Sillage (GLSL → WGSL/WESL), constantes et formules à l'identique. Tout se calcule en
// coordonnées locales de l'instance (pas de texture).
//
// Données par instance : le `MeshTag` de chaque pièce (voir `Part::tag`, Rust) remplace l'attribut `aInfo` et la couleur
// d'instance de Sillage :
//   bits  0..9   graine des fenêtres (`aInfo.x`)
//   bits 10..13  rang de la teinte dans le cycle prismatique (`instanceColor`)
//   bits 14..15  type : 0 corps, 1 couronne, 2 antenne, 3 tour d'arrière-plan (`aInfo.z`)
//   bits 16..23  étage de la ceinture lumineuse + 1, 0 : aucune (`aInfo.y`)
// Le décalage vertical du bloc (`aInfo.w`) est l'altitude de l'instance (sa base) : un corps commence à −BASE_DEPTH, une
// couronne à la hauteur du corps, ce qui prolonge ses étages.
//
// Le champ de la ville (`field.wesl`, ajouté au bout de ce fichier, J7-08) : `field_power` éteint les fenêtres derrière le
// front d'une panne de courant, `field_light` répand la lueur des immeubles-vitrines, `field_zone` porte leur glitch
// (bandes aux canaux inversés, scanlines). Sans immeuble-vitrine ils valent 1, 0 et 0, comme dans Sillage.
//
// Écart avec Sillage (voir la PR) : les pièces portent un `MeshTag` au lieu d'attributs d'instance. Le rang de teinte du
// tag va de 0 à 15 : 0..5 le cycle prismatique, 6..15 les couleurs de la scène (`SceneColor` sans l'encre), pour les corps
// des immeubles-vitrines (lime, alarme, ambre…).
//
// Les dérivées (`fwidth`) sont calculées en tête, avant tout `discard` et tout retour anticipé : WGSL les exige en flot de
// contrôle uniforme.

import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::mesh_functions::{get_world_from_local, get_tag};
import bevy_pbr::render::view_transformations::position_world_to_clip;

// Même ordre de champs que `FacadeParams` (Rust, `ShaderType`). Tout en vec4 : aucune surprise d'alignement.
struct Facade {
    // Palette de la scène (lumière linéaire, w inutilisé).
    ink: vec4<f32>,
    ice: vec4<f32>,
    amber: vec4<f32>,
    haze: vec4<f32>,
    mist: vec4<f32>,
    scan: vec4<f32>,
    // La teinte d'une tour : 0..5 le cycle prismatique, 6..15 les couleurs de la scène (J7-08).
    cycle: array<vec4<f32>, 16>,
    // x : temps (s), y : pouls (`uPulse`), z : basses (`uBass`), w : médiums (`uMid`).
    frame: vec4<f32>,
    // x : aigus (`uHigh`), y : temps musical continu (`uBeat`), z : pré-intro (`uBuild`), w : éclair d'orage (`uFlash`).
    audio: vec4<f32>,
    // x : humidité (`uWet`), y : pluie (`uRain`), z : pouls des fenêtres (`uWinPulse`).
    weather: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> facade: Facade;

// Hauteur d'un étage et largeur d'une travée de fenêtres, en mètres.
const FLOOR_HEIGHT: f32 = 2.4;
const BAY_WIDTH: f32 = 1.3;
// Altitude de la nappe de brouillard sous la route, en mètres.
const MIST_Y: f32 = -50.0;

// `smoothstep(edge0, edge1, x)` de GLSL accepte edge0 > edge1 ; WGSL l'interdit. 1 - smoothstep(e1, e0, x) donne la même
// courbe.
fn sstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge0 < edge1 {
        return smoothstep(edge0, edge1, x);
    }
    return 1.0 - smoothstep(edge1, edge0, x);
}

fn h21(p_in: vec2<f32>) -> f32 {
    var p = fract(p_in * vec2<f32>(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    var f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(h21(i), h21(i + vec2<f32>(1.0, 0.0)), f.x),
        mix(h21(i + vec2<f32>(0.0, 1.0)), h21(i + vec2<f32>(1.0, 1.0)), f.x),
        f.y,
    );
}

// Les tours « poussent » du sol entre 300 et 440 m : aucune apparition brutale (`cityGrow`). Même formule dans
// `facade_prepass.wesl`.
fn grow_at(world: vec3<f32>) -> f32 {
    return 1.0 - smoothstep(300.0, 440.0, distance(world.xz, view.world_position.xz));
}

struct FacadeVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

struct FacadeVarying {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal_w: vec3<f32>,
    @location(2) @interpolate(flat) normal_l: vec3<f32>,
    @location(3) local: vec3<f32>,
    @location(4) @interpolate(flat) size: vec3<f32>,
    // x : graine, y : hauteur de l'étage des projecteurs (-1 si aucun), z : type, w : décalage vertical du bloc.
    @location(5) @interpolate(flat) info: vec4<f32>,
    @location(6) @interpolate(flat) tint: vec3<f32>,
}

@vertex
fn vertex(v: FacadeVertex) -> FacadeVarying {
    let m = get_world_from_local(v.instance_index);
    let tag = get_tag(v.instance_index);
    var out: FacadeVarying;
    out.size = vec3<f32>(length(m[0].xyz), length(m[1].xyz), length(m[2].xyz));
    out.local = v.position * out.size;
    out.normal_l = v.normal;
    out.normal_w = normalize((m * vec4<f32>(v.normal, 0.0)).xyz);
    var wp = m * vec4<f32>(v.position, 1.0);
    wp.y *= grow_at(wp.xyz);
    out.world = wp.xyz;
    let spot_floor = (tag >> 16u) & 0xFFu;
    let spot_y = select(-1.0, (f32(spot_floor) - 0.5) * FLOOR_HEIGHT, spot_floor > 0u);
    out.info = vec4<f32>(f32(tag & 0x3FFu), spot_y, f32((tag >> 14u) & 3u), m[3].y);
    out.tint = facade.cycle[(tag >> 10u) & 0xFu].rgb;
    out.clip = position_world_to_clip(wp.xyz);
    return out;
}

@fragment
fn fragment(in: FacadeVarying) -> @location(0) vec4<f32> {
    let time = facade.frame.x;
    let pulse = facade.frame.y;
    let bass = facade.frame.z;
    let mid = facade.frame.w;
    let high = facade.audio.x;
    let beat = facade.audio.y;
    let build = facade.audio.z;
    let flash = facade.audio.w;
    let wet = facade.weather.x;
    let rain = facade.weather.y;
    let win_pulse = facade.weather.z;
    let ink = facade.ink.rgb;
    let ice = facade.ice.rgb;
    let amber = facade.amber.rgb;
    let mist = facade.mist.rgb;
    let scan = facade.scan.rgb;
    let v_c = in.tint;
    let cam = view.world_position;

    // Coordonnées de façade (travées, étages), calculées pour toutes les faces : les dérivées exigent un flot uniforme.
    let along_x = abs(in.normal_l.x) > 0.5;
    let u = select(in.local.x, in.local.z, along_x);
    let width = select(in.size.x, in.size.z, along_x);
    let h = in.local.y + in.info.w;
    let uu = u / BAY_WIDTH + 64.0;
    let vv = h / FLOOR_HEIGHT;
    let px = max(fwidth(uu), fwidth(vv));

    // Révélation holographique (pré-intro) : la ville monte, les tours proches d'abord ; une ligne de balayage marque la
    // coupe.
    var build_top = 1.0e5;
    if build < 0.999 {
        let nearness = clamp(distance(in.world.xz, cam.xz) / 380.0, 0.0, 1.0);
        build_top = mix(-70.0, 340.0, clamp(build * 1.45 - 0.45 * nearness, 0.0, 1.0));
        if in.world.y > build_top {
            discard;
        }
    }
    // Entier arrondi : une graine interpolée varierait d'un pixel à l'autre et bruiterait le hachage.
    let seed = floor(in.info.x + 0.5);
    let tier = in.info.z;
    let n = normalize(in.normal_w);
    let haze = ink + facade.haze.rgb * 0.05;
    let far = tier > 2.5;
    let dist = length(in.world - cam);

    // Antenne : un trait lumineux dont la pointe bat avec le kick.
    if tier > 1.5 && !far {
        let tip = smoothstep(0.6, 1.0, in.local.y / in.size.y);
        let c = v_c * (0.5 + tip * (2.2 + pulse * 3.0));
        let f = smoothstep(120.0, 380.0, dist);
        return vec4<f32>(mix(c, haze, f), 1.0);
    }

    let lambert = max(dot(n, normalize(vec3<f32>(-0.4, 0.8, 0.5))), 0.0);
    var col = ink * 0.9 + ice * (0.014 + 0.04 * lambert) + v_c * 0.012;
    // Pluie : la façade s'assombrit, devient brillante, et l'eau ruisselle en traînées verticales.
    col *= 1.0 - 0.25 * wet;

    let roof = in.normal_l.y > 0.5;
    if !roof && in.normal_l.y > -0.5 {
        let face = select(select(4.0, 3.0, in.normal_l.z > 0.0), select(2.0, 1.0, in.normal_l.x > 0.0), along_x);
        let floor_id = floor(vv);
        let f = vec2<f32>(fract(uu), fract(vv));

        // Étages éteints : une rangée sur six environ est un étage technique, noir.
        let floor_rnd = h21(vec2<f32>(floor_id, seed * 0.77 + face));
        let dead = step(floor_rnd, 0.17);

        // Fenêtres : plus il y a d'aigus, plus il y en a d'allumées.
        let cell = vec2<f32>(floor(uu), floor_id);
        let window_rnd = h21(cell + vec2<f32>(seed * 0.37 + face * 3.1, seed * 0.11));
        let lit_chance = 0.5 + high * 0.25 + 0.2 * h21(vec2<f32>(seed * 0.29, floor_id));
        var lit = step(window_rnd, lit_chance) * (1.0 - dead);
        if far {
            // Plages de lumière sur de larges blocs de façade : rien ne scintille quand les fenêtres deviennent sous-pixel.
            let zone = vec2<f32>(floor(uu / 7.0), floor(vv / 6.0));
            lit = (0.2 + 0.8 * step(h21(zone + vec2<f32>(seed * 0.37 + face * 3.1, seed * 0.11)), lit_chance)) * (1.0 - dead);
        }
        // Quelques fenêtres clignotent en rythme avec les médiums.
        let blink = select(step(0.93, h21(cell + 9.7 + seed * 0.13)), 0.0, far);
        lit *= mix(1.0, step(0.5, fract(time * 1.5 + window_rnd * 6.0)) * (0.4 + mid), blink);

        // Pouls de la ville : sur chaque temps, un autre lot de fenêtres éteintes s'allume (événement « city-pulse »).
        lit = max(lit, win_pulse * step(h21(cell + 13.3 + floor(beat) * 1.7), 0.55) * (1.0 - dead));

        // Plus loin, les fenêtres se fondent en une teinte moyenne (pas de scintillement).
        let fade_out = select(smoothstep(0.12, 0.45, px), 1.0, far);
        let frame = step(0.16, f.x) * step(f.x, 0.84) * step(0.2, f.y) * step(f.y, 0.8);
        let mask = mix(frame, 0.34, fade_out);

        let warm = mix(v_c, amber, step(0.7, h21(cell + 31.0)));
        let window_color = mix(warm, ice, step(0.9, h21(cell + 57.0)));

        // Vague de lumière qui monte d'un cran à chaque kick.
        let travel = fract(beat / 8.0 + seed * 0.0371) * 190.0;
        let wave_d = (h - travel) / 9.0;
        let wave = select(exp(-wave_d * wave_d), 0.0, far);
        let glow = 0.55 + wave * (1.0 + pulse * 0.9) + pulse * 0.2 + win_pulse * 0.6;
        col += window_color * lit * mask * glow * (1.0 + 0.45 * wet) * field_power(in.world);

        // Ruissellement : des filets d'eau qui descendent, éclairés par les fenêtres et la teinte de la tour.
        let run_h = h21(vec2<f32>(floor(uu * 1.7) + seed * 0.3 + face, 3.0));
        let run = run_h * run_h;
        if !far {
            let rivulet = smoothstep(0.55, 1.0, vnoise(vec2<f32>(uu * 1.7 + face * 5.0, vv * 0.12 + time * (0.25 + run * 0.5) + seed)))
                * smoothstep(0.3, 0.9, run);
            col += mix(v_c, ice, 0.4) * rivulet * wet * (0.12 + 0.35 * lit + pulse * 0.1) * (1.0 - fade_out);
        }
        // Vitre mouillée : un reflet blanc diffus des lumières voisines.
        col += ice * 0.05 * wet * lambert;

        // Pied d'immeuble éclairé par la route, façon halo.
        col += v_c * exp(-max(h, 0.0) / 7.0) * (0.04 + pulse * 0.1);

        // Ceinture lumineuse à l'étage des projecteurs.
        let spot = step(0.0, in.info.y) * sstep(0.7, 0.2, abs(h - in.info.y));
        col += mix(ice, v_c, 0.35) * spot * (1.2 + pulse * 1.6);

        // Arêtes verticales en néon sur une partie des tours.
        let edge = sstep(0.14, 0.0, min(abs(u - width * 0.5), abs(u + width * 0.5)));
        let trimmed = step(0.45, fract(seed * 0.0531));
        if !far {
            col += v_c * edge * trimmed * (0.7 + pulse * 1.0 + bass * 0.4);
        }
    } else if roof {
        // Toit : bordure néon et lueur sourde.
        let half_size = in.size.xz * 0.5;
        let rim = sstep(0.35, 0.0, min(half_size.x - abs(in.local.x), half_size.y - abs(in.local.z)));
        col += v_c * rim * (0.7 + pulse * 1.3);
    } else {
        // Dessous (tablier des arches) : rampes lumineuses transversales.
        let ramp = sstep(0.05, 0.0, abs(fract(in.local.z / 6.0) - 0.5) - 0.42);
        col += mix(ice, v_c, 0.5) * ramp * (0.8 + pulse * 1.2);
    }

    // Lumières dynamiques des immeubles-vitrines, et leur zone de glitch (inversion de canaux, scanlines).
    if !far {
        col += (ice * 0.1 + v_c * 0.06) * field_light(in.world, n) * (1.0 + wet * 2.5);
    }
    let zw = field_zone(in.world);
    let zone_band = step(0.92, fract(in.world.y * 0.045 + time * 0.7));
    let zone_scan = step(0.5, fract(in.world.y * 0.9 - time * 3.0));
    col = mix(col, col.brg * 1.3, zw * zone_band);
    col += field.zone_tint.rgb * zw * (0.04 + 0.12 * zone_scan);

    // Éclair d'orage : la façade blanchit d'un coup, du côté éclairé.
    col += ice * flash * (0.25 + 0.5 * lambert);

    // Sous la route, la nappe de brouillard avale les façades.
    let low = sstep(-4.0, MIST_Y + 2.0, in.world.y);
    col = mix(col, mist, low);

    if build < 0.999 {
        let d = (in.world.y - build_top) / 1.6;
        col += scan * (exp(-d * d) * 3.0 + 0.3 * smoothstep(build_top - 22.0, build_top, in.world.y));
    }

    let fog = smoothstep(90.0 - 30.0 * rain, 380.0 - 100.0 * rain, dist);
    return vec4<f32>(mix(col, haze, fog), 1.0);
}
