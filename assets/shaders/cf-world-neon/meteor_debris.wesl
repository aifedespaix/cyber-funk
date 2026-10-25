// Gerbe de débris incandescents d'une explosion : des carrés tournés vers la caméra, de côté fixe en mètres.
//
// Portage des `Points` de `engine/meteor-blast.ts` (`size: 4`, `sizeAttenuation`, couleurs par sommet). Les quatre sommets
// d'un débris sont à son centre (relatif au point d'impact, qui est la position de l'entité) ; le vertex shader les écarte
// selon les axes de la vue. La couleur est portée par le sommet : teinte de chaleur × niveau d'incandescence.

import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::mesh_functions::get_world_from_local;
import bevy_pbr::render::view_transformations::position_world_to_clip;

// Demi-côté d'un débris (m) : `size: 4` de Sillage, en perspective à 60° de champ vertical, soit ~2,3 m de côté.
const HALF_M: f32 = 1.15;

struct DebrisVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    // Coin du quad, 0 ou 1.
    @location(2) corner: vec2<f32>,
    @location(5) color: vec4<f32>,
}

struct DebrisVarying {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
}

@vertex
fn vertex(v: DebrisVertex) -> DebrisVarying {
    let center = (get_world_from_local(v.instance_index) * vec4<f32>(v.position, 1.0)).xyz;
    let right = view.world_from_view[0].xyz;
    let up = view.world_from_view[1].xyz;
    let c = (v.corner * 2.0 - 1.0) * HALF_M;
    var out: DebrisVarying;
    out.clip = position_world_to_clip(center + right * c.x + up * c.y);
    out.color = v.color.rgb;
    return out;
}

@fragment
fn fragment(in: DebrisVarying) -> @location(0) vec4<f32> {
    // Carré plein, comme les `Points` de Three.js sans texture. Alpha nul : Bevy mélange `AlphaMode::Add` en alpha
    // prémultiplié (`src + dst × (1 − α)`), la couleur s'ajoute donc telle quelle à l'image (voir `meteor_fx.wesl`).
    return vec4<f32>(in.color, 0.0);
}
