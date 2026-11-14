// Les poussières d'ambiance (chantier C3, DA §3.2) : trois coquilles autour de la caméra (proche, médiane, lointaine), un
// seul maillage statique, tout dans le vertex shader. Aucune mise à jour côté CPU, aucun état : la position d'un grain est
// `p0(graine) + dérive(t)`, repliée autour de la caméra.
//
// Une poussière n'est visible que là où il y a de la lumière : sa couleur est celle de la brume locale (bloc `SceneLight`
// de `atmosphere.wesl`, ajouté au bout de ce fichier), relevée par les transitoires du son.
//
// Densité (qualité adaptative) : un grain n'existe que si son seuil passe sous la densité de sa coquille. Les quatre
// sommets d'un grain absent partent derrière le plan lointain : zéro fragment (comme la pluie).

import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::view_transformations::position_world_to_clip;

// Même ordre de champs que `DustParams` (Rust, `ShaderType`). Tout en vec4.
struct Dust {
    // x : temps (s, replié), y : vent de la brume (m/s), z : enveloppe des transitoires 0..1, w : vitesse du sujet (m/s).
    frame: vec4<f32>,
    // xyz : part de grains affichés par coquille (proche, médiane, lointaine), w : gain de luminance.
    density: vec4<f32>,
    // Rayon intérieur de chaque coquille (m) ; w : 1 si le scintillement est permis.
    inner: vec4<f32>,
    // Rayon extérieur de chaque coquille (m).
    outer: vec4<f32>,
    // Taille à l'écran (px) de chaque coquille : minimum.
    size_min: vec4<f32>,
    // Taille à l'écran (px) de chaque coquille : maximum.
    size_max: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> dust: Dust;

struct DustVertex {
    @location(0) position: vec3<f32>,
    // Coin du quad, −1..1.
    @location(2) corner: vec2<f32>,
    // x : coquille (0, 1, 2), y : identifiant du grain 0..1.
    @location(3) info: vec2<f32>,
    // xyz : position dans la boîte de la coquille 0..1, w : seuil de densité.
    @location(5) seed: vec4<f32>,
}

struct DustVarying {
    @builtin(position) clip: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) color: vec3<f32>,
}

fn dust_mod(x: vec3<f32>, y: f32) -> vec3<f32> {
    return x - y * floor(x / y);
}

// Hachage entier (lowbias32) : pas de `sin`, stable d'un GPU à l'autre.
fn dust_hash(x_in: u32) -> f32 {
    var x = x_in;
    x ^= x >> 16u;
    x *= 0x7feb352du;
    x ^= x >> 15u;
    x *= 0x846ca68bu;
    x ^= x >> 16u;
    return f32(x >> 8u) / 16777216.0;
}

fn dust_absent() -> DustVarying {
    var out: DustVarying;
    out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    out.corner = vec2<f32>(0.0);
    out.color = vec3<f32>(0.0);
    return out;
}

@vertex
fn vertex(v: DustVertex) -> DustVarying {
    let shell = i32(v.info.x + 0.5);
    if v.seed.w >= dust.density[shell] {
        return dust_absent();
    }
    let cam = view.world_position;
    let r_in = dust.inner[shell];
    let r_out = dust.outer[shell];
    let time = dust.frame.x;
    let id = v.info.y;
    // Dérive lente propre au grain (proche), vent de la brume (médiane) ; la coquille lointaine ne bouge pas.
    var drift = vec3<f32>(0.0);
    if shell == 0 {
        let a = id * 6.2831853;
        drift = 0.1 * time * vec3<f32>(cos(a), 0.35 * sin(a * 3.0), sin(a));
    } else if shell == 1 {
        drift = vec3<f32>(dust.frame.y * time, 0.05 * time, 0.3 * dust.frame.y * time);
    }
    // Position ancrée dans le monde, repliée dans le cube de la coquille autour de la caméra.
    let side = 2.0 * r_out;
    let p = cam + dust_mod(v.seed.xyz * side + drift - cam + r_out, side) - r_out;
    let to_cam = p - cam;
    let dist = length(to_cam);
    // Hors de la coquille (coins du cube, trop près de l'objectif) : le grain s'efface.
    let band = smoothstep(r_in, r_in * 1.3 + 0.01, dist) * (1.0 - smoothstep(r_out * 0.8, r_out, dist));
    if band <= 0.0 {
        return dust_absent();
    }
    // Taille à l'écran en pixels, convertie en mètres à la distance du grain.
    let px = mix(dust.size_min[shell], dust.size_max[shell], fract(id * 7.31));
    let world_per_px = 2.0 * dist / (view.clip_from_view[1][1] * max(view.viewport.w, 1.0));
    let half = 0.5 * px * world_per_px;
    let right = view.world_from_view[0].xyz;
    let up = view.world_from_view[1].xyz;
    // Traînée par la vitesse : seuls les grains proches s'étirent le long de la route.
    var along = vec3<f32>(0.0);
    if shell == 0 {
        along = vec3<f32>(0.0, 0.0, 1.0) * min(dust.frame.w * 0.004, 0.25) * v.corner.y;
    }
    let world = p + right * half * v.corner.x + up * half * v.corner.y + along;

    // La lumière locale : la brume, ce qu'elle diffuse, et les tubes proches (loi en 1/r²).
    let dir = to_cam / max(dist, 0.001);
    var light = scene.c_fog.rgb
        + (scene.c_key.rgb * atmo_phase(dot(dir, scene.dir_key.xyz), scene.fog.z)
            + scene.c_rim.rgb * atmo_phase(dot(dir, scene.dir_rim.xyz), scene.fog.z)) * scene.c_fog.w;
    for (var i = 0; i < 4; i++) {
        let l = scene.tube_pos[i].xyz - p;
        light += scene.tube_col[i].rgb / max(dot(l, l), 1.0) * 40.0;
    }
    var gain = dust.density.w * band * (1.0 + 2.0 * dust.frame.z);
    // Scintillement des lointaines : une valeur tirée 3 fois par seconde et fondue vers la suivante (jamais de saut),
    // bornée à ±30 % (jamais d'extinction franche). DA §3.2 demandait 8 Hz : ramené sous le seuil de la règle 7.
    if shell == 2 && dust.inner.w > 0.5 {
        let beat3 = time * 3.0;
        let tick = u32(floor(beat3)); // max 3 Hz
        let key = u32(id * 16777216.0);
        let a = dust_hash(key ^ (tick * 747796405u));
        let b = dust_hash(key ^ ((tick + 1u) * 747796405u));
        gain *= 0.7 + 0.6 * mix(a, b, smoothstep(0.0, 1.0, fract(beat3)));
    }
    var out: DustVarying;
    out.clip = position_world_to_clip(world);
    out.corner = v.corner;
    out.color = light * gain;
    return out;
}

@fragment
fn fragment(in: DustVarying) -> @location(0) vec4<f32> {
    // Disque flou. Additif en alpha prémultiplié : couleur déjà multipliée, alpha nul (voir `weather_rain.wesl`).
    let r2 = dot(in.corner, in.corner);
    let a = exp(-3.0 * r2) * (1.0 - smoothstep(0.8, 1.0, r2));
    return vec4<f32>(in.color * a, 0.0);
}
