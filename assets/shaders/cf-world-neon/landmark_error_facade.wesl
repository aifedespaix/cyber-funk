// La façade rouge d'ERROR : des tranches rouges qui se décalent et se déchirent avec la déchirure du panneau.
//
// Paramètres : a.x = déchirure (0..1, bornée par le limiteur). Au repos (`a.x = 0`) il ne reste que le balayage lent : les
// tranches n'apparaissent qu'avec la déchirure (règle 7 de `CLAUDE.md`). Écart avec Sillage : le motif change 3 fois
// par seconde (`5 + g × 20` dans Sillage) et **tout** est proportionnel à `g`.

const SPAN: f32 = 42.0;
const HEIGHT: f32 = 58.0;

@vertex
fn vertex(v: PlaneIn) -> PlaneOut {
    return plane_vertex(v);
}

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let time = p.frame.x;
    let g = p.a.x;
    let q = vec2<f32>(in.uv.x * SPAN, in.uv.y * HEIGHT);
    let row = floor(q.y / 1.2);
    let tick = floor(time * 3.0);
    let bar = step(1.0 - 0.04 - g * 0.3, h11(row + tick * 3.7)) * clamp(g * 4.0, 0.0, 1.0);
    let shift_x = (h11(row * 1.3 + tick) - 0.5) * 6.0 * g;
    let seg = step(0.5, fract((q.x + shift_x) / 8.0 + h11(row)));
    let scan = 0.5 + 0.5 * sin(q.y * 4.0 - time * 6.0);
    var col = color(C_ALARM) * (bar * seg * (0.6 + g * 1.4) + scan * 0.04 * (0.4 + p.frame.z));
    col += color(C_MINT) * step(0.98, h11(row + tick * 9.1)) * g * 0.5;
    return vec4<f32>(col * city_fade(in.world), 0.0);
}
