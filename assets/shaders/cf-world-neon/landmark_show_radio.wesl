// La tour radio (`showcase/radio.ts`) : un treillis qui s'effile jusqu'à une antenne, dix balises qui s'allument de bas en haut
// à chaque kick, un feu de pointe. Les ondes sont une pièce à part (`landmark_radio_waves.wesl`).

const SPAN: f32 = 24.0;
const HEIGHT: f32 = 118.0;
const TOP: f32 = 102.0;

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let s = show();
    let q = vec2<f32>((in.uv.x - 0.5) * SPAN, in.uv.y * HEIGHT);
    var col = s.ink * 0.85;

    let y01 = q.y / HEIGHT;
    let hw0 = mix(SPAN / 2.0 - 1.0, 0.9, pow(y01, 0.8));
    // Treillis : deux montants, des diagonales par panneaux de 11 m, des entretoises.
    let panel = 11.0;
    let seg = floor(q.y / panel);
    let f = fract(q.y / panel);
    let legs = soft(abs(abs(q.x) - hw0) - 0.35, 0.1);
    let pa = vec2<f32>(-hw0, seg * panel);
    let pb = vec2<f32>(hw0, (seg + 1.0) * panel);
    let pc = vec2<f32>(-hw0, (seg + 1.0) * panel);
    let pd = vec2<f32>(hw0, seg * panel);
    let diag = soft(min(sd_seg(q, pa, pb), sd_seg(q, pc, pd)) - 0.22, 0.08) * step(abs(q.x), hw0 + 0.3);
    let rung = soft(abs(f) * panel - 0.2, 0.1) * step(abs(q.x), hw0);
    let steel = max(legs, max(diag * 0.8, rung * 0.7));
    col = mix(col, s.a * 0.18 * (0.5 + 0.5 * cos(q.x * 1.2)), steel);
    // Antenne.
    let mast = soft(abs(q.x) - 0.35, 0.1) * step(TOP, q.y) * step(q.y, HEIGHT - 2.0);
    col = mix(col, s.a * 0.5, mast);

    // Balises : une par panneau, allumées l'une après l'autre en montant (un cran à chaque kick).
    let climb = gl_mod(floor(s.beat), 10.0);
    for (var i = 0; i < 10; i++) {
        let fi = f32(i);
        let y = (fi + 0.5) * (HEIGHT / 10.0);
        let hw = mix(SPAN / 2.0 - 1.0, 0.9, pow(y / HEIGHT, 0.8));
        let on = exp(-abs(fi - climb) * 1.1) + s.pulse * 0.3;
        let spot = soft(length(q - vec2<f32>(-hw, y)) - 0.7, 0.2) + soft(length(q - vec2<f32>(hw, y)) - 0.7, 0.2);
        col += s.c * spot * (0.25 + on * 3.0);
    }
    // Pointe : un feu fixe qui pulse avec la mélodie.
    col += s.b * soft(length(q - vec2<f32>(0.0, HEIGHT - 1.5)) - 1.0, 0.3) * (1.0 + s.mid * 3.0 + s.pulse * 2.0);
    col += s.d * exp(-length(vec2<f32>(q.x, q.y - (HEIGHT - 1.5))) / 9.0) * 0.4;

    return vec4<f32>(city_fog(col, in.world), 1.0);
}
