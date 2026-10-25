// Les ondes de la tour radio (`showcase/radio.ts`, `build`) : un plan additif de 90 × 90 m à la pointe de l'antenne, de pure
// lumière (la voiture passe à travers sans les voir bouger).
//
// Paramètres : a.xyz = teinte (glace vers rose avec l'intensité), a.w = onde de choc du drop (`uBlast`, 0..1, niveau
// borné par le limiteur de flashs : règle 7).

const WAVES: f32 = 90.0;

@vertex
fn vertex(v: PlaneIn) -> PlaneOut {
    return plane_vertex(v);
}

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let mid = p.frame.w;
    let beat = p.audio.y;
    let phase = p.audio.w;
    let blast = p.a.w;
    let q = (in.uv - 0.5) * WAVES;
    let r = length(q);
    // Ondes qui s'élargissent ; la phase suit le tempo (une onde par kick).
    let t = fract(r / 18.0 - (beat + phase) * 0.25);
    let ring = pow(1.0 - t, 6.0) * smoothstep(0.0, 0.08, t);
    let shock_d = (r - (1.0 - blast) * (WAVES / 2.0)) / 2.0;
    let shock = exp(-shock_d * shock_d) * blast;
    let fade = 1.0 - smoothstep(WAVES * 0.2, WAVES / 2.0, r);
    let a = (ring * (0.3 + mid * 0.8) + shock * 1.4) * fade;
    return vec4<f32>(p.a.xyz * a * city_fade(in.world), 0.0);
}
