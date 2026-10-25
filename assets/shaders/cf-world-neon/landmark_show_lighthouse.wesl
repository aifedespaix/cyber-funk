// Le phare (`showcase/lighthouse.ts`) : une tour rayée rouge et blanche, sa galerie et sa lanterne. Le faisceau qui balaie la
// ville est une pièce à part (`landmark_lighthouse_beam.wesl`).

const SPAN: f32 = 20.0;
const HEIGHT: f32 = 104.0;
const LAMP: f32 = 94.0;

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let s = show();
    let time = s.time;
    let q = vec2<f32>((in.uv.x - 0.5) * SPAN, in.uv.y * HEIGHT);
    var col = s.ink * 0.85;

    // Tour : rayures horizontales, léger fruit au sommet, fenêtres en meurtrières.
    let taper = SPAN * 0.5 - 1.0 - q.y * 0.035;
    let tower = step(abs(q.x), taper);
    let stripe = step(0.5, fract(q.y / 20.0));
    var paint = mix(s.a * 0.32, s.b * 0.2, stripe);
    paint *= 0.55 + 0.45 * cos(q.x / taper * 1.45);
    col = mix(col, paint, tower);
    let slit = rect(vec2<f32>(q.x, gl_mod(q.y, 20.0)), vec2<f32>(0.0, 10.0), vec2<f32>(0.45, 1.6)) * step(q.y, LAMP - 8.0) * tower;
    col += s.c * slit * (0.8 + s.mid * 1.2);
    // Galerie et lanterne.
    let gallery = rect(q, vec2<f32>(0.0, LAMP - 3.0), vec2<f32>(SPAN * 0.5 - 0.2, 0.7));
    col = mix(col, s.ink * 1.6 + s.b * 0.1, gallery);
    let lamp = rect(q, vec2<f32>(0.0, LAMP + 1.5), vec2<f32>(3.2, 3.2));
    let flash = 0.6 + 0.4 * sin(time * 4.0);
    col = mix(col, s.c * (2.0 + s.pulse * 2.5 + s.drop * 3.0) * flash, lamp);
    let roof = max(0.0, 1.0 - abs(q.x) / 4.2) * 3.0;
    col = mix(col, s.a * 0.5, step(q.y, LAMP + 4.7 + roof) * step(LAMP + 4.7, q.y));
    col += s.c * exp(-length(vec2<f32>(q.x, q.y - (LAMP + 1.5))) / 6.0) * 0.5;

    return vec4<f32>(city_fog(col, in.world), 1.0);
}
