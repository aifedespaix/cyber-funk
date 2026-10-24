// Étoile filante de Neon City : un trait à tête brillante.
//
// Portage du matériau de `engine/shooting-stars.ts`. Le quad est tourné vers le centre du ciel, son axe X suit la
// direction du mouvement : `uv.x` vaut 1 à la tête et 0 au bout de la queue, `uv.y` est centré sur l'axe du trait.

import bevy_pbr::render::forward_io::VertexOutput;

// Même ordre de champs que `StreakParams` (Rust, `ShaderType`).
struct Streak {
    color: vec4<f32>,
    // x : opacité (`uAlpha`), y : fondu du ciel (`uReveal`).
    frame: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> streak: Streak;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = streak.frame.x;
    let reveal = streak.frame.y;
    let tail = pow(in.uv.x, 3.0);
    // `smoothstep(0.5, 0.0, d)` de GLSL (bord décroissant), écrit 1 - smoothstep(0.0, 0.5, d).
    let width = (1.0 - smoothstep(0.0, 0.5, abs(in.uv.y - 0.5))) * mix(0.4, 1.0, in.uv.x);
    let head = smoothstep(0.93, 1.0, in.uv.x) * 1.6;
    let a = (tail + head) * width * alpha;
    return vec4<f32>(streak.color.rgb * a * 2.2 * reveal, a * reveal);
}
