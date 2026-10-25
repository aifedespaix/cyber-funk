// La tour horloge (`showcase/clock.ts`) : un cadran monumental au sommet, une mécanique de rouages au pied.
//
// Paramètres : b.w = rotation des rouages, c.w = horloge des aiguilles. Toutes deux sont intégrées côté CPU : les produits
// `time × (0,25 + intensité × 0,5)` et `time × 0,24 × (1 + drop × 6)` de Sillage téléportaient les aiguilles à chaque
// variation de l'intensité et au drop. Les aiguilles s'emballent toujours au drop, mais sans saut.

const SPAN: f32 = 34.0;
const HEIGHT: f32 = 96.0;
const DIAL_Y: f32 = 66.0;
const DIAL_R: f32 = 13.5;

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let s = show();
    let time = s.time;
    let q = vec2<f32>((in.uv.x - 0.5) * SPAN, in.uv.y * HEIGHT);
    var col = s.ink * 0.85;

    // Pierre de la tour, et cadran encastré.
    col += s.a * 0.012 * (0.5 + 0.5 * sin(q.y * 1.3));
    let dp = q - vec2<f32>(0.0, DIAL_Y);
    let r = length(dp);
    let face = step(r, DIAL_R);
    let rim = soft(abs(r - DIAL_R) - 0.6, 0.15);
    col = mix(col, s.ink * 1.8 + s.b * 0.05, face);
    col += s.a * rim * (0.9 + s.pulse * 0.8);
    let ang = atan2(dp.x, dp.y);
    let marks = soft(abs(fract(ang / 0.5236 + 0.5) - 0.5) * r * 0.5236 - 0.22, 0.08) * step(DIAL_R - 2.4, r) * step(r, DIAL_R - 0.9);
    col += s.b * marks * face * 1.3;

    // Aiguilles : la grande tourne avec le temps, la trotteuse saute d'un cran à chaque kick.
    let hour_a = s.clock_c * 0.02;
    let min_a = s.clock_c * 0.24;
    let sec_a = (floor(s.beat) + smoothstep(0.0, 0.18, s.phase)) * 0.2618;
    var hand = 0.0;
    hand += soft(sd_seg(dp, vec2<f32>(0.0), vec2<f32>(sin(hour_a), cos(hour_a)) * (DIAL_R * 0.5)) - 0.5, 0.1);
    hand += soft(sd_seg(dp, vec2<f32>(0.0), vec2<f32>(sin(min_a), cos(min_a)) * (DIAL_R * 0.78)) - 0.32, 0.1);
    col += s.b * hand * face * 1.4;
    col += s.d * soft(sd_seg(dp, vec2<f32>(0.0), vec2<f32>(sin(sec_a), cos(sec_a)) * (DIAL_R * 0.9)) - 0.14, 0.08) * face * 1.8;
    col += s.a * soft(r - 0.9, 0.1) * face * 1.2;

    // Rouages : deux grandes roues dentées en prise, une petite entre les deux. Les dents s'emboîtent (sens opposés).
    let turn = s.clock_b + s.drop * 2.0;
    for (var g = 0; g < 3; g++) {
        var center = vec2<f32>(-7.0, 30.0);
        var radius = 8.0;
        var teeth = 16.0;
        var dir = 1.0;
        var speed = 1.0;
        var flip = 1.0;
        if g == 1 {
            center = vec2<f32>(8.0, 38.0);
            radius = 7.0;
            teeth = 14.0;
            dir = -1.0;
            speed = 16.0 / 14.0;
        } else if g == 2 {
            center = vec2<f32>(-3.0, 46.0);
            radius = 4.6;
            teeth = 9.0;
            speed = 16.0 / 9.0;
            flip = -1.0;
        }
        let gp = rot(q - center, turn * speed * dir * flip);
        let ga = atan2(gp.y, gp.x);
        let gr = length(gp);
        let tooth = 0.9 * step(0.5, fract(ga / 6.2832 * teeth));
        let body = step(gr, radius + tooth * 0.6 - 0.4) * step(radius * 0.35, gr);
        let spoke = step(0.5, abs(sin(ga * 3.0))) * step(gr, radius * 0.8);
        let ring = step(radius * 0.8, gr);
        let mask = body * max(ring, spoke + step(gr, radius * 0.45));
        let brass = mix(s.a, s.b, 0.5 + 0.5 * sin(ga * 2.0 + time)) * (0.25 + 0.2 * sin(gr * 1.7));
        col = mix(col, brass * (1.0 + s.pulse * 0.6), mask);
        col += s.b * soft(abs(gr - radius * 0.35) - 0.12, 0.08) * body * 0.8;
    }

    // Balancier : une tige et un disque qui oscillent d'un côté à l'autre à chaque kick.
    let swing = cos(3.14159 * (s.beat + s.phase)) * 0.45;
    let pivot = vec2<f32>(0.0, 24.0);
    let bob = pivot + vec2<f32>(sin(swing), -cos(swing)) * 17.0;
    col += s.a * soft(sd_seg(q, pivot, bob) - 0.2, 0.08) * 0.9;
    col += s.b * soft(length(q - bob) - 2.2, 0.15) * (1.0 + s.pulse);
    // Le sol de la mécanique : une lueur chaude.
    col += s.a * exp(-q.y / 5.0) * 0.2;

    return vec4<f32>(city_fog(col, in.world), 1.0);
}
