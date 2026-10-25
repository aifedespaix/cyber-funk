// Route de Neon City : asphalte noir, liserés lumineux, tirets, joints, flaques et gouttes.
//
// Portage du fragment shader de `engine/road.ts` de Sillage (GLSL → WGSL/WESL), motif par motif, mêmes constantes.
//
// Repère : le maillage est un plan que le monde déplace sous la voiture ; les motifs (bords, tirets, joints) sont
// calculés sur la position monde RENDUE (`in.world_position`), c'est-à-dire relative à l'origine flottante de
// `cf-render` (multiple de 28 800 m, voir `origin.rs`). C'est l'équivalent de `vRel` de Sillage (« position du maillage
// moins l'origine des motifs ») : la coordonnée reste inférieure à ~15 km, donc précise au millimètre, et les périodes
// des motifs (9, 24, 100, 8, 5 m) divisent toutes le pas d'origine : aucun à-coup au recentrage.
//
// Teintes continues : dans Sillage, `prism(p.y * 0.035 + …)` et `prism(p.y * 0.05 - …)` lisent la coordonnée relative à
// l'origine, mais 28 800 m ne sont pas un multiple de leur période : la teinte des liserés et des joints sautait à chaque
// recentrage (toutes les ~14 min). `phase` réinjecte, en mètres et calculé en `f64` sur le processeur, la position de
// l'origine modulo la période commune des deux sinus de `prism` (`sin(t)` et `sin(1,7 t + 2,1)` reviennent ensemble tous
// les 2π × 10 / fréquence mètres) : la teinte est celle de la coordonnée absolue, sans saut. Près du départ (origine 0,
// `phase` nulle) le rendu est celui de Sillage au bit près.
//
// Le champ de la ville (`field.wesl`, ajouté au bout de ce fichier, J7-08) : reflet des lumières des immeubles-vitrines sur
// le sol mouillé (`field_sheen`), lueur diffuse (`field_light`), zone de glitch et décalque « ERROR ». Sans immeuble-vitrine
// tout vaut zéro, comme dans Sillage (lumières à y = -999, zone et décalque à force nulle).

import bevy_pbr::render::forward_io::VertexOutput;
import bevy_pbr::render::mesh_view_bindings::view;

// L'atlas du mot « ERROR » que l'immeuble du même nom projette sur la chaussée (cuit à l'ouverture, J7-08).
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var decal_map: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var decal_sampler: sampler;

