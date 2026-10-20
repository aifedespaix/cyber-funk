// Passe d'étalonnage finale : aberration, gouttes sur l'objectif, vignette, grain, et les effets du spectacle.
//
// Portage de `engine/postfx-grade.ts` de Sillage (GLSL → WGSL). Elle travaille en lumière linéaire HDR, après le
// bloom et avant le tonemapping, exactement comme le `ShaderPass` de Sillage entre l'`UnrealBloomPass` et
// l'`OutputPass`. Les niveaux de flash et de vague (`flash`, `sweep`) viennent du limiteur photosensible de cf-show :
// ce shader ne fait que les appliquer.
//
// Repère : Bevy donne des UV avec y vers le bas, three.js avec y vers le haut. Tous les calculs se font en UV « à la
// three.js » (`v_uv`, y vers le haut) pour garder le sens des gouttes (elles glissent vers le bas) ; seule la lecture
// de texture retourne y (`screen`).

struct FullscreenVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Même ordre de champs que `GradeUniforms` (Rust, `ShaderType`) : la disposition mémoire en découle.
struct Grade {
    aberration: f32,
    time: f32,
    grain: f32,
    lens: f32,
    rain_time: f32,
    aspect: f32,
    flash: f32,
    sweep: f32,
    sweep_pos: f32,
    contrast: f32,
    saturation: f32,
    slow: f32,
    flash_color: vec3<f32>,
    blur: f32,
    sweep_color: vec3<f32>,
};

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> grade: Grade;

const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

// Lecture de l'image source en UV three.js (y vers le haut). `textureSampleLevel` : pas d'exigence de flot uniforme,
// la lecture reste permise dans les branches.
fn screen(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(screen_texture, screen_sampler, vec2<f32>(uv.x, 1.0 - uv.y), 0.0).rgb;
}

fn hh(p: vec2<f32>, k: f32) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7)) + k * 91.7) * 43758.5453);
}

// Une couche de gouttes sur l'objectif : une grille de cellules, une goutte au plus par cellule, qui glisse
// par à-coups du haut vers le bas de sa case en laissant un filet. Renvoie xy = décalage de réfraction,
// z = opacité de la goutte, w = reflet.
fn drop_layer(uv: vec2<f32>, scale: f32, amount: f32, seed: f32, trail_amount: f32) -> vec4<f32> {
    let p = vec2<f32>(uv.x * grade.aspect, uv.y) * scale;
    let id = floor(p);
    let f = fract(p) - 0.5;
    let h0 = hh(id, seed);
    if (h0 > amount) {
        return vec4<f32>(0.0);
    }
    let h1 = hh(id, seed + 1.0);
    let h2 = hh(id, seed + 2.0);
    let h3 = hh(id, seed + 3.0);
    let s = fract(grade.rain_time * (0.05 + 0.1 * h1) + h2);
    let ease = s * s * (3.0 - 2.0 * s);
    let c = vec2<f32>((h1 - 0.5) * 0.5, mix(0.32, -0.32, ease));
    let fade = smoothstep(0.0, 0.08, s) * (1.0 - smoothstep(0.9, 1.0, s));
    let r = 0.08 + h3 * 0.08;
    let d = (f - c) * vec2<f32>(1.0, 0.85);
    let body = (1.0 - smoothstep(r * 0.55, r, length(d))) * fade;
    // Le filet laissé derrière la goutte : fin, et de plus en plus pâle vers le haut.
    let t = clamp((f.y - c.y) / (0.32 - c.y + 0.001), 0.0, 1.0);
    let trail = trail_amount * step(c.y, f.y) * (1.0 - smoothstep(0.0, 0.035, abs(f.x - c.x))) * (1.0 - t) * 0.5 * fade;
    let refraction = -d / r * body * 0.35 / scale;
    let glint = body * (1.0 - smoothstep(0.0, 0.5, length(d / r - vec2<f32>(-0.3, 0.4))));
    return vec4<f32>(refraction + vec2<f32>(0.0, -trail * 0.01), max(body, trail * 0.6), glint);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let v_uv = vec2<f32>(in.uv.x, 1.0 - in.uv.y);
    var uv = v_uv;
    var wet = 0.0;
    var glint = 0.0;
    // Branches sur des uniformes : toute l'image prend le même chemin, elles ne coûtent rien quand elles sont fermées.
    if (grade.lens > 0.001) {
        let big = drop_layer(uv, 5.0, 0.2 * grade.lens, 1.0, 1.0);
        let small = drop_layer(uv + 0.37, 15.0, 0.22 * grade.lens, 7.0, 0.0);
        uv += big.xy + small.xy;
        wet = max(big.z, small.z * 0.8);
        glint = big.w + small.w * 0.6;
    }
    let c = v_uv - 0.5;
    // Aberration chromatique : elle s'accentue vers les bords et sur les kicks
    let shift = c * grade.aberration * (0.4 + dot(c, c) * 2.0);
    let r = screen(uv + shift).r;
    let g = screen(uv).g;
    let b = screen(uv - shift).b;
    var col = vec3<f32>(r, g, b);
    // Bullet-time : flou radial léger (quelques prises vers le centre), plus fort vers les bords.
    if (grade.blur > 0.001) {
        var acc = col;
        for (var i = 1; i <= 4; i++) {
            acc += screen(uv - c * (f32(i) * 0.014 * grade.blur));
        }
        col = acc / 5.0;
    }
    // Étalonnage du niveau : saturation et contraste ≥ 1, pivot bas pour ne pas boucher les noirs.
    let luma = dot(col, LUMA);
    col = max(vec3<f32>(0.0), mix(vec3<f32>(luma), col, grade.saturation));
    col = max(vec3<f32>(0.0), 0.1 + (col - 0.1) * grade.contrast);
    // Goutte : un peu plus sombre en son centre, un reflet clair sur le haut.
    col = mix(col, col * 0.82, wet) + glint * 0.22;
    // Vignette et lignes de balayage très discrètes
    let vignette = smoothstep(0.95, 0.25, length(c) * 1.25);
    col *= mix(0.5, 1.0, vignette);
    col *= 0.97 + 0.03 * sin(v_uv.y * 1100.0);
    if (grade.slow > 0.001) {
        col = mix(col, vec3<f32>(dot(col, LUMA)), 0.35 * grade.slow);
        col *= 1.0 - 0.3 * grade.slow * smoothstep(0.25, 0.85, length(c) * 1.4);
    }
    // Vague de lumière d'un changement de niveau, puis flash : leurs niveaux viennent du limiteur photosensible.
    // `pow(x, 2.0)` de GLSL devient `x * x` : WGSL (comme GLSL) ne définit pas `pow` pour une base négative.
    let across = (v_uv.x - grade.sweep_pos) / 0.18;
    let band = exp(-(across * across));
    col += grade.sweep_color * (grade.sweep * band);
    col += grade.flash_color * grade.flash;
    let grain = fract(sin(dot(v_uv + fract(grade.time), vec2<f32>(12.9898, 78.233))) * 43758.5453);
    col += (grain - 0.5) * grade.grain;
    return vec4<f32>(col, 1.0);
}
