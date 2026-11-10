// Persistance rétinienne (DA §3.1). Deux points d'entrée, même disposition de liaison :
//   `update`    (demi-résolution) : H_t = max(C·masque, H_{t−1}(uv')·exp(−Δt/τ_p))
//   `composite` (pleine résolution) : C_out = C + 0.6·(H_t − C·masque)⁺
// avec masque = smoothstep(θ_p, 2·θ_p, luma(C)) et uv' = pf + (uv − pf)·(1 − ζ). Miroir Rust : `postfx::persistence`.
// Le flash du spectacle s'ajoute plus tard (passe d'étalonnage) : la persistance ne prolonge jamais un flash.

struct FullscreenVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct Persist {
    decay: f32,
    zeta: f32,
    reset: f32,
    enabled: f32,
    vanishing: vec2<f32>,
    threshold: f32,
    gain: f32,
};

@group(0) @binding(0) var scene_texture: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;
@group(0) @binding(2) var<uniform> persist: Persist;
@group(0) @binding(3) var history_texture: texture_2d<f32>;

const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

fn emissive(c: vec3<f32>) -> vec3<f32> {
    let m = smoothstep(persist.threshold, 2.0 * persist.threshold, dot(c, LUMA));
    return c * m;
}

@fragment
fn update(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    // Quatre prises pour réduire la scène en demi-résolution sans crénelage.
    let texel = 0.5 / vec2<f32>(textureDimensions(scene_texture));
    var scene = vec3<f32>(0.0);
    scene += textureSampleLevel(scene_texture, linear_sampler, in.uv + vec2<f32>(-texel.x, -texel.y), 0.0).rgb;
    scene += textureSampleLevel(scene_texture, linear_sampler, in.uv + vec2<f32>(texel.x, -texel.y), 0.0).rgb;
    scene += textureSampleLevel(scene_texture, linear_sampler, in.uv + vec2<f32>(-texel.x, texel.y), 0.0).rgb;
    scene += textureSampleLevel(scene_texture, linear_sampler, in.uv + vec2<f32>(texel.x, texel.y), 0.0).rgb;
    let fresh = emissive(scene * 0.25);
    let uv_prev = persist.vanishing + (in.uv - persist.vanishing) * (1.0 - persist.zeta);
    let prev = textureSampleLevel(history_texture, linear_sampler, uv_prev, 0.0).rgb * persist.decay * (1.0 - persist.reset);
    return vec4<f32>(max(fresh, prev), 1.0);
}

@fragment
fn composite(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(scene_texture, linear_sampler, in.uv, 0.0).rgb;
    let h = textureSampleLevel(history_texture, linear_sampler, in.uv, 0.0).rgb;
    let trail = max(h - emissive(c), vec3<f32>(0.0));
    return vec4<f32>(c + persist.gain * trail, 1.0);
}
