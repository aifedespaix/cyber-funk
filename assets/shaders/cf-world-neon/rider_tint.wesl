// Teinte de carrosserie des motos et de leurs pilotes : la couleur visée remplace celle de la texture, en gardant la
// luminance de la texture (usure, reliefs, ombres peintes). Le métal, la rugosité et les normales ne sont pas touchés :
// seule la couleur de base change, et seulement dans le masque (canal R de la texture d'occlusion) ou sur tout le matériau
// quand il n'y en a pas.
//
// Pourquoi par luminance et pas par décalage de teinte (la peinture de la voiture) : les carrosseries des motos sont
// grises ou brunes, sans teinte à décaler.
//
// Le shader reprend le fragment de `pbr.wesl` de Bevy et ne modifie que l'entrée PBR avant l'éclairage.

import bevy_pbr::render::{
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
};

// `tint` : rgb couleur visée (lumière linéaire), w masque présent (1) ou non (0).
// `params` : x luminance moyenne de la peinture d'origine, y luminosité de la couleur visée, z force (0 : rien).
struct Tint {
    tint: vec4<f32>,
    params: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> tint: Tint;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var mask_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var mask_sampler: sampler;

@fragment
fn fragment(vertex_output: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var in = vertex_output;
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color,
    );

    let base = pbr_input.material.base_color;
    let lum = dot(base.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    var amount = tint.params.z;
    if tint.tint.w > 0.5 {
        amount = amount * textureSample(mask_texture, mask_sampler, in.uv).r;
    }
    let tinted = tint.tint.rgb * tint.params.y * clamp(lum / max(tint.params.x, 0.0001), 0.0, 2.5);
    pbr_input.material.base_color = vec4<f32>(mix(base.rgb, tinted, amount), base.a);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
