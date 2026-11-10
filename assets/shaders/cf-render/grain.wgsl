// Grain de pellicule (DA §3.1), après le tonemapping, en espace affiché :
//   n  = hash_entier(px, floor(t·24))            (lowbias32, aucun sin())
//   c' = c·(1 + G·(n − 0.5)·w(Y')) ,  w(Y') = 0.25 + 3·Y'·(1 − Y')     maximal dans les demi-teintes
// Multiplicatif : un pixel à 0 reste à 0. Miroir Rust : `postfx::finish` (grain_noise, grain_factor).

struct FullscreenVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct Grain {
    gain: f32,
    frame: u32,
    _pad0: f32,
    _pad1: f32,
};

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> grain: Grain;

const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

fn lowbias32(v: u32) -> u32 {
    var x = v;
    x ^= x >> 16u;
    x *= 0x7feb352du;
    x ^= x >> 15u;
    x *= 0x846ca68bu;
    x ^= x >> 16u;
    return x;
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let px = vec2<u32>(in.position.xy);
    let col = textureLoad(screen_texture, vec2<i32>(px), 0);
    let h = lowbias32(px.x ^ lowbias32((px.y + 0x9e3779b9u) ^ lowbias32(grain.frame)));
    let n = f32(h >> 8u) / 16777216.0;
    let y = clamp(dot(col.rgb, LUMA), 0.0, 1.0);
    let k = 1.0 + grain.gain * (n - 0.5) * (0.25 + 3.0 * y * (1.0 - y));
    return vec4<f32>(col.rgb * k, col.a);
}
