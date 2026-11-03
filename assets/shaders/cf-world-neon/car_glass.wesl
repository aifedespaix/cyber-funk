// Les vitres de la voiture sous la pluie : gouttes posées, gouttes qui coulent en laissant une traînée, film d'eau qui lisse
// le verre. Extension de `StandardMaterial` (le verre reste du verre : reflets des néons, transparence) ; le shader ne touche
// qu'à l'entrée PBR avant l'éclairage, comme `car_paint.wesl`.
//
// Le motif est dessiné **sur le plan de la vitre**, en position monde rendue : la base du motif est la pente du verre
// (la projection de « vers le bas » sur sa surface). Les gouttes coulent donc vers le bas de la vitre quelle que soit son
// inclinaison, et suivent la gravité si la voiture se cabre en vol. À plat (toit), il n'y a pas de pente : les gouttes ne
// coulent pas, elles s'étalent sur place.
//
// Chaque goutte est une petite lentille : sa normale est inclinée selon la distance au centre, ce qui déforme les reflets
// (les néons se brisent dans la goutte) et l'éclaircit un peu. Sans pluie (`glass.rain.x` ≈ 0) le shader sort aussitôt :
// la vitre est celle du modèle.

import bevy_pbr::render::{
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
};

// `rain` : x intensité de l'averse (0 à 1), y temps (s), z vitesse rapportée à la croisière (0 à 1), w non utilisé.
struct Glass {
    rain: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> glass: Glass;

// Taille des gouttes : une cellule du motif fait environ 7 cm de côté.
const DENSITY: f32 = 14.0;

fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(123.34, 345.45));
    q += dot(q, q + 34.345);
    return fract(q.x * q.y);
}

fn hash22(p: vec2<f32>) -> vec2<f32> {
    let n = hash21(p);
    return vec2<f32>(n, hash21(p + n));
}

// Gouttes posées : une par cellule, qui grossit puis s'efface. `x` : masque, `yz` : position dans la goutte (-1 à 1).
fn resting_drops(uv: vec2<f32>, t: f32, amount: f32) -> vec3<f32> {
    let cell = floor(uv);
    let f = fract(uv);
    var out = vec3<f32>(0.0);
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let o = vec2<f32>(f32(dx), f32(dy));
            let id = cell + o;
            let r = hash22(id);
            // Seule une part des cellules porte une goutte : plus il pleut, plus il y en a.
            if hash21(id + 11.7) > amount * 0.8 {
                continue;
            }
            let life = fract(t * (0.02 + 0.03 * r.x) + r.y);
            let swell = smoothstep(0.0, 0.18, life) * (1.0 - smoothstep(0.75, 1.0, life));
            let radius = (0.10 + 0.17 * hash21(id + 3.1)) * swell;
            let centre = o + 0.15 + r * 0.7 - f;
            let d = length(centre);
            let m = smoothstep(radius, radius * 0.55, d) * step(0.001, radius);
            if m > out.x {
                out = vec3<f32>(m, -centre / max(radius, 0.001));
            }
        }
    }
    return out;
}

// Gouttes qui coulent : une colonne sur deux porte une goutte qui descend de façon saccadée et laisse un filet derrière
// elle. Le motif se répète tous les `HEIGHT` ; la traînée ne franchit pas la jonction (invisible : la goutte repart d'en haut).
fn running_drops(uv: vec2<f32>, t: f32, amount: f32, speed: f32) -> vec3<f32> {
    let HEIGHT = 5.0;
    let column = floor(uv.x * 0.5);
    let r = hash22(vec2<f32>(column, 41.0));
    if r.x > amount * 0.9 {
        return vec3<f32>(0.0);
    }
    let x_in = uv.x * 0.5 - column - 0.5;
    // Avance par à-coups (la goutte accroche, puis dévale) : sinus redressé, plus rapide quand la voiture file.
    let pace = (0.06 + 0.12 * r.y) * (1.0 + speed * 2.5);
    let clock = t * pace + r.x * 9.0;
    let head = fract(clock + 0.18 * sin(clock * 6.2832 * 3.0) * 0.3) * HEIGHT;
    let wobble = sin(uv.y * 1.3 + r.y * 6.0) * 0.12 * (1.0 - smoothstep(0.0, HEIGHT, head));
    let dx = (x_in + wobble) * 2.0;
    let y_wrapped = uv.y - floor(uv.y / HEIGHT) * HEIGHT;
    let dy = y_wrapped - head;
    // Goutte : un ovale allongé vers le bas (le haut est fin, le bas bombé).
    let body = smoothstep(0.17, 0.05, length(vec2<f32>(dx, dy * 0.75)));
    // Filet : étroit, derrière la goutte (dy < 0), il s'amincit puis s'efface.
    let behind = clamp(-dy / 2.4, 0.0, 1.0);
    let trail = smoothstep(0.045 * (1.0 - behind * 0.7), 0.0, abs(dx)) * step(dy, 0.0) * (1.0 - behind) * 0.6;
    let m = max(body, trail);
    return vec3<f32>(m, vec2<f32>(dx, dy * 0.75) / 0.17 * body);
}

@fragment
fn fragment(vertex_output: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var in = vertex_output;
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color,
    );

    let amount = clamp(glass.rain.x, 0.0, 1.0);
    if amount > 0.01 {
        let n = normalize(in.world_normal);
        let up = vec3<f32>(0.0, 1.0, 0.0);
        // Pente du verre : « vers le bas » ramené dans le plan de la vitre. Pas de pente à plat : repli arbitraire.
        var down = -up + n * dot(n, up);
        var slope = length(down);
        if slope < 0.08 {
            down = vec3<f32>(0.0, 0.0, 1.0);
            slope = 0.0;
        } else {
            down = down / slope;
        }
        let right = normalize(cross(n, down));
        let uv = vec2<f32>(dot(in.world_position.xyz, right), dot(in.world_position.xyz, down)) * DENSITY;

        let t = glass.rain.y;
        let resting = resting_drops(uv, t, amount);
        // Seules les vitres en pente voient couler : à plat, l'eau s'étale.
        let running = running_drops(uv, t, amount, glass.rain.z) * smoothstep(0.1, 0.4, slope);
        let drop = select(running, resting, resting.x >= running.x);
        let mask = clamp(drop.x, 0.0, 1.0);

        // Lentille : la normale s'incline selon la position dans la goutte, ce qui brise les reflets.
        let bend = (right * drop.y + down * drop.z) * mask * 0.75;
        pbr_input.N = normalize(pbr_input.N + bend);
        pbr_input.world_normal = pbr_input.N;

        // Film d'eau : le verre mouillé est plus lisse ; les gouttes sont presque transparentes mais plus présentes (alpha).
        pbr_input.material.perceptual_roughness = mix(
            pbr_input.material.perceptual_roughness,
            0.04,
            clamp(amount * 0.5 + mask * 0.5, 0.0, 1.0),
        );
        let base = pbr_input.material.base_color;
        pbr_input.material.base_color = vec4<f32>(
            mix(base.rgb, vec3<f32>(0.75, 0.85, 0.95), mask * 0.25),
            max(base.a, mask * 0.45),
        );
    }

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
