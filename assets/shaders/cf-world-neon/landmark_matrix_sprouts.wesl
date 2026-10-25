// Les glyphes qui jaillissent du mur : soixante-dix grands caractères nets, à quelques mètres de la façade, de la vraie
// géométrie. Un seul maillage fusionné (un plan par glyphe) ; toute l'animation est dans le vertex shader. Chaque plan porte
// ses données dans la couleur de ses sommets : (x, phase, profondeur, vitesse), tirées une fois à l'ouverture.

struct SproutIn {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(5) seed: vec4<f32>,
}

struct SproutOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world: vec3<f32>,
    @location(2) seed: vec4<f32>,
}

@vertex
fn vertex(v: SproutIn) -> SproutOut {
    let time = p.frame.x;
    let m = get_world_from_local(v.instance_index);
    let size = 1.3 + v.seed.z * 0.35;
    let y = gl_mod(v.seed.y * HEIGHT - time * 3.0 * v.seed.w * (1.0 + p.frame.z), HEIGHT);
    let centre = vec3<f32>((v.seed.x - 0.5) * p.b.x, y, v.seed.z + p.frame.y * 1.2);
    let local = centre + vec3<f32>(v.position.x * size, v.position.y * size * 1.4, 0.0);
    var wp = m * vec4<f32>(local, 1.0);
    wp.y *= city_grow(wp.xyz);
    var out: SproutOut;
    out.uv = v.uv;
    out.seed = v.seed;
    out.world = wp.xyz;
    out.clip = position_world_to_clip(wp.xyz);
    return out;
}

@fragment
fn fragment(in: SproutOut) -> @location(0) vec4<f32> {
    let time = p.frame.x;
    let pulse = p.frame.y;
    let gid = floor(h21(vec2<f32>(in.seed.x * 91.0, in.seed.y * 57.0)) * 40.0 + floor(time * (0.5 + 0.8 * in.seed.w)) * 3.0); // max 2.3 Hz
    let g = glyph(in.uv, gid, 0.0, p.b.y);
    let fall = fract(in.seed.y - time * 0.05 * in.seed.w);
    let col = flow_color(in.seed.x + time * 0.8, 1.0 + pulse * 1.2) * g * (0.6 + 0.5 * fall);
    return vec4<f32>(col * city_fade(in.world), 0.0);
}
