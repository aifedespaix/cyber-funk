// Un mot de l'atlas du panneau ERROR, rendu lettre par lettre (`GLYPH_GLSL` de `landmarks/glsl.ts`) : tranches décalées,
// aberration chromatique, lettres éteintes. Partagé par le panneau (`landmark_error_sign.wesl`) et le décalque de la route
// (`road.wesl`).
//
// L'atlas : quatre rangées de 1024 × 256, blanc sur noir, un mot par rangée, rangée 0 en haut de l'image. `word` : x =
// rangée, y et z = début et fin du texte (0..1), w = nombre de lettres. `uv` : v vers le haut, comme `vUv` de Sillage.
//
// Écart avec Sillage (règle 7 de `CLAUDE.md`) : les tranches et les lettres éteintes changeaient jusqu'à 24 et 30 fois par
// seconde (`6 + glitch × 18`, `4 + glitch × 26`). Ici : 3 par seconde, pas une de plus (WCAG 2.3.1), et **tout** l'effet est
// proportionnel à `glitch`, le niveau que le limiteur de flashs a accordé côté CPU : à 0 (flashs coupés, respiration), le mot
// est stable.

fn glyph_h11(x: f32) -> f32 {
    return fract(sin(x * 127.1) * 43758.5453);
}

// Renvoie un masque par canal : le rouge et le bleu sont décalés, le vert est le cœur.
fn glyph_word(tex: texture_2d<f32>, samp: sampler, uv: vec2<f32>, word: vec4<f32>, glitch: f32, t: f32) -> vec3<f32> {
    let slice = floor(uv.y * 16.0);
    let tick = floor(t * 3.0);
    let hit = step(1.0 - 0.12 - glitch * 0.5, glyph_h11(slice * 7.3 + tick));
    let jump = (glyph_h11(slice + tick * 1.7) - 0.5) * 0.3 * glitch * hit;
    let shift = 0.003 + glitch * 0.02;
    // Le mot occupe la rangée `word.x` en comptant depuis le haut de l'image (v vers le bas en WGSL).
    let v = (word.x + 1.0 - uv.y) / 4.0;
    var m = vec3<f32>(0.0);
    for (var c = 0; c < 3; c++) {
        let x = uv.x + jump + (f32(c) - 1.0) * shift;
        let letter = floor((x - word.y) / max(word.z - word.y, 0.001) * word.w);
        let off = step(glyph_h11(letter * 13.1 + floor(t * 3.0)), glitch * 0.55);
        m[c] = textureSampleLevel(tex, samp, vec2<f32>(x, v), 0.0).r * (1.0 - off);
    }
    return m;
}
