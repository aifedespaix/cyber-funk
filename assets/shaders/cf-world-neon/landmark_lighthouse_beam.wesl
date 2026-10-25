// Le faisceau du phare (`showcase/lighthouse.ts`, `build`) : deux plans en croix de 150 × 15 m, additifs, qui tournent autour
// de la lanterne. uv.x = distance le long du faisceau (0 à la lampe), uv.y = travers.
//
// Paramètres : a.xyz = couleur (le soleil), a.w = puissance (`uPower`).

@vertex
fn vertex(v: PlaneIn) -> PlaneOut {
    return plane_vertex(v);
}

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let along = in.uv.x;
    let across = abs(in.uv.y - 0.5) * 2.0;
    let spread = mix(0.12, 1.0, along);
    let core = exp(-pow(across / spread, 2.0) * 3.0);
    let a = core * (1.0 - along) * (1.0 - along) * p.a.w;
    // Additif de Sillage : rgb × alpha, avec alpha = a.
    return vec4<f32>(p.a.xyz * a * 1.4 * a * city_fade(in.world), 0.0);
}
