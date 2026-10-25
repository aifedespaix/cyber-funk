// Panneaux des structures d'événements : dalle et piliers de la route croisée, méga-tour et ses rails, portiques et
// rideaux, danseur géant, aurore. Un seul shader pour les deux couches (le plein, opaque ; la lumière, additive) : le
// motif est choisi par panneau.
//
// Portage des `ShaderMaterial` de `crossroad.ts` (`slabMaterial`), `tower.ts` (`railsMaterial`), `portals.ts` (`CURTAIN`),
// `dancer.ts` (`DANCER`), `hologram.ts` (`HOLO_MAIN`, `holoBug`), `aurora.ts` et `block-set.ts`, constantes à l'identique.
//
// Par sommet :
//   position  relative à l'origine de l'image (la voiture) ; l'entité suit cette origine ;
//   normal    normale unitaire de la face ;
//   uv        position dans le panneau, 0..1 ;
//   uv_b      dimensions réelles du panneau (largeur, hauteur), en mètres ;
//   color     rgb : la couleur déjà multipliée par l'éclat (pour la dalle, les rails et l'aurore : `r` = pouls accordé, plus
//             la quantité de l'aurore) ; a : le motif en partie entière (`KIND_*`), son paramètre `0..1` en partie fractionnaire.
//
// Écarts (voir la PR) :
//  - règle 7 : le pouls n'est jamais calculé ici, il arrive dans `color` (le niveau du limiteur de flashs) ;
//  - le « bug » de l'hologramme (bandes qui se décalent, canaux écartés, morceaux éteints, vacillement) ne change plus
//    d'état 16 fois par seconde mais 2,5 fois (`GLITCH_HZ` : au plus trois changements par seconde, WCAG 2.3.1), et son
//    vacillement général est atténué (0,5 → 0,3) ;
//  - les piliers et le corps de la méga-tour n'ont pas le matériau des façades de la ville.
//
// Lumière additive : `AlphaMode::Add` de Bevy mélange `src + (1 - src.a) × dst` (alpha prémultiplié), donc les motifs
// additifs rendent `rgb × alpha` avec un alpha de 0 (voir `traffic_trail.wesl`).

import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::mesh_functions::get_world_from_local;
import bevy_pbr::render::view_transformations::position_world_to_clip;

// Même ordre de champs que `PanelParams` (Rust, `ShaderType`). Tout en vec4 : aucune surprise d'alignement.
struct Panel {
    // Palette de la scène (lumière linéaire, w inutilisé).
    ink: vec4<f32>,
    ice: vec4<f32>,
    azure: vec4<f32>,
    rose: vec4<f32>,
    mint: vec4<f32>,
    violet: vec4<f32>,
    lime: vec4<f32>,
    // Nappe de brouillard sous la route : l'azur et le violet, assombris.
    mist: vec4<f32>,
    // x : temps (s), y : temps musical continu en battements (`uBeat`).
    frame: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> panel: Panel;

const KIND_BODY: i32 = 0;
const KIND_SLAB_TOP: i32 = 1;
const KIND_SLAB_SIDE: i32 = 2;
const KIND_POST: i32 = 3;
const KIND_RAILS: i32 = 4;
const KIND_CURTAIN: i32 = 5;
const KIND_DANCER: i32 = 6;

// Altitude de la nappe de brouillard sous la route, en mètres.
const MIST_Y: f32 = -50.0;
// Cadence maximale des changements d'état du « bug » de l'hologramme (WCAG 2.3.1 : au plus trois par seconde).
const GLITCH_HZ: f32 = 2.5;
// Demi-étendue du « corps » de la voiture pour le bug : large, haute et longue comme elle, avec une marge.
const CAR_REACH: f32 = 5.5;
const PI: f32 = 3.14159265;

// `smoothstep(edge0, edge1, x)` de GLSL accepte edge0 > edge1 ; WGSL l'interdit.
fn sstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge0 < edge1 {
        return smoothstep(edge0, edge1, x);
    }
    return 1.0 - smoothstep(edge1, edge0, x);
}

fn h11(x: f32) -> f32 {
    return fract(sin(x * 127.1) * 43758.5453);
}

