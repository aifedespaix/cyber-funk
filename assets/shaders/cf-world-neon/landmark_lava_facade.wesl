@vertex
fn vertex(v: PlaneIn) -> PlaneOut {
    return plane_vertex(v);
}

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let time = p.frame.x;
    let pulse = p.frame.y;
    let bass = p.frame.z;
    let erupt = p.a.x;
    let scroll = p.b.x;
    let q = vec2<f32>(in.uv.x * SPAN, in.uv.y * HEIGHT);
    let cell = vec2<f32>(1.0, 1.5);
    let column = floor(q.x / cell.x);
    let cs = h11(column * 3.7);
    // Le contenu descend : à une hauteur fixe, on lit des rangées de plus en plus hautes.
    let fall = q.y + scroll * (0.6 + cs * 0.9);

    let np = vec2<f32>(q.x * 0.18, (q.y + scroll * 0.7) * 0.07);
    let n = fbm(np + vec2<f32>(fbm(np * 2.0 + time * 0.05), 0.0) * 1.5);
    let ridge = 1.0 - abs(2.0 * fbm(np * 1.7 + 4.0) - 1.0);
    var heat = n * 1.1 + pow(ridge, 6.0) * 0.55 + pulse * 0.2 + bass * 0.25;
    // Éruption : un front de chaleur monte de bas en haut en 1,6 s.
    let front = (1.0 - erupt) * HEIGHT * 1.2;
    let fd = (q.y - front) / 10.0;
    heat += exp(-fd * fd) * erupt * 0.9;
    heat += exp(-q.y / 6.0) * 0.5;

    let id = floor(fall / cell.y);
    let bit = step(0.5, h21(vec2<f32>(column, id + floor(time * (1.0 + cs)))));
    let d = digit(vec2<f32>(fract(q.x / cell.x), fract(fall / cell.y)), bit);
    let k = fract(id / (14.0 + cs * 20.0) + cs * 7.0);
    let streak = pow(1.0 - k, 2.0);
    let hot = clamp(heat * 0.9 + streak * 0.5, 0.0, 1.4);

    var col = lava_ramp(heat * 0.62) * 0.7;
    col += lava_ramp(hot + 0.35) * d * (0.8 + streak * 1.6);
    return vec4<f32>(city_fog(col, in.world), 1.0);
}
