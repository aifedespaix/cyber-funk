// L'hologramme arcade (`arcade-building.ts`) : la partie (une texture de 72 × 48 cases, un octet par case) dessinée comme une
// image de petits cubes lumineux, avec lignes de balayage, tranches instables et trame de volume. Trois plans additifs,
// légèrement décalés en profondeur, donnent l'épaisseur ; un seul maillage fusionné, la couche de chaque sommet dans `uv1.x`
// (1 avant, 0,4 milieu, 0 arrière).
//
// Paramètres : a.x = déchirure (0..1, **déjà bornée** par le limiteur de flashs). Texture : la grille (`map`, échantillonnage
// au plus proche). Écart avec Sillage (règle 7) : les tranches ne bougent qu'avec la déchirure (pas de plancher), au plus
// 8 fois par seconde (10 dans Sillage).

const GRID_W: f32 = 72.0;
const GRID_H: f32 = 48.0;
const HOLO_W: f32 = 54.0;
const HOLO_H: f32 = 36.0;

struct HoloIn {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) layer: vec2<f32>,
}

struct HoloOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world: vec3<f32>,
    @location(2) layer: f32,
}

@vertex
fn vertex(v: HoloIn) -> HoloOut {
    let m = get_world_from_local(v.instance_index);
    var wp = m * vec4<f32>(v.position, 1.0);
    wp.y *= city_grow(wp.xyz);
    var out: HoloOut;
    out.uv = v.uv;
    out.layer = v.layer.x;
    out.world = wp.xyz;
    out.clip = position_world_to_clip(wp.xyz);
    return out;
}

// Couleur de chaque indice de la grille ; l'ordre suit `Px` (Rust).
fn pal(i: u32) -> vec3<f32> {
    switch i {
        case 0u: { return color(C_INK); }
        case 1u: { return color(C_MINT); }
        case 2u: { return color(C_ROSE); }
        case 3u: { return color(C_SUN); }
        case 4u: { return color(C_LIME); }
        case 5u: { return color(C_ICE); }
        case 6u: { return color(C_ALARM); }
        case 7u: { return color(C_AMBER); }
        case 8u: { return mix(color(C_VIOLET), color(C_ICE), 0.35); }
        case 9u: { return color(C_ALARM); }
        case 10u: { return color(C_AZURE); }
        default: { return color(C_VIOLET); }
    }
}

@fragment
fn fragment(in: HoloOut) -> @location(0) vec4<f32> {
    let time = p.frame.x;
    let pulse = p.frame.y;
    let glitch = p.a.x;
    let mood = p.mood.rgb;
    let grid = vec2<f32>(GRID_W, GRID_H);
    let front = step(0.5, in.layer);
    // Hologramme instable : des tranches se décalent, quand la déchirure le permet.
    let slice = floor(in.uv.y * grid.y / 3.0);
    let tick = floor(time * (4.0 + glitch * 4.0));
    let jump = (h11(slice + tick * 1.3) - 0.5) * 0.025 * glitch * step(0.86 - glitch * 0.3, h11(slice * 2.1 + tick));
    let g = vec2<f32>(in.uv.x + jump, in.uv.y) * grid;
    let cell = floor(g);
    let f = fract(g);
    let idx = u32(floor(textureSampleLevel(map, map_sampler, (cell + 0.5) / grid, 0.0).r * 255.0 + 0.5));
    let lit = step(0.5, f32(idx));
    let body = step(0.1, f.x) * step(f.x, 0.9) * step(0.1, f.y) * step(f.y, 0.9);
    var c = pal(idx);
    // Les plans arrière sont froids, pâles, et ne montrent que des pixels épars.
    c = mix(mix(c, mood, 0.55) * 0.45, c * 1.5, front);
    let scan = 0.72 + 0.28 * sin(in.uv.y * grid.y * 3.14159 + time * 6.0);
    let sweep = 1.0 + 0.5 * sstep(0.03, 0.0, abs(fract(in.uv.y - time * 0.12) - 0.5) - 0.47);
    var col = c * lit * body * scan * sweep * (1.0 + pulse * 0.4);
    // Trame du volume, très discrète.
    col += mood * (1.0 - step(0.06, min(f.x, f.y))) * 0.05 * (0.4 + front);
    // Cadre de l'image.
    let b = min(in.uv, 1.0 - in.uv) * vec2<f32>(HOLO_W, HOLO_H);
    col += mood * sstep(0.35, 0.15, min(b.x, b.y)) * (0.4 + front * 0.6);
    return vec4<f32>(col * city_fade(in.world), 1.0);
}