// Même ordre de champs que `RoadParams` (Rust, `ShaderType`). Tout en vec4 : aucune surprise d'alignement.
struct Road {
    // Palette de la scène (lumière linéaire, w inutilisé).
    ink: vec4<f32>,
    mint: vec4<f32>,
    azure: vec4<f32>,
    violet: vec4<f32>,
    rose: vec4<f32>,
    ice: vec4<f32>,
    // Lueur sous la voiture (`uGlow`) et teinte d'ambiance de la section (`uMood`).
    glow: vec4<f32>,
    mood: vec4<f32>,
    // xyz : voiture en repère rendu (`uCar`).
    car: vec4<f32>,
    // x : temps (s), y : pouls (`uPulse`), z : basses (`uEnergy`), w : freinage (`uBrake`).
    frame: vec4<f32>,
    // x : part dessinée (`uDraw`, 1 = tout), y : humidité (`uWet`), z : pluie (`uRain`).
    look: vec4<f32>,
    // Position de l'origine flottante (m) modulo la période de la teinte : x pour les liserés (2π × 10 / 0,035 m), y pour
    // les joints (2π × 10 / 0,05 m). Voir plus haut, « teintes continues ».
    phase: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> road: Road;

// `smoothstep(edge0, edge1, x)` de GLSL accepte edge0 > edge1 (Sillage s'en sert pour des liserés : `smoothstep(0.16,
// 0.0, d)`) ; WGSL l'interdit. 1 - smoothstep(e1, e0, x) donne exactement la même courbe.
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

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

// `NOISE_GLSL` de Sillage (`landmarks/glsl.ts`).
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

fn prism(t: f32) -> vec3<f32> {
    let a = 0.5 + 0.5 * sin(t);
    let b = 0.5 + 0.5 * sin(t * 1.7 + 2.1);
    return mix(mix(road.mint.rgb, road.azure.rgb, a), mix(road.violet.rgb, road.rose.rgb, b), 0.5);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let world = in.world_position.xyz;
    let p = world.xz; // x = largeur, y = profondeur (z monde)
    let time = road.frame.x;
    let pulse = road.frame.y;
    let energy = road.frame.z;
    let brake = road.frame.w;
    let draw = road.look.x;
    let wet = road.look.y;
    let rain = road.look.z;
    let car = road.car.xyz;
    let ink = road.ink.rgb;
    let ice = road.ice.rgb;
    let mood = road.mood.rgb;
    let glow_color = road.glow.rgb;
    let view_distance = length(view.world_position - world);

    // Asphalte presque noir, grain fin
    let grain = hash(floor(p * vec2<f32>(9.0, 4.0)));
    var col = ink * (0.82 + 0.3 * grain);

    // Pluie : des flaques irrégulières. Mouillé, l'asphalte s'assombrit et reflète ; sec, il reste mat.
    let pud = smoothstep(0.34, 0.66, vnoise(p * vec2<f32>(0.35, 0.12) + 7.0) * 0.6 + vnoise(p * vec2<f32>(1.3, 0.5)) * 0.4);
    let wet_k = wet * (0.55 + 0.45 * pud);
    col *= 1.0 - 0.35 * wet_k;

    // Bords : liserés lumineux qui changent de teinte le long de la route
    // L'ambiance du morceau (calme, groove, sommet) teinte les liserés.
    let edge_color = mix(prism((p.y + road.phase.x) * 0.035 + time * 0.35), mood, 0.55);
    let edge = sstep(0.16, 0.0, abs(abs(p.x) - 8.4));
    col += edge_color * (edge * (1.1 + pulse * 1.2)
        + wet_k * sstep(1.2, 0.0, abs(abs(p.x) - 8.4)) * (0.3 + 0.14 * pulse) * (0.6 + 0.4 * vnoise(vec2<f32>(p.y * 0.5, p.x * 2.0))));

    // Tirets centraux
    let dash_d = abs(gl_mod(p.y + 1000.0, 9.0) - 4.5) - 1.9;
    let dash = sstep(0.1, 0.0, dash_d) * sstep(0.1, 0.0, abs(p.x) - 0.18);
    col += road.azure.rgb * (dash * (0.9 + pulse * 1.4) + wet_k * sstep(1.6, 0.0, dash_d) * sstep(0.9, 0.0, abs(p.x)) * 0.35);

    // Joints transversaux : une vague de lumière à chaque kick
    let seam_d = abs(gl_mod(p.y + 1000.0, 24.0) - 12.0);
    let seam = sstep(0.2, 0.0, seam_d) * sstep(8.6, 7.4, abs(p.x));
    col += prism((p.y + road.phase.y) * 0.05 - time) * seam * (0.2 + pulse * 0.9 + energy * 0.2);

    // Lueur colorée sous la voiture
    let d = (p - car.xz) / vec2<f32>(2.6, 4.2);
    let glow = exp(-dot(d, d));
    col += mix(road.mint.rgb, glow_color, 0.6) * glow * (0.1 + pulse * 0.2 + energy * 0.1) + glow_color * exp(-dot(d, d) * 1.6) * 0.22;

    // Sol mouillé : les lumières des immeubles-vitrines y laissent un reflet et une lueur.
    col += field_sheen(world, view.world_position) * (0.38 + wet_k * 2.4)
        + ice * 0.06 * field_light(world, vec3<f32>(0.0, 1.0, 0.0)) * (1.0 + wet_k * 3.0);

    // Chaussée mouillée : les optiques de la voiture se reflètent dans le sol (phares devant, feux rouges derrière).
    let hq = p - car.xz;
    var ahead = 0.0;
    var behind = 0.0;
    for (var s = 0; s < 2; s++) {
        let sx = select(0.75, -0.75, s == 0);
        let da = (hq - vec2<f32>(sx, -9.0)) / vec2<f32>(0.8, 10.0);
        ahead += exp(-dot(da, da));
        let r = (hq - vec2<f32>(sx * 1.05, 2.8)) / vec2<f32>(0.5, 2.4);
        behind += exp(-dot(r, r));
    }
    col += mix(ice, glow_color, 0.25) * ahead * wet_k * 0.8;
    col += road.rose.rgb * behind * wet_k * (0.5 + brake * 2.0);

    // Gouttes qui tombent : des ronds qui s'élargissent et s'éteignent, plus nets dans les flaques.
    if rain > 0.01 {
        var rip = 0.0;
        for (var l = 0; l < 2; l++) {
            let sc = select(2.5, 1.6, l == 0);
            let q = p / sc + f32(l) * 13.7;
            let id = floor(q);
            let f = fract(q) - 0.5;
            let h = hash(id);
            let ph = fract(time * (0.6 + 0.5 * hash(id + 3.1)) + h);
            let c = (vec2<f32>(hash(id + 1.7), hash(id + 9.2)) - 0.5) * 0.5;
            let dd = length(f - c) * sc;
            rip += sstep(0.08, 0.0, abs(dd - ph * 0.6)) * (1.0 - ph) * step(h, 0.4 + rain * 0.5);
        }
        let near = 1.0 - smoothstep(30.0, 130.0, view_distance);
        let lit = mix(ice, mood, 0.4) * 0.45 + field_light(world, vec3<f32>(0.0, 1.0, 0.0)) * 1.4;
        col += lit * rip * rain * (0.35 + 0.65 * pud) * near;
    }

    // Zone de glitch : scanlines qui défilent, bandes aux canaux inversés.
    let zw = field_zone(world);
    let road_band = step(0.9, fract(world.z * 0.07 + time * 0.9));
    let road_scan = step(0.5, fract(world.z * 0.9 - time * 4.0));
    col = mix(col, col.brg * 1.4, zw * road_band);
    col += field.zone_tint.rgb * zw * road_scan * 0.3;

    // Décalque : le mot du panneau ERROR, projeté sur la route devant la voiture.
    if field.decal.x > 0.001 {
        let q = (world.xz - field.decal_box.xy) / field.decal_box.zw;
        if abs(q.x) < 1.0 && abs(q.y) < 1.0 {
            // Le haut du mot pointe vers l'avant (-z), pour qu'il se lise depuis le siège.
            let m = glyph_word(decal_map, decal_sampler, vec2<f32>(q.x * 0.5 + 0.5, 0.5 - q.y * 0.5), field.decal_word, field.decal.y, time);
            col += (field.zone_tint.rgb * (m.g * 1.6 + m.r) + ice * m.b * field.decal.y) * field.decal.x;
        }
    }

    // Pré-intro : les lignes néon se dessinent depuis la voiture vers l'horizon, avec un front lumineux.
    if draw < 0.999 {
        let reach = draw * 1100.0;
        let ahead_z = car.z - world.z;
        let drawn = 1.0 - smoothstep(reach - 40.0, reach, ahead_z);
        // `pow(x, 2.0)` de GLSL, écrit en produit : `pow` d'une base négative est indéfini en WGSL.
        let u = (ahead_z - reach + 6.0) / 5.0;
        let front = exp(-u * u) * 0.9 * step(ahead_z, reach + 1.0) * smoothstep(0.0, 0.05, draw);
        col = mix(ink * 0.8, col, drawn) + road.mint.rgb * front;
    }

    // Brouillard vers l'encre : la route se perd à l'horizon
    let fog = smoothstep(90.0 - 30.0 * rain, 340.0 - 90.0 * rain, view_distance);
    col = mix(col, ink, fog);

    return vec4<f32>(col, 1.0);
}
