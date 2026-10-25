// Mur de code (`matrix-building.ts`) : les glyphes et la couleur du flux, partagés par le mur et les glyphes qui jaillissent.
//
// Paramètres (`p.a`, `p.b`) : a.rgb = teinte courante du flux (vert terminal, rouge sang ou cyan, fondue d'une palette à
// l'autre), a.w = part d'arc-en-ciel (0 à 1, fondue), b.x = largeur du mur (m), b.y = variante (0 pluie, 1 flux, 2 binaire), c.x = défilement du code (intégré côté CPU).

const HEIGHT: f32 = 74.0;

// Glyphe pseudo-katakana : une grille 5×7 de points tirés au hasard, sans police.
fn glyph_kana(cell_uv: vec2<f32>, id: f32, blur: f32) -> f32 {
    let g = cell_uv * vec2<f32>(5.0, 7.0);
    let gi = floor(g);
    let f = fract(g) - 0.5;
    let on = step(0.52, h21(vec2<f32>(id, gi.x + gi.y * 5.0)));
    let inside = step(0.0, gi.x) * step(gi.x, 4.0) * step(0.0, gi.y) * step(gi.y, 6.0);
    let shape = 1.0 - smoothstep(0.28 + blur * 0.2, 0.5 + blur * 0.4, max(abs(f.x), abs(f.y)));
    return on * inside * shape;
}

// Chiffre binaire : un anneau pour 0, un trait avec son drapeau pour 1.
fn glyph_digit(cell_uv: vec2<f32>, id: f32, blur: f32) -> f32 {
    let q = cell_uv - 0.5;
    let soft = 0.05 + blur * 0.18;
    let one = step(0.5, h11(id * 1.37));
    let ring = sstep(soft, 0.0, abs(length(q * vec2<f32>(1.25, 0.82)) - 0.3) - 0.07);
    let bar = sstep(soft, 0.0, abs(q.x + 0.04) - 0.07) * sstep(soft, 0.0, abs(q.y) - 0.36);
    let flag = sstep(soft, 0.0, abs(q.y - 0.3 - (q.x + 0.04) * 0.9) - 0.06) * step(-0.28, q.x) * step(q.x, -0.03);
    return mix(ring, max(bar, flag), one);
}

fn glyph(cell_uv: vec2<f32>, id: f32, blur: f32, variant: f32) -> f32 {
    if variant > 1.5 {
        return glyph_digit(cell_uv, id, blur);
    }
    return glyph_kana(cell_uv, id, blur);
}

// Rampe de l'arc-en-ciel, bouclée : toutes les teintes viennent de la palette.
fn ramp(i: u32) -> vec3<f32> {
    switch i % 6u {
        case 0u: { return color(C_ALARM); }
        case 1u: { return color(C_AMBER); }
        case 2u: { return color(C_SUN); }
        case 3u: { return color(C_LIME); }
        case 4u: { return color(C_MINT); }
        default: { return color(C_VIOLET); }
    }
}

fn ramp_at(t: f32) -> vec3<f32> {
    let x = fract(t) * 6.0;
    let i = u32(floor(x));
    return mix(ramp(i), ramp(i + 1u), fract(x));
}

fn flow_color(along: f32, lum: f32) -> vec3<f32> {
    let base = mix(p.a.rgb, ramp_at(along), p.a.w);
    return mix(base, color(C_ICE), clamp(lum - 0.85, 0.0, 1.0)) * lum;
}
