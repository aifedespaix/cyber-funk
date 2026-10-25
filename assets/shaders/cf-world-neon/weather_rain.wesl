// La pluie de Neon City : des traînées fines, toutes animées dans le vertex shader (aucune mise à jour côté CPU).
//
// Portage de `engine/rain.ts` de Sillage (GLSL → WGSL/WESL), mêmes constantes. Chaque goutte tombe à vitesse propre dans
// une boîte qui se recentre sur la caméra mais reste ancrée dans le monde, inclinée vers l'arrière par la vitesse de la
// voiture. Elles s'illuminent dans le faisceau des phares et près des enseignes (le champ de la ville, `field.wesl`, ajouté
// au bout de ce fichier).
//
// Repère : tout est calculé en repère rendu (relatif à l'origine flottante de `cf-render`), comme `in.world`. Le maillage
// n'a que des sommets nuls : la position de l'entité (le centre de la boîte) n'est lue que pour décider où replier les
// gouttes ; le vertex shader n'applique pas la matrice du maillage.
//
// Densité (qualité adaptative) : une goutte n'existe que si sa graine passe sous la densité. Les quatre sommets d'une goutte
// absente sont envoyés derrière le plan lointain : le rasterizer rejette le quad sans produire un seul fragment.

import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::mesh_functions::get_world_from_local;
import bevy_pbr::render::view_transformations::position_world_to_clip;

// Même ordre de champs que `RainParams` (Rust, `ShaderType`). Tout en vec4.
struct Rain {
    // xyz : la voiture en repère rendu (`uCar`), w : densité de gouttes affichées, 0 → 1 (`uDensity`).
    car: vec4<f32>,
    // x : temps (s), y : inclinaison (`uTilt`).
    frame: vec4<f32>,
    // xyz : la boîte de pluie (m).
    box_size: vec4<f32>,
    ice: vec4<f32>,
    mood: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> rain: Rain;

// `mod(x, y)` de GLSL (x - y * floor(x / y)), différent de `%` en WGSL pour les négatifs.
fn gl_mod(x: f32, y: f32) -> f32 {
    return x - y * floor(x / y);
}

fn sstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge0 < edge1 {
        return smoothstep(edge0, edge1, x);
    }
    return 1.0 - smoothstep(edge1, edge0, x);
}

struct RainVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    // Coin du quad : x ∈ {-1, 1} (largeur), y ∈ {0, 1} (le long de la traînée).
    @location(2) corner: vec2<f32>,
    // Graine de la goutte : xy position dans la boîte, z vitesse et phase, w seuil de densité.
    @location(5) seed: vec4<f32>,
}

struct RainVarying {
    @builtin(position) clip: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) world: vec3<f32>,
    @location(2) dist: f32,
}

@vertex
fn vertex(v: RainVertex) -> RainVarying {
    var out: RainVarying;
    out.corner = v.corner;
    // Goutte absente : derrière le plan lointain (z > w), le quad est rejeté.
    if v.seed.w > rain.car.w {
        out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
        out.world = vec3<f32>(0.0);
        out.dist = 0.0;
        return out;
    }
    let box_size = rain.box_size.xyz;
    let center = get_world_from_local(v.instance_index)[3].xyz;
    let speed = 34.0 + v.seed.z * 16.0;
    let fall = normalize(vec3<f32>(0.04, -1.0, rain.frame.y));
    // Position dans la boîte, ancrée dans le monde : elle tombe en boucle, et suit la caméra par repli.
    var p = vec3<f32>(
        v.seed.x * box_size.x,
        box_size.y - gl_mod(v.seed.z * box_size.y + rain.frame.x * speed, box_size.y),
        v.seed.y * box_size.z,
    );
    p.x = center.x + gl_mod(p.x - center.x + box_size.x * 0.5, box_size.x) - box_size.x * 0.5;
    p.z = center.z + gl_mod(p.z - center.z + box_size.z * 0.5, box_size.z) - box_size.z * 0.5;
    let len = 1.2 + v.seed.z * 1.4;
    let view_dir = normalize(p - view.world_position);
    let right = normalize(cross(fall, view_dir));
    let world = p - fall * len * v.corner.y + right * 0.014 * v.corner.x;
    out.world = world;
    out.dist = length(world - view.world_position);
    out.clip = position_world_to_clip(world);
    return out;
}

// Bevy mélange `AlphaMode::Add` en alpha prémultiplié : la couleur arrive déjà multipliée par l'alpha, avec un alpha nul
// (voir `meteor_fx.wesl`).
fn additive(color: vec3<f32>, alpha: f32) -> vec4<f32> {
    return vec4<f32>(color * alpha, 0.0);
}

@fragment
fn fragment(in: RainVarying) -> @location(0) vec4<f32> {
    let across = 1.0 - abs(in.corner.x);
    let along = sin(in.corner.y * 3.14159);
    var col = mix(rain.ice.rgb, rain.mood.rgb, 0.35) * 0.22;
    // Faisceau des phares : un cône devant la voiture, où chaque goutte accroche la lumière.
    let rel = in.world - rain.car.xyz;
    let ahead = -rel.z;
    let cone = step(0.0, ahead) * (1.0 - smoothstep(0.0, 26.0, ahead))
        * (1.0 - smoothstep(0.0, 1.0, length(vec2<f32>(rel.x, rel.y - 0.7)) / (1.2 + ahead * 0.11)));
    col += rain.ice.rgb * cone * 1.6;
    // Enseignes, néons et immeubles-vitrines éclairent aussi la pluie.
    col += field_light(in.world, vec3<f32>(0.0, 1.0, 0.0)) * 0.9;
    let dist_fade = 1.0 - smoothstep(60.0, 150.0, in.dist);
    return additive(col, across * along * dist_fade);
}
