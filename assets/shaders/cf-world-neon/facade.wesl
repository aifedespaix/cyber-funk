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
//   bits 24..26  tempérament de l'immeuble : indice dans `facade.tempers` (voir `temperament.rs`, Rust)
//   bits 27..31  motif de graffiti mural, 0 : aucun (`facade_paint.wesl`, ajouté au bout de ce fichier)
// Le décalage vertical du bloc (`aInfo.w`) est l'altitude de l'instance (sa base) : un corps commence à −BASE_DEPTH, une
// couronne à la hauteur du corps, ce qui prolonge ses étages.
//
// Le champ de la ville (`field.wesl`, ajouté au bout de ce fichier, J7-08) : `field_light` répand la lueur des
// immeubles-vitrines, `field_zone` porte leur glitch (bandes aux canaux inversés, scanlines). Sans immeuble-vitrine ils
// valent 0 et 0, comme dans Sillage. La panne de courant (`power`) est celle de J7-05.
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
    // x : humidité (`uWet`), y : pluie (`uRain`), z : pouls des fenêtres (`uWinPulse`), w : chute des tours (0..1, arrêt du show).
    weather: vec4<f32>,
    // Front d'une panne de courant (`uPower`) : x position en z du front, y demi-largeur de son fondu, z part des fenêtres
    // éteintes au-delà du front, w sens (−1 : éteint du côté des z négatifs).
    power: vec4<f32>,
    // Texture du son hors kick : x transitoire large bande (caisse claire, clap), y charleston, z brillance, w bruit.
    texture: vec4<f32>,
    // Table des tempéraments, deux vec4 par entrée. [2i] : x étendue des teintes (degrés), y biais d'allumage, z part ambre,
    // w part glace. [2i+1] : x forme, y mouvement, z taux d'étincelles (formes et mouvements : rang des énumérations Rust).
    tempers: array<vec4<f32>, 16>,
    // Mouvements de lumière (`facade_motion.wesl`) : x voie rapide déjà limitée (0..1), y part de mouvement rapide
    // autorisée (0..1), z part de vie des fenêtres (0..1, curseur opérateur, 0 en qualité basse), w 1 en qualité basse (écrans
    // qui défilent coupés).
    motion: vec4<f32>,
    // Traîne de la voiture (`facade_trail.wesl`) : 16 points, xyz en repère rendu, w intensité restante (0 : vide).
    trail: array<vec4<f32>, 16>,
    // Couleur de la voiture (xyz) et force de la traîne (w).
    car_tint: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> facade: Facade;

// Les graffitis muraux (`facade_paint.wesl`) : l'atlas en tableau de couches, et où lire chaque motif. Même ordre de champs
// que `WallCells` (Rust). Le motif k du tag est l'entrée k (l'entrée 0 n'est jamais lue).
struct Wall {
    // x : largeur d'une couche (px), y : hauteur, z : nombre de couches, w : distance où le graffiti a disparu (m ; la qualité la raccourcit).
    atlas: vec4<f32>,
    // Par motif : x, y, largeur, hauteur du rectangle utile (px).
    rects: array<vec4<f32>, 32>,
    // Par motif : x couche, y part de la hauteur occupée par les lettres, w 1 si le motif existe.
    info: array<vec4<f32>, 32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(2) var wall_map: texture_2d_array<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var wall_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(4) var<uniform> wall: Wall;

// Hauteur d'un étage et largeur d'une travée de fenêtres, en mètres.
const FLOOR_HEIGHT: f32 = 2.4;
const BAY_WIDTH: f32 = 1.3;
// Altitude de la nappe de brouillard sous la route, en mètres.
const MIST_Y: f32 = -50.0;
// Albédo du béton (DA §2.2) : la part des lumières de scène que le corps d'une tour renvoie.
const FACADE_ALBEDO: f32 = 0.18;
// Fenêtres hiérarchisées (C4) : part des étages allumés au calme, ce que `Δ` y ajoute, et chance d'une fenêtre d'être
// allumée dans un étage allumé ou éteint. Miroir Rust : `facade/windows.rs`.
const WINDOW_SHARE_CALM: f32 = 0.18;
const WINDOW_SHARE_DELTA: f32 = 0.42;
const WINDOW_ON_FLOOR: f32 = 0.9;
const WINDOW_OFF_FLOOR: f32 = 0.03;
// Intensité log-normale d'une fenêtre : palier E1 et écart-type du logarithme.
const WINDOW_E1: f32 = 0.8;
const WINDOW_SIGMA: f32 = 0.6;

// Part des étages allumés : 0,18 + 0,42·Δ, décalée par le tempérament de l'immeuble.
fn window_share(delta: f32, bias: f32) -> f32 {
    return clamp(WINDOW_SHARE_CALM + WINDOW_SHARE_DELTA * clamp(delta, 0.0, 1.0) + bias, 0.02, 0.95);
}

// Gain log-normal d'une fenêtre, `n` gaussien (somme de trois hachages, bornée à ±3).
fn window_gain(n: f32) -> f32 {
    return WINDOW_E1 * exp(WINDOW_SIGMA * clamp(n, -3.0, 3.0));
}

// Plafond d'une fenêtre : aucun canal ne dépasse `cap` (0,9·θ_b), la teinte est gardée.
fn window_cap(c: vec3<f32>, cap: f32) -> vec3<f32> {
    let top = max(c.r, max(c.g, c.b));
    return c * min(1.0, cap / max(top, 0.0001));
}

// `smoothstep(edge0, edge1, x)` de GLSL accepte edge0 > edge1 ; WGSL l'interdit. 1 - smoothstep(e1, e0, x) donne la même
// courbe.
fn sstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge0 < edge1 {
        return smoothstep(edge0, edge1, x);
    }
    return 1.0 - smoothstep(edge1, edge0, x);
}

