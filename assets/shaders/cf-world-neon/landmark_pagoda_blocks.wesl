// Les blocs de la pagode (`showcase/pagoda.ts`, `BlockSet`) : cinq étages, cinq toits à débord, cinq dalles de faîtage et la
// flèche, en un seul maillage. Sillage les fait passer par le matériau des gratte-ciel (fenêtres, brouillard) ; ici une
// version légère du même esprit : un volume sombre, des fenêtres rose qui s'allument par endroits, le brouillard de la ville.
//
// Paramètres : a.xyz = teinte des fenêtres (rose).

@vertex
fn vertex(v: PlaneIn) -> PlaneOut {
    return plane_vertex(v);
}

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let ink = color(C_INK);
    let side = step(abs(in.normal.y), 0.5);
    // Le long de la face : x pour les faces ±Z, z pour les faces ±X.
    let along = select(in.local.z, in.local.x, abs(in.normal.z) > 0.5);
    let cell = vec2<f32>(floor(along / 2.2), floor(in.local.y / 2.6));
    let inner = abs(fract(vec2<f32>(along / 2.2, in.local.y / 2.6)) - 0.5);
    let pane = step(max(inner.x, inner.y), 0.32);
    let lit = step(0.55, h21(cell + 17.0));
    var col = ink * 0.9 + p.a.xyz * 0.01;
    col += p.a.xyz * 0.35 * lit * pane * side * (1.0 + 0.4 * p.frame.y);
    return vec4<f32>(city_fog(col, in.world), 1.0);
}
