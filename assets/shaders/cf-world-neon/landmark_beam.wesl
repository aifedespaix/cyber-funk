// Faisceau de projection (`projection.ts`) : un volume additif très léger entre une source (le panneau, la lentille d'un
// projecteur) et la surface qu'il éclaire (la route, l'hologramme). Les lignes de balayage qui le parcourent donnent
// l'impression d'une image en cours d'émission.
//
// Paramètres : a.x = opacité du faisceau ; la teinte est `p.mood`. Maillage : pyramide ouverte, sommet à l'origine, base à
// z = 1 ; `position.z` est donc la position le long du faisceau.

struct BeamIn {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
}

struct BeamOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) t: f32,
    @location(1) world: vec3<f32>,
}

@vertex
fn vertex(v: BeamIn) -> BeamOut {
    let m = get_world_from_local(v.instance_index);
    let wp = m * vec4<f32>(v.position, 1.0);
    var out: BeamOut;
    out.t = v.position.z;
    out.world = wp.xyz;
    out.clip = position_world_to_clip(wp.xyz);
    return out;
}

@fragment
fn fragment(in: BeamOut) -> @location(0) vec4<f32> {
    let scan = 0.55 + 0.45 * sin(in.t * 90.0 - p.frame.x * 14.0);
    let a = p.a.x * (0.04 + 0.06 * scan) * (1.0 - in.t * 0.35);
    return vec4<f32>(p.mood.rgb * a * city_fade(in.world) * (1.0 + p.frame.y), 0.0);
}
