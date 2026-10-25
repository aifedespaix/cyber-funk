@vertex
fn vertex(v: PlaneIn) -> PlaneOut {
    return plane_vertex(v);
}

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let time = p.frame.x;
    let erupt = p.a.x;
    // uv.y = 1 contre la façade, 0 côté route.
    let away = 1.0 - in.uv.y;
    let reach = 0.35 + 0.45 * fbm(vec2<f32>(in.uv.x * 14.0 + time * 0.1, time * 0.2)) + p.frame.z * 0.2 + erupt * 0.3;
    let m = sstep(reach, reach - 0.25, away);
    let h = m * (0.5 + fbm(in.uv * vec2<f32>(30.0, 6.0) + time * 0.3) * 0.7) + p.frame.y * 0.2;
    return vec4<f32>(lava_ramp(h) * m * 1.1 * city_fade(in.world), 1.0);
}
