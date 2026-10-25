// La manette géante (`showcase/console.ts`) : un écran de pixels et d'envahisseurs, la coque d'une manette, une croix
// directionnelle et quatre boutons qui s'allument l'un après l'autre à chaque kick.
//
// Les boutons et la croix sont de petites surfaces (≈ 1 % de la façade) qui changent à chaque temps. Au-delà de 180 BPM
// elles passeraient les 3 Hz : leur luminance reste bornée (≤ 2,4 sur une surface de quelques m²), loin du seuil de
// surface de la règle 7 ; le blanc plein du drop, lui, passe par le niveau borné `drop`.

const SPAN: f32 = 40.0;
const HEIGHT: f32 = 66.0;

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let s = show();
    let time = s.time;
    let q = vec2<f32>((in.uv.x - 0.5) * SPAN, in.uv.y * HEIGHT);
    var col = s.ink * 0.85;

    // Écran : des pixels qui défilent vers le bas, plus vifs avec les aigus.
    let px = floor(vec2<f32>(q.x, q.y + time * 5.0) / 1.4);
    let n = h21(px);
    col += s.d * 0.05 * step(0.8, n) * (0.4 + s.high);
    // Envahisseurs : motifs symétriques de 5×4 pixels, en rangées espacées.
    let ip = vec2<f32>(q.x + time * 2.0, q.y);
    let icell = floor(ip / vec2<f32>(7.0, 6.0));
    let ilocal = (ip - icell * vec2<f32>(7.0, 6.0)) / 1.2;
    var inv = 0.0;
    if ilocal.x < 5.0 && ilocal.y < 4.0 && h21(icell + 6.0) > 0.45 {
        let g = floor(ilocal);
        let mx = min(g.x, 4.0 - g.x);
        inv = step(0.5, h21(vec2<f32>(mx, g.y) + icell * 3.1));
    }
    col += mix(s.c, s.d, h21(icell + 2.0)) * inv * (0.2 + s.mid * 0.4) * step(q.y, 60.0) * step(4.0, q.y);

    // Coque de la manette : un corps arrondi, deux poignées, des épaulières.
    let center = sd_round(q, vec2<f32>(0.0, 30.0), vec2<f32>(17.0, 9.0), 5.0);
    let grip_l = length(q - vec2<f32>(-14.0, 23.0)) - 9.0;
    let grip_r = length(q - vec2<f32>(14.0, 23.0)) - 9.0;
    let shoulder = min(
        sd_round(q, vec2<f32>(-11.0, 41.5), vec2<f32>(7.0, 1.8), 1.5),
        sd_round(q, vec2<f32>(11.0, 41.5), vec2<f32>(7.0, 1.8), 1.5),
    );
    let body = min(min(center, min(grip_l, grip_r)), shoulder);
    let shell = soft(body, 0.15);
    col = mix(col * 0.2, s.ink * 1.2 + s.a * 0.015, shell);
    col += s.a * soft(abs(body) - 0.2, 0.12) * (0.7 + s.pulse * 1.1 + s.drop * 1.5);

    // Croix directionnelle : quatre bras, celui qui répond aux basses s'allume.
    let dp = q - vec2<f32>(-10.0, 32.0);
    let plus = max(rect(dp, vec2<f32>(0.0), vec2<f32>(4.2, 1.5)), rect(dp, vec2<f32>(0.0), vec2<f32>(1.5, 4.2)));
    let arm = gl_mod(floor(s.beat), 4.0);
    var arm_lit = 0.0;
    arm_lit += rect(dp, vec2<f32>(0.0, 2.7), vec2<f32>(1.5, 1.5)) * step(abs(arm - 0.0), 0.5);
    arm_lit += rect(dp, vec2<f32>(2.7, 0.0), vec2<f32>(1.5, 1.5)) * step(abs(arm - 1.0), 0.5);
    arm_lit += rect(dp, vec2<f32>(0.0, -2.7), vec2<f32>(1.5, 1.5)) * step(abs(arm - 2.0), 0.5);
    arm_lit += rect(dp, vec2<f32>(-2.7, 0.0), vec2<f32>(1.5, 1.5)) * step(abs(arm - 3.0), 0.5);
    col = mix(col, s.ink * 0.5 + s.d * 0.1, plus);
    col += s.d * arm_lit * (1.5 + s.bass * 3.0 + s.drop * 3.0);

    // Quatre boutons en losange, allumés en tournant : un par kick.
    for (var i = 0; i < 4; i++) {
        let fi = f32(i);
        let a = fi * 1.5708 + 0.785;
        let bp = vec2<f32>(10.0, 32.0) + vec2<f32>(cos(a), sin(a)) * 4.2;
        let on = step(abs(gl_mod(floor(s.beat), 4.0) - fi), 0.5);
        let tone = select(select(select(s.d, s.c, fi < 2.5), s.b, fi < 1.5), s.a, fi < 0.5);
        let disc = soft(length(q - bp) - 2.1, 0.12);
        let ring = soft(abs(length(q - bp) - 2.7) - 0.15, 0.1);
        col = mix(col, tone * (0.15 + on * (2.4 + s.pulse * 1.5)), disc);
        col += tone * ring * 0.4;
    }
    // Sticks : deux disques qui se balancent au tempo.
    let sway = cos(3.14159 * (s.beat + s.phase));
    col += s.d * soft(length(q - vec2<f32>(-6.0, 24.0) - vec2<f32>(sway, sin(time)) * 0.8) - 2.4, 0.12) * 0.7;
    col += s.d * soft(length(q - vec2<f32>(6.0, 24.0) - vec2<f32>(-sway, cos(time)) * 0.8) - 2.4, 0.12) * 0.7;
    col += s.d * soft(abs(length(q - vec2<f32>(-6.0, 24.0)) - 3.6) - 0.15, 0.1) * 0.5;
    col += s.d * soft(abs(length(q - vec2<f32>(6.0, 24.0)) - 3.6) - 0.15, 0.1) * 0.5;
    // Start / select, et le cordon qui pend du haut.
    col += s.c * rect(q, vec2<f32>(-2.0, 30.5), vec2<f32>(1.0, 0.45)) * (0.5 + s.pulse);
    col += s.c * rect(q, vec2<f32>(2.0, 30.5), vec2<f32>(1.0, 0.45)) * (0.5 + s.pulse);
    col += s.a * soft(abs(q.x - 2.0 * sin(q.y * 0.2 - time)) - 0.3, 0.12) * step(46.0, q.y) * 0.6;
    col += vec3<f32>(2.0) * s.drop * shell * 0.15;

    return vec4<f32>(city_fog(col, in.world), 1.0);
}