fn h21(p_in: vec2<f32>) -> f32 {
    var p = fract(p_in * vec2<f32>(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

// Bruit de valeur 1D (aurore).
fn n1(x: f32) -> f32 {
    let i = floor(x);
    var f = fract(x);
    f = f * f * (3.0 - 2.0 * f);
    return mix(h11(i), h11(i + 1.0), f);
}

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(5) color: vec4<f32>,
}

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec3<f32>,
    @location(3) world: vec3<f32>,
    @location(4) normal: vec3<f32>,
    // La voiture, en coordonnées du monde rendu : l'entité est posée sur l'origine de l'image, qui est la voiture.
    @location(5) car: vec3<f32>,
    @location(6) @interpolate(flat) kind: f32,
}

@vertex
fn vertex(v: Vertex) -> VertexOut {
    var out: VertexOut;
    let model = get_world_from_local(v.instance_index);
    let world = model * vec4<f32>(v.position, 1.0);
    out.clip = position_world_to_clip(world.xyz);
    out.uv = v.uv;
    out.size = v.size;
    out.color = v.color.rgb;
    out.world = world.xyz;
    out.normal = v.normal;
    out.car = model[3].xyz;
    out.kind = v.color.a;
    return out;
}

// --- Corps d'immeuble : encre, fenêtres, arêtes néon à la teinte du panneau --------------------------------------------

fn body(in: VertexOut, param: f32, dist: f32) -> vec3<f32> {
    let ink = panel.ink.rgb;
    let ice = panel.ice.rgb;
    let tint = in.color;
    let n = in.normal;
    let lambert = max(dot(n, normalize(vec3<f32>(-0.4, 0.8, 0.5))), 0.0);
    var col = ink * 0.9 + ice * (0.014 + 0.04 * lambert) + tint * 0.012;
    if abs(n.y) < 0.5 {
        // Face latérale : travées de 1,3 m, étages de 2,4 m ; une fenêtre sur deux environ est allumée.
        let m = in.uv * in.size;
        let uu = m.x / 1.3;
        let vv = m.y / 2.4;
        let cell = vec2<f32>(floor(uu), floor(vv));
        let f = vec2<f32>(fract(uu), fract(vv));
        let rnd = h21(cell + vec2<f32>(param * 37.0, param * 11.0));
        let lit = step(rnd, 0.5);
        let dead = step(h21(vec2<f32>(cell.y, param * 77.0)), 0.17);
        let frame = step(0.16, f.x) * step(f.x, 0.84) * step(0.2, f.y) * step(f.y, 0.8);
        // Plus loin, les fenêtres se fondent en une teinte moyenne.
        let far = smoothstep(120.0, 300.0, dist);
        let mask = mix(frame, 0.34, far);
        let window_color = mix(tint, panel.mint.rgb, step(0.8, h21(cell + 57.0)));
        col += window_color * lit * (1.0 - dead) * mask * 0.9;
        // Arêtes verticales en néon.
        let edge = sstep(0.14, 0.0, min(m.x, in.size.x - m.x));
        col += tint * edge * 0.9;
    } else {
        // Toit : bordure néon.
        let m = in.uv * in.size;
        let rim = sstep(0.35, 0.0, min(min(m.x, in.size.x - m.x), min(m.y, in.size.y - m.y)));
        col += tint * rim * 0.9;
    }
    // Pied d'immeuble éclairé par la route.
    col += tint * exp(-max(in.world.y, 0.0) / 7.0) * 0.04;
    return col;
}

// --- Dalle de la route croisée -------------------------------------------------------------------------------------

fn slab_top(in: VertexOut) -> vec3<f32> {
    let pulse = in.color.r;
    let half_length = in.size.x * 0.5;
    let half_width = in.size.y * 0.5;
    let x = (in.uv.x - 0.5) * in.size.x;
    let dz = (in.uv.y - 0.5) * in.size.y;
    var col = panel.ink.rgb * 0.85;
    let edge_color = mix(panel.azure.rgb, panel.rose.rgb, 0.5 + 0.5 * sin(x * 0.02 + panel.frame.x * 0.4));
    col += edge_color * sstep(0.4, 0.0, abs(abs(dz) - (half_width - 0.5))) * (1.2 + pulse * 1.2);
    col += panel.ice.rgb * sstep(0.18, 0.0, abs(abs(dz) - 0.45)) * 0.6;
    let dash = step(0.5, fract(x / 9.0));
    col += panel.azure.rgb * dash * sstep(0.14, 0.0, abs(abs(dz) - 4.1)) * 0.9;
    let travel = fract(panel.frame.y / 8.0) * in.size.x - half_length;
    let along = (x - travel) / 16.0;
    col += panel.mint.rgb * exp(-along * along) * (0.35 + pulse) * sstep(9.4, 0.0, abs(dz));
    return col;
}

fn slab_side(in: VertexOut) -> vec3<f32> {
    let pulse = in.color.r;
    var col = panel.ink.rgb * 0.85 + panel.azure.rgb * 0.05;
    if in.normal.y < -0.5 {
        // Sous la dalle : deux rails lumineux.
        let x = (in.uv.x - 0.5) * in.size.x;
        let dz = (in.uv.y - 0.5) * in.size.y;
        let edge_color = mix(panel.azure.rgb, panel.rose.rgb, 0.5 + 0.5 * sin(x * 0.02 + panel.frame.x * 0.4));
        col += edge_color * sstep(0.35, 0.0, abs(abs(dz) - 3.2)) * (0.9 + pulse);
    }
    return col;
}

// --- Rails de la méga-tour (additif) -------------------------------------------------------------------------------

fn rails(in: VertexOut, dist: f32) -> vec3<f32> {
    let pulse = in.color.r;
    let lanes = 5.0;
    let spacing = 13.0;
    let z = (in.uv.x - 0.5) * in.size.x;
    let y = in.uv.y * in.size.y;
    // Voie la plus proche, son centre et la distance à celui-ci.
    let lane = clamp(floor(z / spacing + lanes * 0.5), 0.0, lanes - 1.0);
    let centre = (lane + 0.5 - lanes * 0.5) * spacing;
    let d = abs(z - centre);
    let in_lane = 1.0 - step(5.0, d);
    let dirn = select(-1.0, 1.0, lane % 2.0 < 0.5);
    let tint = mix(panel.mint.rgb, panel.rose.rgb, fract(lane * 0.37 + 0.2));
    let rail = sstep(0.35, 0.0, abs(d - 4.3));
    let dash = step(0.62, fract(y / 14.0 - panel.frame.x * 0.9 * dirn + lane * 0.31));
    let centre_line = sstep(0.2, 0.0, d) * dash;
    let wave_d = (y - fract(panel.frame.y / 16.0 * dirn + lane * 0.13) * in.size.y) / 18.0;
    let wave = exp(-wave_d * wave_d);
    let a = rail * (0.9 + pulse) + centre_line * 0.8 + in_lane * wave * (0.25 + pulse * 0.6) + in_lane * 0.05;
    let fade = 1.0 - smoothstep(180.0, 480.0, dist);
    return tint * a * fade * 1.5 * clamp(a * fade, 0.0, 1.0);
}

// --- Hologrammes : rideau de portique, danseur ---------------------------------------------------------------------

fn sd_seg(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    return length(pa - ba * clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0));
}

