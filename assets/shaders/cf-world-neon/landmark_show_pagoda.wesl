// La pagode néon (`showcase/pagoda.ts`) : cinq étages de paravents de papier allumés et cinq rangées de lanternes qui se
// balancent au tempo. Lumière seule, par-dessus les blocs de ville (`landmark_pagoda_blocks.wesl`) : additif.

const SPAN: f32 = 32.0;
const HEIGHT: f32 = 92.0;
const TIERS: i32 = 5;
const TIER_H: f32 = 14.0;
const BASE_Y: f32 = 4.0;
const WIDTH0: f32 = 30.0;
const EAVE: f32 = 4.5;

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let s = show();
    let time = s.time;
    let q = vec2<f32>((in.uv.x - 0.5) * SPAN, in.uv.y * HEIGHT);
    // Rien n'est peint sur le volume lui-même : seulement les paravents et les lanternes, par étage.
    var col = vec3<f32>(0.0);
    let tier = floor((q.y - BASE_Y) / TIER_H);
    let local_y = q.y - BASE_Y - tier * TIER_H;
    let width = WIDTH0 - tier * 4.0;
    if tier >= 0.0 && tier < f32(TIERS) {
        let bay = floor((q.x + width * 0.5) / (width / 3.0));
        let cx = (bay + 0.5) * (width / 3.0) - width * 0.5;
        let screen = rect(vec2<f32>(q.x, local_y), vec2<f32>(cx, TIER_H * 0.45), vec2<f32>(width / 6.0 - 0.9, TIER_H * 0.3));
        let in_room = step(abs(q.x), width * 0.5);
        let grid = soft(min(abs(fract((q.x - cx) / 1.2 + 0.5) - 0.5) * 1.2, abs(fract(local_y / 1.2) - 0.5) * 1.2) - 0.05, 0.05);
        let lit = 0.5 + 0.5 * step(0.4, h21(vec2<f32>(bay, tier + floor(time * 0.3))));
        let paper = mix(s.b, s.c, h11(tier * 3.0 + bay)) * (0.35 + 0.35 * lit + s.pulse * 0.4 + s.drop * 1.2);
        col += paper * screen * in_room * (1.0 - grid * 0.6);
    }
    // Lanternes sous les avant-toits : une rangée par étage, qui balance au tempo.
    for (var t = 0; t < TIERS; t++) {
        let ft = f32(t);
        let roof_y = BASE_Y + (ft + 1.0) * TIER_H - 1.2;
        let rw = WIDTH0 - ft * 4.0 + EAVE * 2.0 - 3.0;
        let n = floor(rw / 3.0);
        let k = floor((q.x + rw * 0.5) / 3.0);
        let lx = (k + 0.5) * 3.0 - rw * 0.5;
        let side = select(-1.0, 1.0, gl_mod(k, 2.0) < 0.5);
        let sway = sin(time * 2.0 + k * 0.9 + ft) * 0.35 + side * cos(3.14159 * (s.beat + s.phase)) * 0.15;
        let lp = vec2<f32>(lx + sway, roof_y - 1.8 - h11(k + ft) * 0.6);
        let in_row = step(0.0, k) * step(k, n - 1.0);
        let cord = soft(sd_seg(q, vec2<f32>(lx, roof_y), lp + vec2<f32>(0.0, 0.5)) - 0.04, 0.04);
        let lantern = soft(sd_box(q, lp, vec2<f32>(0.45, 0.7)), 0.08);
        let tone = mix(s.a, s.b, h11(k * 1.7 + ft));
        col += (s.ink * 0.5 + tone * 0.2 * cord) * cord * in_row;
        col += tone * lantern * in_row * (0.9 + s.pulse * 1.6 + s.drop * 2.0);
    }
    // Flèche : un feu au sommet.
    col += s.c * exp(-length(vec2<f32>(q.x, q.y - (HEIGHT - 3.0))) / 3.5) * (0.6 + s.pulse);

    // Additif : la lumière seule, atténuée avec la distance (pas de brouillard qui ajouterait de la lumière).
    return vec4<f32>(col * city_fade(in.world), 0.0);
}