// Décale la teinte d'une couleur de `deg` degrés par une rotation autour de l'axe des gris (1,1,1) : la somme des canaux
// est conservée (pas la luminance), et 0° rend la couleur à l'identique (cos = 1, sin = 0). Une couleur saturée peut
// sortir de [0,1] d'un canal : on borne à 0 par le bas.
fn hue_shifted(color: vec3<f32>, deg: f32) -> vec3<f32> {
    let a = radians(deg);
    let k = vec3<f32>(0.57735027);
    return max(color * cos(a) + cross(k, color) * sin(a) + k * dot(k, color) * (1.0 - cos(a)), vec3<f32>(0.0));
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

// Facteur de vue lointaine : 1 au ras du sol, jusqu'à 2,6 quand la caméra est haut (plans larges, tour de planète). La
// brume et la pousse des tours reculent d'autant : la ville reste visible à l'horizon. Même formule dans
// `facade_prepass.wesl` et `landmark_common.wesl`, et `VISTA_MAX` côté Rust (portée de culling).
fn vista() -> f32 {
    return 1.0 + 1.6 * smoothstep(35.0, 140.0, view.world_position.y);
}

// Les tours « poussent » du sol entre 300 et 440 m : aucune apparition brutale (`cityGrow`). Même formule dans
// `facade_prepass.wesl`.
fn grow_at(world: vec3<f32>) -> f32 {
    let v = vista();
    return 1.0 - smoothstep(300.0 * v, 440.0 * v, distance(world.xz, view.world_position.xz));
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
    // Indice du tempérament de l'immeuble (bits 24..26 du tag).
    @location(7) @interpolate(flat) temper: u32,
    // Motif de graffiti mural (bits 27..31 du tag), 0 : aucun.
    @location(8) @interpolate(flat) graffiti: u32,
    // Décalage propre à la tour (0..1) : les tours apparaissent en cascade, pas en mur.
    @location(9) @interpolate(flat) cascade: f32,
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
    // Arrêt du show : la tour rétrécit vers sa base enfouie (`facade_collapse.wesl`), glitch coupé en qualité basse.
    wp = collapse_apply(wp, m[3].xz, facade.weather.w, facade.frame.x, facade.motion.w < 0.5 && facade.motion.y > 0.0);
    // Une tour sur quatre respire d'un pour cent avec le kick : assez pour sentir le poids du beat, trop peu pour faire dessin
    // animé. Le passage de profondeur ignore ce gonflement (il n'écarte que la pointe, jamais un voisin).
    let breath_seed = f32(tag & 0x3FFu);
    wp.y *= 1.0 + 0.012 * facade.frame.y * step(h21(vec2<f32>(breath_seed, 7.0)), 0.25);
    out.world = wp.xyz;
    let spot_floor = (tag >> 16u) & 0xFFu;
    let spot_y = select(-1.0, (f32(spot_floor) - 0.5) * FLOOR_HEIGHT, spot_floor > 0u);
    out.info = vec4<f32>(f32(tag & 0x3FFu), spot_y, f32((tag >> 14u) & 3u), m[3].y);
    out.tint = facade.cycle[(tag >> 10u) & 0xFu].rgb;
    out.temper = (tag >> 24u) & 7u;
    out.graffiti = tag >> 27u;
    out.cascade = tower_offset(m[3].xz);
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
    let onset = facade.texture.x;
    let hat = facade.texture.y;
    let bright = facade.texture.z;
    let noisy = facade.texture.w;
    let ink = facade.ink.rgb;
    let ice = facade.ice.rgb;
    let amber = facade.amber.rgb;
    let mist = facade.mist.rgb;
    let scan = facade.scan.rgb;
    let v_c = in.tint;
    let temper_a = facade.tempers[in.temper * 2u];
    let temper_b = facade.tempers[in.temper * 2u + 1u];
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
        // Le décalage de la tour étale l'apparition (le terme en build le compense : à build = 1 tout est en place).
        build_top = mix(-70.0, 340.0, clamp(build * 1.75 - 0.45 * nearness - 0.3 * in.cascade, 0.0, 1.0));
        if in.world.y > build_top {
            discard;
        }
    }
    // Les lumières sont celles de la tour elle-même : une tour qui sort de terre n'allume ses fenêtres qu'une fois formée
    // (elles s'allument à quelques étages sous la ligne de balayage, jamais devant), et une tour qui se désynchronise
    // les éteint dès qu'elle commence à glitcher puis à s'enfoncer. Aucune lumière n'est donnée aux tours absentes.
    var awake = 1.0;
    if build < 0.999 {
        awake = 1.0 - smoothstep(build_top - 45.0, build_top - 12.0, in.world.y);
    }
    awake *= 1.0 - smoothstep(0.0, 0.35, collapse_t(facade.weather.w, in.cascade));
    // Entier arrondi : une graine interpolée varierait d'un pixel à l'autre et bruiterait le hachage.
    let seed = floor(in.info.x + 0.5);
    let tier = in.info.z;
    let n = normalize(in.normal_w);
    let haze = ink + facade.haze.rgb * 0.05;
    let far = tier > 2.5;
    let dist = length(in.world - cam);
    // Entièrement dans la brume : pas la peine de calculer fenêtres et ruissellement (les dérivées sont déjà prises).
    if dist > (380.0 - 100.0 * facade.weather.y) * vista() {
        // Avec l'atmosphère (C3), cette silhouette passe par la brume, qui est plus claire qu'elle (DA §1.2, règle 2).
        return vec4<f32>(atmo_fog(haze, in.world, cam), 1.0);
    }

    // Antenne : un trait lumineux dont la pointe bat avec le kick.
    if tier > 1.5 && !far {
        let tip = smoothstep(0.6, 1.0, in.local.y / in.size.y);
        let c = v_c * (0.5 + tip * (2.2 + pulse * 3.0)) * awake;
        let f = smoothstep(120.0 * vista(), 380.0 * vista(), dist);
        return vec4<f32>(atmo_fog(mix(c, haze, f), in.world, cam), 1.0);
    }

    let lambert = max(dot(n, normalize(vec3<f32>(-0.4, 0.8, 0.5))), 0.0);
    var col = ink * 0.9 + ice * (0.014 + 0.04 * lambert) + v_c * 0.012;
    // Pluie : la façade s'assombrit, devient brillante, et l'eau ruisselle en traînées verticales.
    col *= 1.0 - 0.25 * wet;
    // Relief sans calcul d'ombre (C4, DA §2.1), sur le bloc `SceneLight` : ombrage par face du côté de la rue dominant,
    // pied des tours plongé dans le noir, remplissage par la brume, liseré de contre-jour sur les arêtes. Bloc à zéro :
    // rien ne change.
    let lit_scene = scene.fog.w;
    if lit_scene > 0.0 {
        let shade = 0.45 + 0.55 * max(dot(n, scene.dir_key.xyz), 0.0);
        let ao = mix(0.35, 1.0, smoothstep(0.0, 0.25 * max(in.size.y + in.info.w, 1.0), h));
        let fill = scene.c_fill.rgb * (0.5 + 0.5 * n.y) * FACADE_ALBEDO;
        let to_eye = normalize(cam - in.world);
        let grazing = 1.0 - max(dot(n, to_eye), 0.0);
        let rim = grazing * grazing * grazing * grazing * max(dot(scene.dir_rim.xyz, -to_eye), 0.0);
        col = mix(col, (col * shade + fill) * ao + scene.c_rim.rgb * rim * FACADE_ALBEDO, lit_scene);
    }

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
        let lit_chance = 0.5 + high * 0.25 + noisy * 0.12 + 0.2 * h21(vec2<f32>(seed * 0.29, floor_id)) + temper_a.y;
        // Fenêtres hiérarchisées (C4, DA §1.1) : la part allumée suit la densité visuelle `Δ` (0,18 au calme, 0,6 au sommet),
        // et les fenêtres s'allument par étages entiers, pas en confettis. Bloc à zéro : la loi d'avant.
        let floor_on = step(h21(vec2<f32>(floor_id * 1.37 + 5.0, seed * 0.53 + face)), window_share(scene.c_fill.w, temper_a.y));
        let grouped = mix(WINDOW_OFF_FLOOR, WINDOW_ON_FLOOR, floor_on) + high * 0.05;
        var lit = step(window_rnd, mix(lit_chance, grouped, lit_scene)) * (1.0 - dead);
        if far {
            // Plages de lumière sur de larges blocs de façade : rien ne scintille quand les fenêtres deviennent sous-pixel.
            let zone = vec2<f32>(floor(uu / 7.0), floor(vv / 6.0));
            let zone_chance = mix(lit_chance, window_share(scene.c_fill.w, temper_a.y), lit_scene);
            lit = (0.2 + 0.8 * step(h21(zone + vec2<f32>(seed * 0.37 + face * 3.1, seed * 0.11)), zone_chance)) * (1.0 - dead);
        }
        // Quelques fenêtres clignotent en rythme avec les médiums.
        let blink = select(step(0.93, h21(cell + 9.7 + seed * 0.13)), 0.0, far);
        lit *= mix(1.0, step(0.5, fract(time * 1.5 + window_rnd * 6.0)) * (0.4 + mid), blink);

        // Vie propre : télé qui papillote, extinction lente, néon qui grésille, toujours aux mêmes fenêtres (rien au loin).
        if !far {
            lit *= window_life(cell, seed, time, temper_b.z * facade.motion.z, facade.motion.y);
        }

        // Charleston : une poignée de fenêtres crépite, différente à chaque tranche de 40 ms.
        lit = max(lit, hat * step(h21(cell + 5.1 + floor(time * 25.0) * 3.7), 0.07) * (1.0 - dead));

        // Pouls de la ville : sur chaque temps, un autre lot de fenêtres éteintes s'allume (événement « city-pulse »).
        lit = max(lit, win_pulse * step(h21(cell + 13.3 + floor(beat) * 1.7), 0.55) * (1.0 - dead));

        // Plus loin, les fenêtres se fondent en une teinte moyenne (pas de scintillement).
        let fade_out = select(smoothstep(0.12, 0.45, px), 1.0, far);
        // La forme de lumière du tempérament (fenêtres, bandeaux, colonnes, enseigne, écran, étage teinté) ; elle se
        // fond elle-même en la teinte moyenne par `fade_out` (voir `facade_shapes.wesl`).
        let mask = shape_mask(temper_b.x, cell, f, seed, fade_out);

        // Camaïeu : la teinte de la fenêtre est celle de la tour décalée dans ±étendue/2 selon son hachage (le tempérament 0
        // a une étendue nulle : la teinte de la tour, inchangée). Les parts d'ambre et de glace viennent de la table ; pour
        // le tempérament 0 (0,3 et 0,1), 1 - part retombe sur les seuils d'avant, 0,7 et 0,9.
        let pane_hue = hue_shifted(v_c, (h21(cell + 77.0) - 0.5) * temper_a.x);
        let warm = mix(pane_hue, amber, step(1.0 - temper_a.z, h21(cell + 31.0)));
        // La brillance du son refroidit les fenêtres : un filtre qui s'ouvre fait virer la ville vers la glace.
        let window_color = mix(mix(warm, ice, step(1.0 - temper_a.w, h21(cell + 57.0))), ice, bright * 0.35);

        // Vague de lumière qui monte d'un cran à chaque kick.
        let travel = fract(beat / 8.0 + seed * 0.0371) * 190.0;
        let wave_d = (h - travel) / 9.0;
        let wave = select(exp(-wave_d * wave_d), 0.0, far);
        let glow = 0.55 + wave * (1.0 + pulse * 0.9) + pulse * 0.2 + win_pulse * 0.6;
        // Panne de courant : part de lumière qui reste aux fenêtres derrière le front (1 partout sans panne).
        let power_edge = max(facade.power.y, 0.001);
        let behind = smoothstep(-power_edge, power_edge, (in.world.z - facade.power.x) * facade.power.w);
        let power_left = (1.0 - facade.power.z * behind) * awake;
        // Mouvement du tempérament (vague, dérive, balayage, pulsation) ; rien au loin : pas de scintillement sous-pixel.
        var motion = 1.0;
        if !far {
            motion = motion_gain(temper_b.y, h, u, seed, time, beat, facade.motion.x);
        }
        // Règle 7 : le gain de mouvement ne s'ajoute qu'à la base stable (0,55), jamais au pouls brut du kick contenu dans
        // `glow` ; sans mouvement (`motion` = 1) le terme est exactement nul et le rendu d'avant est inchangé.
        let pane = window_color * lit * mask * (glow + 0.55 * (motion - 1.0)) * (1.0 + 0.45 * wet) * power_left;
        // Intensité log-normale par fenêtre, plafonnée sous le seuil du bloom (C4, DA §1.1) : une fenêtre reste au palier
        // E1, quoi que fasse la musique. Bloc à zéro : la fenêtre d'avant.
        let pane_n = (h21(cell + 41.0) + h21(cell + 43.0) + h21(cell + 47.0) - 1.5) * 2.0;
        // Traîne de la voiture : la lumière des fenêtres déjà allumées s'attarde là où elle est passée (rien au loin).
        var trail_pane = vec3<f32>(0.0);
        if !far {
            trail_pane = trail_window_light(in.world, window_color) * lit * mask * (1.0 + 0.45 * wet) * power_left;
        }
        // Le plafond porte sur toute la lumière de la fenêtre, traîne comprise.
        col += mix(pane + trail_pane, window_cap(pane * window_gain(pane_n) + trail_pane, scene.c_key.w), lit_scene);

        // Caisse claire : sur environ un tiers des tours, une bande de lumière tombe du sommet vers le pied pendant que
        // l'impulsion retombe (le kick, lui, fait monter la vague : les deux se croisent).
        if !far {
            let reacts = step(h21(vec2<f32>(seed, 31.0)), 0.33);
            let fall = 1.0 - onset;
            let band_y = mix(in.size.y * 1.1, 0.0, fall * fall);
            let band_d = (in.local.y - band_y) / 2.2;
            col += mix(ice, v_c, 0.5) * exp(-band_d * band_d) * onset * reacts * 0.9;
        }

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

        // Graffiti mural : un décalque dans la zone basse, jamais de variation brusque (la lumière ne suit ni le kick ni
        // l'audio, seulement l'éclairage ambiant, la pluie et la distance).
        if in.graffiti != 0u && !far {
            let paint_fade = 1.0 - smoothstep(0.5 * wall.atlas.w, wall.atlas.w, dist);
            let paint_gain = (0.5 + 0.4 * lambert) * (1.0 - 0.3 * wet) * paint_fade;
            col = paint_wall(vec2<f32>(u, h), vec3<f32>(face, width, seed), in.graffiti, col, paint_gain);
        }

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
        col += v_c * rim * (0.7 + pulse * 1.3 + noisy * 0.8);
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

    let fog = smoothstep((90.0 - 30.0 * rain) * vista(), (380.0 - 100.0 * rain) * vista(), dist);
    // Le fondu efface le détail au loin ; la perspective atmosphérique (C3, `atmosphere.wesl`) découpe ce qui reste.
    return vec4<f32>(atmo_fog(mix(col, haze, fog), in.world, view.world_position), 1.0);
}