fn soft(d: f32, w: f32) -> f32 {
    return 1.0 - smoothstep(-w, w, d);
}

// Rideau de péage : lignes de balayage horizontales, grille fine, trait qui monte, bords vifs.
fn curtain_shape(uv: vec2<f32>, t: f32) -> f32 {
    let lines = smoothstep(0.9, 1.0, sin(uv.y * 70.0 + t * 2.5));
    let grid = smoothstep(0.93, 1.0, sin(uv.x * 110.0)) * 0.5;
    let scan_d = (fract(t * 0.55) - uv.y) * 12.0;
    let scan = exp(-scan_d * scan_d);
    let edge = sstep(0.035, 0.0, min(uv.x, 1.0 - uv.x));
    let hem = sstep(0.03, 0.0, uv.y);
    return 0.1 + lines * 0.3 + grid * lines + scan * 0.9 + edge * 0.9 + hem * 0.7;
}

// La silhouette du danseur : tête, buste, bras et jambes en segments épais, qui suivent le tempo ; un anneau de projection
// aux pieds. `aspect` : largeur sur hauteur du plan.
fn dancer_shape(uv: vec2<f32>, t: f32, aspect: f32, pulse: f32) -> f32 {
    let q = vec2<f32>((uv.x - 0.5) * aspect, uv.y);
    let ph = PI * (panel.frame.y + (1.0 - pulse));
    let sw = sin(ph);
    let cs = cos(ph);
    let bob = abs(cs) * 0.016 + pulse * 0.01;
    let hip = vec2<f32>(sw * 0.018, 0.33 + bob);
    let chest = hip + vec2<f32>(sw * 0.014, 0.2);
    let head = chest + vec2<f32>(sw * 0.02, 0.105);
    let sh_l = chest + vec2<f32>(-0.045, -0.012);
    let sh_r = chest + vec2<f32>(0.045, -0.012);
    let hand_l = sh_l + vec2<f32>(-0.1 - 0.04 * sw, 0.1 + 0.13 * sw);
    let hand_r = sh_r + vec2<f32>(0.1 + 0.04 * cs, 0.1 + 0.13 * cs);
    let elb_l = (sh_l + hand_l) * 0.5 + vec2<f32>(-0.012, -0.03);
    let elb_r = (sh_r + hand_r) * 0.5 + vec2<f32>(0.012, -0.03);
    let knee_l = hip + vec2<f32>(-0.032 - 0.02 * sw, -0.12);
    let knee_r = hip + vec2<f32>(0.032 - 0.02 * sw, -0.12);
    let foot_l = knee_l + vec2<f32>(-0.012, -0.12 + max(0.0, sw) * 0.05);
    let foot_r = knee_r + vec2<f32>(0.012, -0.12 + max(0.0, -sw) * 0.05);
    var d = sd_seg(q, hip, chest) - 0.032;
    d = min(d, length(q - head) - 0.035);
    d = min(d, sd_seg(q, sh_l, elb_l) - 0.014);
    d = min(d, sd_seg(q, elb_l, hand_l) - 0.012);
    d = min(d, sd_seg(q, sh_r, elb_r) - 0.014);
    d = min(d, sd_seg(q, elb_r, hand_r) - 0.012);
    d = min(d, sd_seg(q, hip, knee_l) - 0.018);
    d = min(d, sd_seg(q, knee_l, foot_l) - 0.015);
    d = min(d, sd_seg(q, hip, knee_r) - 0.018);
    d = min(d, sd_seg(q, knee_r, foot_r) - 0.015);
    let line = exp(-max(d, 0.0) * 120.0);
    let fill = soft(d + 0.004, 0.01) * (0.28 + 0.2 * sin(q.y * 140.0 - t * 5.0));
    // Anneau de projection aux pieds, et colonne de lumière qui monte de l'anneau.
    let ring = exp(-abs(length(vec2<f32>(q.x, (q.y - 0.07) * 3.2)) - 0.26) * 55.0);
    let column = sstep(0.26, 0.0, abs(q.x)) * sstep(0.78, 0.06, q.y) * 0.08 * step(0.07, q.y);
    return line * 1.2 + fill + ring * 0.8 + column;
}

