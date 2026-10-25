// La serre jungle (`showcase/greenhouse.ts`) : feuillage de bruit fractal qui monte du pied, nervures qui s'allument en
// vagues, fleurs, verrière, spores.
//
// Paramètres : b.w = phase de la vague de nervures, intégrée côté CPU (`time × (1,2 + basses × 2)` de Sillage faisait
// sauter la vague à chaque variation des basses).

const SPAN: f32 = 40.0;
const HEIGHT: f32 = 72.0;

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let s = show();
    let time = s.time;
    let q = vec2<f32>((in.uv.x - 0.5) * SPAN, in.uv.y * HEIGHT);
    var col = s.ink * 0.85;

    let g = vec2<f32>(q.x * 0.11, q.y * 0.07);
    let reach = 0.82 + s.intensity * 0.12;
    let height01 = q.y / HEIGHT;
    var mass = fbm(g * 1.6 + vec2<f32>(0.0, time * 0.02)) + (1.0 - height01 / reach) * 0.5 - 0.14;
    // Lianes qui pendent du faîtage : un feuillage de plus, du haut vers le bas.
    mass += smoothstep(0.7, 1.0, height01) * (0.3 + 0.35 * fbm(vec2<f32>(q.x * 0.4, 2.0)));
    let leaf = smoothstep(0.42, 0.5, mass);
    // Nervures : crêtes du bruit, qui s'allument en une vague montante.
    let ridge = 1.0 - abs(2.0 * fbm(g * 3.2 + 3.0) - 1.0);
    let vein = pow(ridge, 7.0);
    let wave = 0.5 + 0.5 * sin(q.y * 0.18 - s.clock_b + q.x * 0.05);
    let green = mix(s.ink * 1.2 + s.b * 0.03, s.a * 0.2, fbm(g * 4.0 + 9.0));
    col = mix(col + s.b * 0.012, green, leaf);
    col += s.b * vein * leaf * (0.25 + wave * 0.9 + s.pulse * 0.7 + s.drop * 1.2);
    // Fleurs : de petites corolles roses, ça et là sur la masse.
    let fc = floor(q / 4.5);
    let fp = fract(q / 4.5) - 0.5;
    let flower = step(0.9, h21(fc + 4.0)) * leaf * soft(length(fp) - 0.17 - 0.06 * s.pulse, 0.03);
    col += s.c * flower * (1.2 + s.mid * 1.5);

    // Verrière : un quadrillage de montants clairs, des reflets qui glissent en diagonale.
    let grid_x = soft(abs(fract(q.x / 5.0 + 0.5) - 0.5) * 5.0 - 0.1, 0.06);
    let grid_y = soft(abs(fract(q.y / 6.0) - 0.5) * 6.0 - 2.8, 0.06);
    col += s.d * 0.05 * max(grid_x, grid_y);
    col += s.d * 0.025 * smoothstep(0.85, 1.0, sin((q.x + q.y) * 0.12 - time * 0.3));

    // Spores : des points qui montent et vacillent.
    let sc = floor(vec2<f32>(q.x / 3.0, q.y / 3.0 - time * 0.4));
    var sp = vec2<f32>(fract(q.x / 3.0), fract(q.y / 3.0 - time * 0.4)) - 0.5;
    sp.x += sin(time + h21(sc) * 6.0) * 0.2;
    let spore = step(0.86, h21(sc + 7.0)) * soft(length(sp) - 0.07, 0.05);
    col += mix(s.a, s.d, h21(sc)) * spore * (0.6 + s.high * 2.0);
    // Pied : mousse lumineuse.
    col += s.b * exp(-q.y / 4.0) * 0.18 * (0.6 + leaf);

    return vec4<f32>(city_fog(col, in.world), 1.0);
}
