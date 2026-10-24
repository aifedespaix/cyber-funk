// Rideau d'aurore boréale de Neon City : un plan vertical très lointain, en bruit animé.
//
// Portage de `createAurora` (`engine/sky-materials.ts`). Le plan de Bevy a son origine d'UV en haut à gauche, celui de
// three.js en bas à gauche : `vUv.y` de Sillage s'écrit ici `1 - uv.y`.

import bevy_pbr::render::forward_io::VertexOutput;

// Même ordre de champs que `AuroraParams` (Rust, `ShaderType`).
struct Aurora {
    a: vec4<f32>,
    b: vec4<f32>,
    c: vec4<f32>,
    // x : temps (s), y : pouls, z : graine (`uSeed`), w : mouvement (`uMotion`).
    frame: vec4<f32>,
    // x : fondu du ciel (`uReveal`).
    look: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> aurora: Aurora;

fn hash1(n: f32) -> f32 {
    return fract(sin(n) * 43758.5453);
}

fn noise1(x: f32) -> f32 {
    let i = floor(x);
    let f = fract(x);
    return mix(hash1(i), hash1(i + 1.0), f * f * (3.0 - 2.0 * f));
}

fn fbm1(x_in: f32) -> f32 {
    var x = x_in;
    var v = 0.0;
    var amp = 0.5;
    for (var i = 0; i < 4; i++) {
        v += amp * noise1(x);
        x *= 2.1;
        amp *= 0.5;
    }
    return v;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let time = aurora.frame.x;
    let pulse = aurora.frame.y;
    let seed = aurora.frame.z;
    let motion = aurora.frame.w;
    let reveal = aurora.look.x;
    let uv = vec2<f32>(in.uv.x, 1.0 - in.uv.y);

    let t = time * 0.1 * motion + seed;
    // Le bas du rideau est net et lumineux, le haut se dissout en rayons verticaux.
    let wave = fbm1(uv.x * 6.0 + t) * 0.8 + fbm1(uv.x * 2.0 - t * 0.6) * 0.5;
    let base = 0.12 + wave * 0.28;
    let rise = uv.y - base;
    let curtain = smoothstep(-0.02, 0.03, rise) * exp(-max(rise, 0.0) * 4.2);
    var rays = 0.5 + 0.5 * sin(uv.x * 140.0 + wave * 18.0 + t * 3.0);
    rays = mix(0.55, rays, smoothstep(0.0, 0.5, rise));
    // `smoothstep(1.0, 0.8, x)` de GLSL (bord décroissant), écrit 1 - smoothstep(0.8, 1.0, x).
    let fade_x = smoothstep(0.0, 0.2, uv.x) * (1.0 - smoothstep(0.8, 1.0, uv.x));
    var tint = mix(aurora.a.rgb, aurora.b.rgb, smoothstep(0.2, 0.7, wave));
    tint = mix(tint, aurora.c.rgb, smoothstep(0.1, 0.6, rise));
    let intensity = curtain * rays * fade_x * (0.2 + pulse * 0.22);
    return vec4<f32>(tint * intensity * reveal, intensity * reveal);
}