fn holo_shape(kind: i32, uv: vec2<f32>, t: f32, aspect: f32, pulse: f32) -> f32 {
    if kind == KIND_DANCER {
        return dancer_shape(uv, t, aspect, pulse);
    }
    return curtain_shape(uv, t);
}

// 0 loin de la voiture, 1 en plein dedans.
fn holo_bug(world: vec3<f32>, car: vec3<f32>) -> f32 {
    let d = (world - car) / vec3<f32>(CAR_REACH, CAR_REACH * 0.7, CAR_REACH * 1.9);
    return 1.0 - smoothstep(0.55, 1.5, length(d));
}

fn holo(in: VertexOut, kind: i32, dist: f32) -> vec3<f32> {
    let t = panel.frame.x;
    let gain = in.color.r;
    let bug = holo_bug(in.world, in.car);
    // Le bug change d'état `GLITCH_HZ` fois par seconde au plus.
    let tick = floor(t * GLITCH_HZ);
    let band = floor(in.uv.y * 34.0);
    let hit = step(0.5, h11(band * 7.1 + tick)) * bug;
    var uv = in.uv;
    uv.x += (h11(band + tick * 3.3) - 0.5) * 0.4 * hit;
    // Aberration : les canaux s'écartent, davantage au cœur du bug.
    let split = 0.006 + 0.07 * bug * (0.4 + 0.6 * h11(tick));
    let aspect = in.size.x / max(in.size.y, 1.0);
    // Pouls du danseur : l'éclat du kick accordé tient dans `color.g`.
    let pulse = in.color.g;
    let r = holo_shape(kind, uv + vec2<f32>(split, 0.0), t, aspect, pulse);
    let g = holo_shape(kind, uv, t, aspect, pulse);
    let b = holo_shape(kind, uv - vec2<f32>(split, 0.0), t, aspect, pulse);
    // Morceaux qui s'éteignent par à-coups, et vacillement général (adouci).
    let chunk = step(0.62, h11(floor(in.uv.y * 9.0) * 3.7 + floor(in.uv.x * 5.0) * 1.3 + tick)) * bug;
    let flicker = 1.0 - bug * 0.3 * step(0.55, h11(tick * 1.9));
    let scan = 0.82 + 0.18 * sin(in.uv.y * 260.0 - t * 7.0);
    // Dégradé bas → haut : le danseur va du menthe au rose, le rideau de la couleur du portique à la glace.
    var low = panel.mint.rgb;
    var high = panel.rose.rgb;
    var amount = gain;
    if kind == KIND_CURTAIN {
        low = in.color;
        high = panel.ice.rgb;
        amount = 1.0;
    }
    let base = mix(low, high, in.uv.y);
    var col = base * g + vec3<f32>(r - g, 0.0, b - g) * 0.7 * (0.3 + bug);
    col = max(col, vec3<f32>(0.0)) * scan * flicker * (1.0 - chunk * 0.9) * amount;
    col += vec3<f32>(1.0) * bug * 0.05 * step(0.8, h11(tick * 2.3 + band));
    let fade = 1.0 - smoothstep(110.0, 400.0, dist);
    return col * fade;
}

