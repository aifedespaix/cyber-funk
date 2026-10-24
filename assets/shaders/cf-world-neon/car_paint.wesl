// Peinture de la CyberCar : masque de peinture, teinte du thème, métal, paillettes, liseré néon de silhouette.
//
// Portage de `PaintSystem` (`public/models/car/src/CyberCarPaint.ts`, fragments GLSL injectés par `onBeforeCompile`) en
// extension de `StandardMaterial`. Fidèle au shader de Sillage, avec la finition « metallic » (la finition par défaut de
// Sillage : le sélecteur de finitions n'est pas repris).
//
// - Le GLB porte le masque de peinture dans la texture d'occlusion (canal R) : seules les zones masquées (les tôles) sont
//   repeintes, rendues métalliques et vernies ; pneus, garnitures et optiques restent tels quels.
// - Sans masque (`paint.x`... voir `mask_on`), un masque de repli par teinte (violet) est utilisé, comme dans Sillage.
// - Pas d'irisation : `StandardMaterial` n'a pas de film mince ; la finition « metallic » l'a d'ailleurs à 0.
//
// Le shader reprend le fragment de `pbr.wesl` de Bevy (entrée PBR depuis le matériau standard, éclairage, post-traitement)
// et ne modifie que l'entrée PBR avant l'éclairage.

import bevy_pbr::render::{
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
};

// `paint` : x teinte visée (0 à 1), y échelle de saturation, z échelle de valeur, w masque présent (1) ou non (0).
// `finish` : x rugosité, y métal, z paillettes, w liseré (déjà multiplié par le pouls côté processeur).
// `rim_a`, `rim_b` : les deux couleurs néon du liseré (lumière linéaire), côté gauche et côté droit de l'écran.
struct Paint {
    paint: vec4<f32>,
    finish: vec4<f32>,
    rim_a: vec4<f32>,
    rim_b: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> paint: Paint;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var mask_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var mask_sampler: sampler;

fn cc_hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn cc_rgb2hsv(c: vec3<f32>) -> vec3<f32> {
    let k = vec4<f32>(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let p = mix(vec4<f32>(c.bg, k.wz), vec4<f32>(c.gb, k.xy), step(c.b, c.g));
    let q = mix(vec4<f32>(p.xyw, c.r), vec4<f32>(c.r, p.yzx), step(p.x, c.r));
    let d = q.x - min(q.w, q.y);
    return vec3<f32>(abs(q.z + (q.w - q.y) / (6.0 * d + 1e-10)), d / (q.x + 1e-10), q.x);
}

fn cc_hsv2rgb(c: vec3<f32>) -> vec3<f32> {
    let k = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(c.xxx + k.xyz) * 6.0 - k.www);
    return c.z * mix(k.xxx, clamp(p - k.xxx, vec3<f32>(0.0), vec3<f32>(1.0)), c.y);
}

// Masque de repli (GLB sans masque) : les pixels violets de la texture de couleur.
fn cc_hue_mask(lin: vec3<f32>) -> f32 {
    let h = cc_rgb2hsv(pow(max(lin, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2)));
    return smoothstep(0.66, 0.70, h.x) * (1.0 - smoothstep(0.82, 0.86, h.x)) * smoothstep(0.15, 0.30, h.y);
}

@fragment
fn fragment(vertex_output: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var in = vertex_output;
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color,
    );

    // Masque de peinture : canal R de la texture d'occlusion du GLB.
    var pm = cc_hue_mask(pbr_input.material.base_color.rgb);
    if paint.paint.w > 0.5 {
        pm = textureSample(mask_texture, mask_sampler, in.uv).r;
    }

    // Teinte du thème : la teinte d'origine (violet) est déplacée vers la teinte visée, saturation et valeur mises à
    // l'échelle ; le reste de la texture (usure, reflets) est conservé.
    let base = pbr_input.material.base_color;
    let hsv = cc_rgb2hsv(pow(max(base.rgb, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2)));
    let target_hsv = vec3<f32>(paint.paint.x, clamp(hsv.y * paint.paint.y, 0.0, 1.0), clamp(hsv.z * paint.paint.z, 0.0, 1.0));
    let tinted = pow(cc_hsv2rgb(target_hsv), vec3<f32>(2.2));
    pbr_input.material.base_color = vec4<f32>(mix(base.rgb, tinted, pm), base.a);

    // Métal et rugosité de la finition, dans le masque seulement ; l'usure de la texture les module un peu.
    let rough = pbr_input.material.perceptual_roughness;
    let metal = pbr_input.material.metallic;
    pbr_input.material.perceptual_roughness = mix(rough, clamp(paint.finish.x + (rough - 0.5) * 0.5, 0.03, 1.0), pm);
    pbr_input.material.metallic = mix(metal, clamp(paint.finish.y + (metal - 0.5) * 0.3, 0.0, 1.0), pm);
    // Vernis : le masque le limite aux tôles.
    pbr_input.material.clearcoat = pbr_input.material.clearcoat * pm;

    // Liseré de Fresnel dans les couleurs néon (dessine la silhouette sur le noir) et paillettes métalliques.
    let v = normalize(pbr_input.V);
    let n = normalize(pbr_input.N);
    let ndv = clamp(dot(n, v), 0.0, 1.0);
    let rim = pow(1.0 - ndv, 3.0);
    // « côté gauche / droit de l'écran » : l'axe x de la caméra, comme `N_.x` en repère vue dans Sillage.
    let view_x = dot(n, view.world_from_view[0].xyz);
    var glow = mix(paint.rim_a.rgb, paint.rim_b.rgb, smoothstep(-0.4, 0.4, view_x)) * rim * paint.finish.w * pm;
    if paint.finish.z > 0.001 {
        let cell = floor(in.uv * 560.0);
        let h0 = cc_hash(cell);
        let jitter = vec3<f32>(cc_hash(cell + vec2<f32>(7.3)), cc_hash(cell + vec2<f32>(19.1)), cc_hash(cell + vec2<f32>(3.7)));
        let fn_ = normalize(n + (jitter - vec3<f32>(0.5)) * 0.9);
        let sparkle = step(0.82, h0) * pow(max(dot(fn_, v), 0.0), 140.0);
        glow = glow + mix(vec3<f32>(1.0), pbr_input.material.base_color.rgb * 3.0, 0.5) * sparkle * paint.finish.z * 0.55 * pm;
    }
    pbr_input.material.emissive = vec4<f32>(pbr_input.material.emissive.rgb + glow, pbr_input.material.emissive.a);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
