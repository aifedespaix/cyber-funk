// Projecteurs du décor (`beams.ts`) : le faisceau (cône évasé, additif) et la tache qu'il pose sur la route (disque).
//
// Un seul shader, le type est dans l'uniforme (`kind`). La teinte de chaque projecteur voyage dans son `MeshTag` (RVB, 10
// bits par canal) : tous les cônes se regroupent en un lot de dessin, toutes les taches en un autre, et le niveau d'éclat
// (`level`) est le même pour tous, écrit une fois par image.
//
// `level` est toujours un niveau **accordé** (voir `beams/safety.rs`) : médiums lissés plus pouls de kick que le limiteur
// de flashs laisse passer. Jamais un kick brut.
//
// Bevy mélange `AlphaMode::Add` en alpha prémultiplié (`src + dst × (1 − α)`) : chaque lumière rend sa couleur et un alpha
// nul, ce qui s'ajoute à l'image comme le `AdditiveBlending` de Three.js.

import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::mesh_functions::{get_world_from_local, get_tag};
import bevy_pbr::render::view_transformations::position_world_to_clip;

// Même ordre de champs que `BeamParams` (Rust, `ShaderType`).
struct Beam {
    // x : niveau d'éclat, y : type (0 cône, 1 tache).
    p: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> beam: Beam;

struct BeamIn {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

struct BeamOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Position le long du faisceau, 0 à la pointe, 1 à la base évasée.
    @location(2) along: f32,
    @location(3) uv: vec2<f32>,
    @location(4) tint: vec3<f32>,
}

@vertex
fn vertex(v: BeamIn) -> BeamOut {
    let m = get_world_from_local(v.instance_index);
    let wp = m * vec4<f32>(v.position, 1.0);
    let tag = get_tag(v.instance_index);
    var out: BeamOut;
    out.world = wp.xyz;
    // Normale radiale du cône (y = 0) : l'étirement le long de l'axe ne la change pas.
    out.normal = normalize((m * vec4<f32>(v.normal, 0.0)).xyz);
    out.along = v.position.y;
    out.uv = v.uv;
    out.tint = vec3<f32>(
        f32(tag & 0x3FFu),
        f32((tag >> 10u) & 0x3FFu),
        f32((tag >> 20u) & 0x3FFu),
    ) / 1023.0;
    out.clip = position_world_to_clip(wp.xyz);
    return out;
}

// Au-delà de 100 m le projecteur s'estompe, éteint à 300 m (la brume l'a avalé).
fn far_fade(world: vec3<f32>) -> f32 {
    return 1.0 - smoothstep(100.0, 300.0, distance(world, view.world_position));
}

@fragment
fn fragment(in: BeamOut) -> @location(0) vec4<f32> {
    let level = beam.p.x;
    let far = far_fade(in.world);
    if beam.p.y < 0.5 {
        // Cône : une tranche fine, bords doux (facing), pointe qui s'allume, base qui s'estompe.
        let facing = abs(dot(normalize(in.normal), normalize(view.world_position - in.world)));
        let soft = pow(facing, 1.8);
        let along = (1.0 - in.along * 0.5) * smoothstep(0.0, 0.05, in.along);
        return vec4<f32>(in.tint * (soft * along * far * level), 0.0);
    }
    // Tache au sol : corps doux et liseré.
    let d = length(in.uv - vec2<f32>(0.5)) * 2.0;
    let body = pow(1.0 - smoothstep(0.0, 1.0, d), 1.6);
    let rim = smoothstep(0.78, 0.92, d) * (1.0 - smoothstep(0.92, 1.0, d)) * 0.5;
    return vec4<f32>(in.tint * ((body * 0.6 + rim) * far * level), 0.0);
}