// --- Aurore (additif) ------------------------------------------------------------------------------------------------

fn aurora(in: VertexOut) -> vec3<f32> {
    let t = panel.frame.x;
    let amount = in.color.r;
    let pulse = in.color.g;
    // La base du voile ondule ; des rayons verticaux de largeurs variées se balancent le long.
    let wave = n1(in.uv.x * 7.0 + t * 0.12) * 0.22 + n1(in.uv.x * 19.0 - t * 0.2) * 0.07;
    let y = in.uv.y - wave;
    let rays = n1(in.uv.x * 60.0 + t * 0.35) * 0.65 + n1(in.uv.x * 150.0 - t * 0.6) * 0.35;
    let body = smoothstep(0.0, 0.16, y) * pow(1.0 - clamp(y, 0.0, 1.0), 1.6);
    let a = body * (0.25 + rays * 0.95) * amount;
    let edge = smoothstep(0.0, 0.18, in.uv.x) * sstep(1.0, 0.82, in.uv.x);
    var col = mix(panel.lime.rgb, panel.mint.rgb, smoothstep(0.0, 0.35, y));
    col = mix(col, panel.violet.rgb, smoothstep(0.3, 0.9, y));
    return col * a * edge * (0.9 + pulse * 0.5);
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let kind = i32(floor(in.kind + 0.0001));
    let param = fract(in.kind);
    let dist = distance(in.world, view.world_position);

    // Lumière additive.
    if kind == KIND_RAILS {
        return vec4<f32>(rails(in, dist), 0.0);
    }
    if kind == KIND_CURTAIN || kind == KIND_DANCER {
        return vec4<f32>(holo(in, kind, dist), 0.0);
    }
    if kind > KIND_DANCER {
        return vec4<f32>(aurora(in), 0.0);
    }

    // Plein : un corps opaque, qui se fond dans la nappe de brouillard sous la route et dans la brume au loin.
    if kind == KIND_POST {
        return vec4<f32>(in.color, 1.0);
    }
    var col = vec3<f32>(0.0);
    var fog_near = 90.0;
    var fog_far = 380.0;
    if kind == KIND_BODY {
        col = body(in, param, dist);
    } else if kind == KIND_SLAB_TOP {
        col = slab_top(in);
        fog_near = 120.0;
        fog_far = 440.0;
    } else {
        col = slab_side(in);
        fog_near = 120.0;
        fog_far = 440.0;
    }
    let low = sstep(-4.0, MIST_Y + 2.0, in.world.y);
    col = mix(col, panel.mist.rgb, low);
    let fog = smoothstep(fog_near, fog_far, dist);
    return vec4<f32>(mix(col, panel.ink.rgb, fog), 1.0);
}
