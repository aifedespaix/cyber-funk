// Néon plein d'une lettre de l'enseigne du DJ (`dj-sign-material.ts`). Le cœur (canal R de l'atlas) est presque blanc, le
// halo (G, B) prend la teinte de l'enseigne. Additif, sans écriture de profondeur : l'immeuble qui passe devant masque la
// lettre, la brume l'atténue à distance sans l'éteindre tout à fait (elle reste visible depuis l'horizon).
//
// Rien de ce qui clignote n'est calculé ici : `hue.w` (la lumière de la lettre) arrive du CPU, déjà bornée en vitesse de
// variation (règle 7 de `CLAUDE.md`, voir `djsign::anim::slew`).
//
// Mélange « alpha prémultiplié » : une sortie à alpha nul s'additionne à la scène.

import bevy_pbr::render::forward_io::VertexOutput;
import bevy_pbr::render::mesh_view_bindings::view;

// Même ordre de champs que `LetterParams` (Rust, `ShaderType`).
struct Letter {
    // Rectangle de la lettre dans l'atlas : u0, v0, u1, v1 (v vers le bas, comme l'image).
    rect: vec4<f32>,
    // Teinte du halo (lumière linéaire) ; w : lumière de la lettre, 0 éteinte, 1 allumée (peut dépasser pour un pouls).
    hue: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> letter: Letter;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var atlas: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var atlas_sampler: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = mix(letter.rect.xy, letter.rect.zw, in.uv);
    let t = textureSample(atlas, atlas_sampler, uv).rgb;
    let hue = letter.hue.rgb;
    let hot = mix(hue, vec3<f32>(1.0), 0.8);
    let col = hot * t.r * 3.0 + hue * (t.g * 1.4 + t.b * 0.55);
    let haze = 1.0 - 0.7 * smoothstep(160.0, 700.0, distance(in.world_position.xyz, view.world_position));
    return vec4<f32>(col * letter.hue.w * haze, 0.0);
}
