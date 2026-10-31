// Fond de l'espace de Neon City : noir profond, nébuleuses diffuses et bande de Voie lactée.
//
// Portage du fragment shader de `createDome` (`engine/sky-materials.ts` de Sillage), mêmes constantes.
//
// Repère : l'entité reste à l'origine du repère du ciel (c'est la caméra de fond qui se déplace avec `skyDrift`), la
// position monde du fragment est donc la direction du ciel à l'échelle près : `vDir = normalize(position)` de Sillage.

import bevy_pbr::render::forward_io::VertexOutput;

// Même ordre de champs que `DomeParams` (Rust, `ShaderType`).
struct Dome {
    ink: vec4<f32>,
    violet: vec4<f32>,
    azure: vec4<f32>,
    rose: vec4<f32>,
    // xyz : axe de la Voie lactée.
    axis: vec4<f32>,
    // x : temps (s), y : pouls, z : fondu du ciel (`uReveal`).
    frame: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> dome: Dome;

// `smoothstep(e0, e1, x)` de GLSL accepte e0 > e1 ; WGSL l'interdit. 1 - smoothstep(e1, e0, x) donne la même courbe.
fn sstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge0 < edge1 {
        return smoothstep(edge0, edge1, x);
    }
    return 1.0 - smoothstep(edge1, edge0, x);
}

fn h3(p_in: vec3<f32>) -> f32 {
    var p = fract(p_in * 0.3183099 + 0.1);
    p *= 17.0;
    return fract(p.x * p.y * p.z * (p.x + p.y + p.z));
}

fn vnoise(x: vec3<f32>) -> f32 {
    let i = floor(x);
    var f = fract(x);
    f = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(
            mix(h3(i), h3(i + vec3<f32>(1.0, 0.0, 0.0)), f.x),
            mix(h3(i + vec3<f32>(0.0, 1.0, 0.0)), h3(i + vec3<f32>(1.0, 1.0, 0.0)), f.x),
            f.y,
        ),
        mix(
            mix(h3(i + vec3<f32>(0.0, 0.0, 1.0)), h3(i + vec3<f32>(1.0, 0.0, 1.0)), f.x),
            mix(h3(i + vec3<f32>(0.0, 1.0, 1.0)), h3(i + vec3<f32>(1.0, 1.0, 1.0)), f.x),
            f.y,
        ),
        f.z,
    );
}

fn fbm3(p_in: vec3<f32>) -> f32 {
    var p = p_in;
    var v = 0.0;
    var a = 0.5;
    for (var i = 0; i < 5; i++) {
        v += a * vnoise(p);
        p = p * 2.03 + 11.7;
        a *= 0.5;
    }
    return v;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let time = dome.frame.x;
    let pulse = dome.frame.y;
    let reveal = dome.frame.z;
    let ink = dome.ink.rgb;
    let violet = dome.violet.rgb;
    let azure = dome.azure.rgb;
    let rose = dome.rose.rgb;

    let d = normalize(in.world_position.xyz);
    let h = d.y;
    // Noir très légèrement bleuté ; le bas du ciel reste sombre, l'horizon un peu plus chaud.
    var col = ink * 0.55;
    col += violet * 0.05 * sstep(0.1, -0.5, h);
    col += mix(violet, rose, 0.4) * 0.1 * sstep(0.18, 0.0, abs(h));

    // Nébuleuses : deux couches de bruit à grande échelle, teintées différemment.
    let cloud_a = fbm3(d * 2.2 + vec3<f32>(0.0, time * 0.004, 0.0));
    let cloud_b = fbm3(d * 4.5 + 7.3);
    col += violet * smoothstep(0.5, 0.85, cloud_a) * 0.16;
    col += azure * smoothstep(0.55, 0.9, cloud_b) * 0.1;
    col += rose * smoothstep(0.62, 0.95, fbm3(d * 3.0 - 3.1)) * 0.07;

    // Voie lactée : bande dense, striée de poussière sombre. (`pow(x, 2.0)` de GLSL, écrit x * x : le `pow` de WGSL
    // n'est pas défini pour une base négative.)
    let across = dot(d, dome.axis.xyz) * 5.5;
    let band = exp(-(across * across));
    let dust = smoothstep(0.35, 0.75, fbm3(d * 9.0));
    col += mix(azure, violet, 0.5 + 0.5 * cloud_a) * band * (0.2 - 0.12 * dust) * (1.0 + pulse * 0.25);

    // Soleil synthwave : un grand disque bas sur l'horizon, droit devant (-z), rayé de bandes qui s'épaississent vers le
    // bas. Discret (la ville le cache en grande partie) : il donne un point focal chaud à la route et respire avec le kick.
    let sun_dir = normalize(vec3<f32>(0.0, 0.07, -1.0));
    let sun_radius = 0.2;
    let to_sun = d - sun_dir;
    // Coordonnées dans le disque : x latéral, y vertical (rayon d'arc en radians, assez petit pour être plan).
    let sx = to_sun.x;
    let sy = to_sun.y;
    let disc = 1.0 - smoothstep(sun_radius * 0.96, sun_radius, sqrt(sx * sx + sy * sy));
    // Bandes : absentes en haut, de plus en plus larges vers le bas (les derniers étages sont presque coupés).
    let down = clamp((sun_radius - sy) / (2.0 * sun_radius), 0.0, 1.0);
    let stripe = smoothstep(0.0, 0.08, abs(fract(sy * 28.0 / sun_radius * 0.25) - 0.5) * 2.0 - down * 0.9);
    let sun_mask = disc * mix(1.0, stripe, smoothstep(0.35, 0.55, down));
    let sun_col = mix(rose, violet, down) * 1.6 + azure * 0.15;
    col += sun_col * sun_mask * (0.55 + pulse * 0.2) * step(0.0, -d.z);
    // Halo diffus autour du disque.
    col += mix(rose, violet, 0.5) * exp(-dot(to_sun, to_sun) / (sun_radius * sun_radius * 2.5)) * 0.08;
    return vec4<f32>(col * reveal, reveal);
}
