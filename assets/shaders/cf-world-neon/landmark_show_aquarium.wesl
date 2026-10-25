// L'aquarium géant (`showcase/aquarium.ts`) : caustiques, 16 poissons, un requin par minute, des bulles.
//
// Paramètres : b.w = défilement du banc, intégré côté CPU (`time × (1 + basses × 2,5 + intensité)` de Sillage téléportait
// les poissons à chaque variation des basses).

const SPAN: f32 = 42.0;
const HEIGHT: f32 = 70.0;
const FISH: i32 = 16;

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let s = show();
    let time = s.time;
    let q = vec2<f32>((in.uv.x - 0.5) * SPAN, in.uv.y * HEIGHT);
    var col = s.ink * 0.85;

    // Eau : plus claire en haut, plus profonde en bas ; caustiques en réseau mouvant.
    let depth_k = 1.0 - q.y / HEIGHT;
    col = mix(s.a * 0.16, s.b * 0.04, depth_k);
    let cp = q * 0.22;
    let c1 = sin(cp.x * 3.1 + sin(cp.y * 2.3 + time * 0.8) * 2.0 + time * 0.6);
    let c2 = sin(cp.y * 3.7 + sin(cp.x * 2.7 - time * 0.7) * 2.0);
    let caustic = pow(1.0 - abs(c1 * c2), 5.0);
    col += s.a * caustic * (0.25 + 0.5 * (1.0 - depth_k)) * (0.7 + s.mid);

    // Poissons : chacun a sa couche, son sens, sa vitesse ; le corps est une ellipse, la queue un triangle qui bat.
    for (var i = 0; i < FISH; i++) {
        let fi = f32(i);
        let dir = select(1.0, -1.0, h11(fi * 3.1) < 0.5);
        let speed = 2.2 + h11(fi * 5.3) * 4.0;
        let len = 2.2 + h11(fi * 7.7) * 2.4;
        let span = SPAN + len * 4.0;
        let x = gl_mod(fi * 13.7 + s.clock_b * speed * dir + span, span) - span * 0.5;
        let y = 8.0 + h11(fi * 11.9) * (HEIGHT - 20.0) + sin(time * 0.9 + fi * 2.0) * 1.6;
        let fq = (q - vec2<f32>(x, y)) * vec2<f32>(dir, 1.0);
        let body = length(fq / vec2<f32>(len, len * 0.42)) - 1.0;
        let tail_wag = sin(time * 9.0 + fi * 3.0) * 0.5;
        let t = fq + vec2<f32>(len * 0.95, 0.0);
        var tail = max(abs(t.y + tail_wag * (-t.x) * 0.5) - (len * 0.1 + (t.x + len * 0.1) * 0.5), -t.x - len * 0.1);
        tail = max(tail, t.x - len * 0.9);
        let shape = soft(min(body, tail), 0.12);
        let tone = mix(s.c, mix(s.d, s.a, h11(fi * 1.3)), step(0.5, h11(fi * 2.9)));
        col += tone * shape * (0.8 + s.pulse * 0.8);
        col += s.a * soft(length(fq - vec2<f32>(-len * 0.55, len * 0.1)) - len * 0.05, 0.05) * shape;
    }

    // Le requin : une grande silhouette sombre, une fois par minute environ, qui traverse toute la cuve.
    let cycle = gl_mod(time, 56.0) / 14.0;
    if cycle < 1.0 {
        let sx = mix(-SPAN, SPAN, cycle);
        let sq = q - vec2<f32>(sx, HEIGHT * 0.6);
        let body = length(sq / vec2<f32>(11.0, 2.6)) - 1.0;
        let fin = max(abs(sq.x + 2.0) - (3.5 - sq.y) * 0.5, max(sq.y - 5.0, -sq.y));
        let shape = soft(min(body, fin), 0.2);
        col = mix(col, s.ink * 0.2, shape * 0.9);
        col += s.b * soft(abs(body) - 0.05, 0.1) * 0.5;
    }

    // Bulles : colonnes montantes, plus nombreuses au drop.
    for (var k = 0; k < 6; k++) {
        let fk = f32(k);
        let bx = (h11(fk * 9.1) - 0.5) * (SPAN - 8.0);
        let rise = fract(time * (0.12 + h11(fk) * 0.1) + h11(fk * 4.0));
        for (var j = 0; j < 4; j++) {
            let fj = f32(j);
            let by = fract(rise + fj * 0.25) * HEIGHT;
            let wob = sin(by * 0.5 + fk) * 0.8;
            let bubble = soft(abs(length(q - vec2<f32>(bx + wob + fj, by)) - (0.35 + h11(fk + fj) * 0.3)), 0.08);
            col += s.a * bubble * (0.6 + s.drop * 2.0);
        }
    }

    // Cadre de verre : un liseré clair, et un sable au fond.
    let edge = min(abs(abs(q.x) - (SPAN / 2.0 - 0.4)), abs(q.y - 0.4));
    col += s.a * soft(edge - 0.15, 0.2) * 0.6;
    let sand = step(q.y, 2.5 + sin(q.x * 0.4) * 0.5 + fbm(vec2<f32>(q.x * 0.2, 1.0)) * 1.4);
    col = mix(col, s.d * 0.12 + s.a * 0.03, sand);
    col += s.d * sand * step(0.93, h21(floor(q * 1.5))) * (0.5 + s.pulse);

    return vec4<f32>(city_fog(col, in.world), 1.0);
}
