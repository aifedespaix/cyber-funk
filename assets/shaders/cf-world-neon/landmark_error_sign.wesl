// Le panneau géant ERROR (`error-building.ts`) : sept couches de la même image (l'atlas du mot), empilées, elles font une
// lettre extrudée dont la profondeur se lit en parallaxe. Un seul maillage fusionné (sept plans), la couche de chaque
// sommet dans `uv1.x` (0 derrière, 1 devant).
//
// Paramètres : a.x = déchirure (0..1, **déjà bornée** par le limiteur de flashs), b = le mot (rangée de l'atlas, début, fin,
// nombre de lettres). Texture : l'atlas (`map`).
//
// Écart avec Sillage (règle 7) : le cadre néon se coupait en stroboscope à 17 Hz quand le panneau déraillait ; il est
// ici simplement atténué, sans clignoter.

const SIGN_W: f32 = 60.0;
const SIGN_H: f32 = 15.0;

struct SignIn {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) layer: vec2<f32>,
}

struct SignOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world: vec3<f32>,
    @location(2) layer: f32,
}

@vertex
fn vertex(v: SignIn) -> SignOut {
    let m = get_world_from_local(v.instance_index);
    // Pas de croissance du sol : l'enseigne est un phare, visible dès l'horizon.
    let wp = m * vec4<f32>(v.position, 1.0);
    var out: SignOut;
    out.uv = v.uv;
    out.layer = v.layer.x;
    out.world = wp.xyz;
    out.clip = position_world_to_clip(wp.xyz);
    return out;
}

@fragment
fn fragment(in: SignOut) -> @location(0) vec4<f32> {
    let time = p.frame.x;
    let pulse = p.frame.y;
    let g = p.a.x;
    let front = 1.0 - in.layer;
    let alarm = color(C_ALARM);
    let m = glyph_word(map, map_sampler, in.uv, p.b, g, time);
    var col = alarm * (m.g * (0.9 + front * 2.2) + m.r * 1.1);
    col += color(C_ICE) * m.g * front * front * (0.4 + pulse);
    col += color(C_MINT) * m.b * g * 1.1;
    // Cadre néon : il s'atténue quand le panneau déraille.
    let d = min(in.uv, 1.0 - in.uv) * vec2<f32>(SIGN_W, SIGN_H);
    let frame = sstep(0.32, 0.18, min(d.x, d.y));
    col += alarm * frame * (0.5 + front) * (0.8 + pulse * 1.2) * (1.0 - g * 0.3);
    // Presque pas de brouillard : on doit lire le mot de très loin.
    let far = 1.0 - 0.35 * smoothstep(250.0, 700.0, distance(in.world, view.world_position));
    return vec4<f32>(col * far, 1.0);
}
