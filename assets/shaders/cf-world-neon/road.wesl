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

// Sol mouillé (cyber-funk, pas dans Sillage) : la chaussée réfléchit les lumières comme dans la vraie vie.
//
//  - `Lamps` : jusqu'à 24 lampes ponctuelles que le processeur choisit chaque image (feux et néons de la voiture, puis les
//    points lumineux les plus forts près d'elle : gyrophares, drones, feux de position), en repère rendu ;
//  - le reflet d'une lampe est calculé par le rayon réfléchi : la normale du sol est perturbée par le relief de
//    l'asphalte (étiré dans le sens de la marche, d'où les traînées verticales sous les lumières), les flaques (lisses) et les
//    anneaux des gouttes ; la lampe s'allume là où ce rayon passe près d'elle ;
//  - les fenêtres des immeubles se reflètent de la même façon : le rayon réfléchi est lancé contre les plans des façades, et
//    on y lit la grille de fenêtres (`facade_reflection`) ;
//  - la pluie dessine des anneaux qui s'élargissent, des éclaboussures, et déforme les reflets (`rain_ripples`).

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

// Les lampes que reflète le sol (même disposition que `RoadLamps`, Rust). Une couleur nulle : lampe éteinte.
struct Lamps {
    // xyz : position en repère rendu, w : rayon d'éclat en mètres.
    pos: array<vec4<f32>, 24>,
    // rgb : couleur déjà multipliée par l'éclat (lumière linéaire).
    color: array<vec4<f32>, 24>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(4) var<uniform> lamps: Lamps;

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

// Reflet des lampes dans un sol dont le rayon réfléchi est `r` en `p`. `rough` : étalement angulaire du reflet (rad) ; un
// sol lisse concentre la lampe, un sol rugueux l'étire en traînée.
fn lamp_reflection(p: vec3<f32>, r: vec3<f32>, rough: f32) -> vec3<f32> {
    var acc = vec3<f32>(0.0);
    for (var i = 0; i < 24; i++) {
        let c = lamps.color[i].rgb;
        if c.r + c.g + c.b < 0.001 {
            continue;
        }
        let size = lamps.pos[i].w;
        let to = lamps.pos[i].xyz - p;
        let along = dot(to, r);
        if along < 0.5 {
            continue;
        }
        // Distance de la lampe au rayon, et largeur du reflet à cette distance.
        let d2 = max(dot(to, to) - along * along, 0.0);
        let s = size + rough * along;
        let k = (size * size) / (d2 + s * s);
        // Coupure douce loin de la lampe : le reflet ne s'étend pas à l'infini.
        let cut = 1.0 - smoothstep(0.0, 1.0, d2 / (36.0 * s * s));
        acc += c * min(k, 1.0) * cut;
    }
    return acc;
}

// Reflet des fenêtres des deux rangées d'immeubles (façades à 15,5 m de l'axe) : le rayon réfléchi `r` est lancé contre
// chaque plan, et la grille de fenêtres (travées de 1,3 m, étages de 2,4 m, une sur deux allumée, comme sur les façades) y est
// lue au point touché. Les couleurs viennent de la palette.
fn facade_reflection(p: vec3<f32>, r: vec3<f32>) -> vec3<f32> {
    var acc = vec3<f32>(0.0);
    for (var s = 0; s < 2; s++) {
        let side = select(1.0, -1.0, s == 0);
        if r.x * side < 0.08 {
            continue;
        }
        let t = (side * 15.5 - p.x) / r.x;
        if t < 1.0 || t > 260.0 {
            continue;
        }
        let h = p + r * t;
        if h.y < 0.0 || h.y > 80.0 {
            continue;
        }
        let cell = vec2<f32>(h.z / 1.3, h.y / 2.4);
        let id = floor(cell);
        let f = fract(cell);
        let lit = step(h21(id + vec2<f32>(side * 9.0, 3.0)), 0.5);
        let pane = smoothstep(0.1, 0.22, f.x) * (1.0 - smoothstep(0.78, 0.9, f.x))
            * smoothstep(0.15, 0.3, f.y) * (1.0 - smoothstep(0.7, 0.85, f.y));
        let tone = h21(id + 57.0);
        let warm = mix(road.violet.rgb, road.rose.rgb, clamp((tone - 0.5) * 2.0, 0.0, 1.0));
        let cool = mix(road.mint.rgb, road.azure.rgb, clamp(tone * 2.0, 0.0, 1.0));
        let colour = select(cool, warm, tone > 0.5);
        let bright = 0.55 + 0.45 * step(0.8, h21(id + 11.0));
        acc += colour * lit * pane * bright * (1.0 - smoothstep(120.0, 260.0, t));
    }
    return acc;
}

// Les gouttes qui tombent sur le sol : trois tailles de cellules, une goutte par cellule et par cycle (0,7 à 1,5 Hz), dont
// l'impact lance un anneau qui s'élargit en s'éteignant. Rend la pente de la surface (xy), l'éclat des anneaux (z) et
// l'éclaboussure de l'impact (w).
fn rain_ripples(p: vec2<f32>, time: f32, rain: f32) -> vec4<f32> {
    var out = vec4<f32>(0.0);
    for (var l = 0; l < 3; l++) {
        let sc = select(select(1.2, 1.9, l == 1), 3.0, l == 0);
        let q = p / sc + f32(l) * 13.7;
        let id = floor(q);
        let h = hash(id + f32(l));
        let on = step(h, 0.2 + rain * 0.6);
        let rate = 0.7 + 0.8 * hash(id + 3.1);
        let ph = fract(time * rate + h * 7.0);
        let c = (vec2<f32>(hash(id + 1.7), hash(id + 9.2)) - 0.5) * 0.5;
        let v = (fract(q) - 0.5 - c) * sc;
        let d = length(v);
        let radius = ph * 0.34 * sc;
        let w = 0.04 + 0.05 * ph;
        let env = (1.0 - ph) * (1.0 - ph);
        let e = (d - radius) / w;
        let wave = exp(-e * e) * env * on;
        // Pente : la dérivée du profil (gaussienne) selon le rayon, dirigée selon le rayon.
        let dir = v / max(d, 0.001);
        out = vec4<f32>(out.xy + dir * (-2.0 * e / w) * wave * 0.012, out.z + wave, out.w);
        // Éclaboussure : un éclat bref à l'impact.
        let splash = exp(-d * d / 0.012) * pow(1.0 - ph, 6.0) * on;
        out.w += splash;
    }
    return out;
}

// Flaque de lumière sous le sujet : rayon (m) et gain.
const ROAD_POOL_SIGMA_M: f32 = 6.0;
const ROAD_POOL_GAIN: f32 = 0.12;
// Ondes de kick sur l'eau : vitesse du front (m/s), épaisseur (m), durée de vie (s), pente maximale.
const ROAD_KICK_SPEED: f32 = 14.0;
const ROAD_KICK_WIDTH: f32 = 0.9;
const ROAD_KICK_LIFE: f32 = 0.7;
const ROAD_KICK_SLOPE: f32 = 0.05;

// Pente ajoutée à la surface de l'eau par les quatre derniers kicks (âges dans `scene.kick_age`, source `scene.source`).
// La pente est la dérivée radiale d'une gaussienne : elle s'annule sur la crête et s'éteint avec l'âge.
fn kick_ripples(p: vec2<f32>) -> vec2<f32> {
    let to = p - scene.source.xz;
    let r = max(length(to), 0.001);
    var slope = 0.0;
    for (var i = 0; i < 4; i++) {
        let age = scene.kick_age[i];
        let x = (r - ROAD_KICK_SPEED * age) / ROAD_KICK_WIDTH;
        slope += -x * exp(-x * x) * exp(-age / ROAD_KICK_LIFE);
    }
    return (to / r) * slope * ROAD_KICK_SLOPE * scene.shell.w * 2.0;
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

    // Asphalte presque noir, grain fin. Avec l'éclairage de scène (C4), le grain se fond dans sa moyenne dès qu'une
    // cellule passe sous deux pixels : aucun scintillement au loin (DA §2.2). Bloc à zéro : le grain d'avant.
    let grain_px = max(fwidth(p.x * 9.0), fwidth(p.y * 4.0));
    let grain_raw = hash(floor(p * vec2<f32>(9.0, 4.0)));
    let grain = mix(grain_raw, mix(grain_raw, 0.5, smoothstep(0.25, 0.6, grain_px)), scene.fog.w);
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

    // Flaque de lumière sous le sujet (C4, DA §1.2 règle 6) : `c·exp(−r²/σ²)`, σ = 6 m, dans la teinte de la lumière
    // principale. Elle suit le sujet et lui garde le contraste local le plus fort. Bloc à zéro : rien.
    let pool_r = (p - scene.source.xz) / ROAD_POOL_SIGMA_M;
    col += scene.c_key.rgb * ROAD_POOL_GAIN * exp(-dot(pool_r, pool_r)) * scene.fog.w;

    // Sol mouillé : les lumières des immeubles-vitrines y laissent un reflet et une lueur.
    col += field_sheen(world, view.world_position) * (0.38 + wet_k * 2.4)
        + ice * 0.06 * field_light(world, vec3<f32>(0.0, 1.0, 0.0)) * (1.0 + wet_k * 3.0);

    // Lueur diffuse des optiques sur l'asphalte mouillé (phares devant, feux rouges derrière) : le halo mat autour de la voiture.
    // Les reflets nets viennent des lampes, plus bas.
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
    col += mix(ice, glow_color, 0.25) * ahead * wet_k * 0.4;
    col += road.rose.rgb * behind * wet_k * (0.25 + brake * 1.0);

    // Sol mouillé : reflets des lumières et des fenêtres, brisés par la pluie.
    let water = max(wet, rain);
    if water > 0.02 {
        let pudk = smoothstep(0.25, 0.75, pud);
        // Les flaques sont lisses (reflet net), le reste de l'asphalte rugueux (reflet étiré en traînée).
        let refl = water * (0.4 + 0.6 * pudk);
        let near_k = 1.0 - smoothstep(30.0, 110.0, view_distance);
        var rip = vec4<f32>(0.0);
        if rain > 0.01 {
            rip = rain_ripples(p, time, rain);
        }
        // Relief de l'asphalte, étiré dans le sens de la marche : les reflets des lumières s'étirent en traînées verticales.
        let sigma = mix(0.14, 0.015, pudk) * (1.0 + 1.5 * rain);
        let n1 = vnoise(p * vec2<f32>(2.1, 0.55) + 3.0) - 0.5;
        let n2 = vnoise(p * vec2<f32>(0.9, 0.22) + 11.0) - 0.5;
        let n3 = vnoise(p * vec2<f32>(7.0, 2.6) + 29.0 + vec2<f32>(0.0, time * 0.4)) - 0.5;
        // Ondes de kick (C4) : chaque kick pousse un anneau sur l'eau depuis le sujet. Bloc à zéro : aucune onde.
        let slope = vec2<f32>((n1 + 0.5 * n3) * 2.0, (n2 + 0.5 * n3) * 5.0) * sigma
            + (rip.xy + kick_ripples(p) * scene.fog.w) * near_k;
        let normal = normalize(vec3<f32>(slope.x, 1.0, slope.y));
        let incident = normalize(world - view.world_position);
        let reflected = reflect(incident, normal);
        // Fresnel : presque un miroir sous une incidence rasante, presque rien en regardant droit dessous.
        let facing = clamp(dot(normal, -incident), 0.0, 1.0);
        let fresnel = 0.04 + 0.96 * pow(1.0 - facing, 5.0);
        let rough = mix(0.09, 0.006, pudk);
        let seen = lamp_reflection(world, reflected, rough) + facade_reflection(world, reflected) * 0.8;
        col += min(seen, vec3<f32>(3.0)) * fresnel * refl * 1.3;

        // Les gouttes : anneaux qui s'élargissent (éclairés par la lumière ambiante et celle des immeubles-vitrines) et
        // éclaboussures blanches à l'impact. Plus de gouttes au loin que l'œil ne distingue : on les fond dans la rugosité.
        let lit = mix(ice, mood, 0.4) * 0.6 + field_light(world, vec3<f32>(0.0, 1.0, 0.0)) * 1.4;
        col += lit * rip.z * rain * (0.5 + 0.5 * pudk) * near_k * 1.6;
        col += ice * rip.w * rain * near_k * 1.2;
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
    // Le fondu efface le détail au loin ; la perspective atmosphérique (C3, `atmosphere.wesl`) s'y ajoute.
    col = atmo_fog(mix(col, ink, fog), world, view.world_position);

    return vec4<f32>(col, 1.0);
}
