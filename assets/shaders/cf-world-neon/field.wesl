// Le champ de la ville (`FIELD_GLSL` de `landmarks/field.ts`) : les lumières dynamiques des immeubles-vitrines, leur zone de
// glitch, la panne de courant. Ce texte est ajouté au bout des shaders des façades (`facade.wesl`) et de la route
// (`road.wesl`) : les deux déclarent le même bloc à `@binding(1)`.
//
// Les positions sont en repère rendu (relatives à l'origine flottante de `cf-render`), comme `in.world`. Sans immeuble-vitrine,
// toutes les lumières sont éteintes (position à y = -999, couleur nulle), la zone et le décalque à force nulle, la panne à
// part éteinte nulle : ces fonctions rendent 0, 0, 0 et 1.

// Même ordre de champs que `FieldParams` (Rust, `ShaderType`). Tout en vec4.
struct Field {
    // xyz : position, w : rayon.
    light_pos: array<vec4<f32>, 6>,
    // xyz : couleur déjà multipliée par l'intensité.
    light_col: array<vec4<f32>, 6>,
    // x : z du centre, y : demi-longueur, z : force.
    zone: vec4<f32>,
    zone_tint: vec4<f32>,
    // Panne de courant : x : z du front, y : largeur du front, z : part éteinte derrière le front, w : sens.
    power: vec4<f32>,
    // Décalque : centre x, centre z, demi-largeur, demi-profondeur.
    decal_box: vec4<f32>,
    // Décalque : rangée de l'atlas, début et fin du texte (0..1), nombre de lettres.
    decal_word: vec4<f32>,
    // x : opacité, y : déchirure.
    decal: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(1) var<uniform> field: Field;

// Diffus : une lueur qui s'éteint avec la distance, un peu plus forte face à la source.
fn field_light(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    for (var i = 0; i < 6; i++) {
        let l = field.light_pos[i].xyz - p;
        let d = length(l);
        let att = clamp(1.0 - d / field.light_pos[i].w, 0.0, 1.0);
        let facing = 0.35 + 0.65 * max(dot(n, l / max(d, 0.001)), 0.0);
        sum += field.light_col[i].rgb * att * att * facing;
    }
    return sum;
}

// Sol mouillé : le reflet d'une source haute s'étire vers la caméra.
fn field_sheen(p: vec3<f32>, cam: vec3<f32>) -> vec3<f32> {
    let v = normalize(cam - p);
    let r = vec3<f32>(-v.x, v.y, -v.z);
    var sum = vec3<f32>(0.0);
    for (var i = 0; i < 6; i++) {
        let l = field.light_pos[i].xyz - p;
        let d = length(l);
        let att = clamp(1.0 - d / (field.light_pos[i].w * 1.7), 0.0, 1.0);
        let spec = pow(max(dot(r, l / max(d, 0.001)), 0.0), 18.0);
        sum += field.light_col[i].rgb * spec * att;
    }
    return sum;
}

// Part de lumière qui reste aux fenêtres : 1 partout, moins derrière le front d'une panne de courant.
fn field_power(p: vec3<f32>) -> f32 {
    let behind = smoothstep(-field.power.y, field.power.y, (p.z - field.power.x) * field.power.w);
    return 1.0 - field.power.z * behind;
}

// Zone de glitch : un fondu autour du centre, plein sur la moitié de sa longueur.
fn field_zone(p: vec3<f32>) -> f32 {
    return field.zone.z * (1.0 - smoothstep(field.zone.y * 0.55, field.zone.y, abs(p.z - field.zone.x)));
}
