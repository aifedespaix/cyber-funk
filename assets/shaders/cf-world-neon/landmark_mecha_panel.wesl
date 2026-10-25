// Un os de robot (`mecha/panel.ts`) : même langage que les gratte-ciel (fenêtres allumées par les aigus, arêtes néon), mais
// calculé sur un boîtier isolé dont les coordonnées locales sont centrées. Les arêtes se repèrent à la seconde plus petite
// distance au bord ; la teinte vient de l'humeur du robot (`p.mood`).
//
// Le maillage est un cube unité mis à l'échelle par la pose de la pièce ; les dimensions de la géométrie (sans l'échelle du
// robot, comme `uHalf` de Sillage) arrivent en paramètre : a.xyz = demi-dimensions (m), a.w = graine du boîtier.
// c.x = aigus et c.y = basses, **lissés** côté CPU : les fenêtres ne scintillent plus au rythme brut de l'analyse (règle 7).

struct PanelOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) world: vec3<f32>,
}

@vertex
fn vertex(v: PlaneIn) -> PanelOut {
    let m = get_world_from_local(v.instance_index);
    var wp = m * vec4<f32>(v.position, 1.0);
    wp.y *= city_grow(wp.xyz);
    var out: PanelOut;
    // Cube unité centré : `position` va de -0.5 à 0.5, la géométrie de Sillage de -demi-taille à +demi-taille.
    out.local = v.position * (p.a.xyz * 2.0);
    out.normal = v.normal;
    out.world = wp.xyz;
    out.clip = position_world_to_clip(wp.xyz);
    return out;
}

@fragment
fn fragment(in: PanelOut) -> @location(0) vec4<f32> {
    let pulse = p.frame.y;
    let high = p.c.x;
    let bass = p.c.y;
    let half_size = p.a.xyz;
    let seed = p.a.w;
    let n = normalize(in.normal);
    let an = abs(n);
    var face = in.local.xz;
    if an.z > 0.5 {
        face = in.local.xy;
    } else if an.x > 0.5 {
        face = in.local.zy;
    }
    let q = half_size - abs(in.local);
    let nearest = min(min(q.x, q.y), q.z);
    let farthest = max(max(q.x, q.y), q.z);
    let edge = sstep(0.55, 0.0, q.x + q.y + q.z - nearest - farthest);

    let wc = face / vec2<f32>(1.7, 2.7) + vec2<f32>(seed * 3.7, seed);
    let cell = floor(wc);
    let wf = fract(wc);
    let lit = step(h21(cell + seed), 0.26 + high * 0.3);
    let pane = step(0.18, wf.x) * step(wf.x, 0.82) * step(0.2, wf.y) * step(wf.y, 0.8);
    let side = 1.0 - step(0.5, an.y);

    let lambert = 0.5 + 0.5 * dot(n, normalize(vec3<f32>(-0.3, 0.7, 0.6)));
    let ice = color(C_ICE);
    let mood = p.mood.rgb;
    var col = color(C_INK) * 0.9 + ice * (0.012 + 0.03 * lambert);
    col += mix(mood, ice, step(0.9, h21(cell + 5.0))) * lit * pane * side * (0.5 + 0.8 * pulse);
    col += mood * edge * (0.9 + pulse * 1.6 + bass * 0.6);
    return vec4<f32>(city_fog(col, in.world), 1.0);
}
