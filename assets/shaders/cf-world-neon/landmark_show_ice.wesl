// La falaise de glace (`showcase/ice.ts`) : facettes de Voronoï, arêtes claires, givre au pied et sur les bords, étincelles.

const SPAN: f32 = 38.0;
const HEIGHT: f32 = 80.0;

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let s = show();
    let time = s.time;
    let q = vec2<f32>((in.uv.x - 0.5) * SPAN, in.uv.y * HEIGHT);
    var col = s.ink * 0.85;

    // Voronoï : la facette la plus proche, la deuxième (pour l'arête).
    let gp = vec2<f32>(q.x * 0.22, q.y * 0.2);
    let cell = floor(gp);
    var d1 = 9.0;
    var d2 = 9.0;
    var id = vec2<f32>(0.0);
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let g = cell + vec2<f32>(f32(i), f32(j));
            let o = vec2<f32>(h21(g + 3.1), h21(g + 7.7));
            let d = length(gp - (g + o));
            if d < d1 {
                d2 = d1;
                d1 = d;
                id = g;
            } else if d < d2 {
                d2 = d;
            }
        }
    }
    let tone = h21(id + 1.7);
    let edge = sstep(0.1, 0.0, d2 - d1);
    // Teinte de facette : bleu profond à glace claire, avec une vague de lumière qui monte.
    let shine = smoothstep(0.82, 1.0, sin(q.y * 0.12 - time * 0.9 + tone * 6.0));
    var facet = mix(s.b * 0.06, s.a * 0.2, tone) + s.c * 0.03 * h21(id + 5.0);
    facet += s.a * shine * 0.3;
    col = facet + s.a * edge * (0.5 + s.pulse * 0.9 + s.drop * 1.5);
    // Cœur de glace : un halo froid qui bat au centre de la falaise.
    col += s.b * exp(-length(vec2<f32>(q.x, q.y - HEIGHT * 0.45)) / 22.0) * (0.12 + s.bass * 0.4);
    // Étincelles : une facette sur quinze s'allume un instant.
    let tick = floor(time * 2.0 + tone * 5.0); // max 2 Hz
    let spark = step(0.93, h21(id + tick)) * sstep(0.5, 0.0, d1);
    col += s.d * spark * 2.2;
    // Givre : bruit blanc qui mange le pied et les bords.
    let edge_dist = min(SPAN / 2.0 - abs(q.x), q.y);
    let frost = sstep(7.0, 0.0, edge_dist + fbm(q * 0.35) * 6.0);
    col = mix(col, s.a * (0.25 + 0.2 * fbm(q * 0.9)), frost * 0.75);
    col += s.d * s.drop * smoothstep(0.7, 1.0, h21(id + 11.0)) * 1.5;

    return vec4<f32>(city_fog(col, in.world), 1.0);
}
