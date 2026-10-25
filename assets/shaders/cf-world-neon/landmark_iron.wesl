// Un aplat sans éclairage : les mâts et la traverse du panneau ERROR (`MeshBasicMaterial({ color: palette.ink })`).

@vertex
fn vertex(v: PlaneIn) -> PlaneOut {
    return plane_vertex(v);
}

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    return vec4<f32>(color(C_INK), 1.0);
}
