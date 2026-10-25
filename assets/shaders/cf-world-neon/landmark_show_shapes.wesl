// Socle des façades des immeubles-vitrines de J7-09 (`FacadeLandmark` de `facade.ts`) : formes 2D communes (`SHAPE_GLSL`),
// lecture des uniformes et vertex d'un plan de façade. Placé après `landmark_common.wesl`.
//
// Paramètres communs à toutes les façades (le même `PartPaint` pour les dix) :
//   a.xyz, b.xyz, c.xyz, d.xyz = les couleurs uA, uB, uC, uD (lumière linéaire, choisies dans la palette par la fiche) ;
//   a.w = `uDrop`, le niveau BORNÉ par le limiteur de flashs (règle 7), 1 au drop, retombe en ~0,8 s ;
//   b.w, c.w = horloges propres à la façade, intégrées côté CPU (voir l'en-tête de chaque shader) : un produit
//   `time × audio` de Sillage téléportait les motifs à chaque variation de l'énergie.

struct Show {
    time: f32,
    pulse: f32,
    bass: f32,
    mid: f32,
    high: f32,
    beat: f32,
    intensity: f32,
    phase: f32,
    drop: f32,
    clock_b: f32,
    clock_c: f32,
    a: vec3<f32>,
    b: vec3<f32>,
    c: vec3<f32>,
    d: vec3<f32>,
    ink: vec3<f32>,
}

fn show() -> Show {
    var s: Show;
    s.time = p.frame.x;
    s.pulse = p.frame.y;
    s.bass = p.frame.z;
    s.mid = p.frame.w;
    s.high = p.audio.x;
    s.beat = p.audio.y;
    s.intensity = p.audio.z;
    s.phase = p.audio.w;
    s.drop = p.a.w;
    s.clock_b = p.b.w;
    s.clock_c = p.c.w;
    s.a = p.a.xyz;
    s.b = p.b.xyz;
    s.c = p.c.xyz;
    s.d = p.d.xyz;
    s.ink = color(C_INK);
    return s;
}

// Masque net (1 dedans, 0 dehors) d'un rectangle de demi-côtés `h` centré en `c`.
fn rect(q: vec2<f32>, c: vec2<f32>, h: vec2<f32>) -> f32 {
    let d = abs(q - c) - h;
    return 1.0 - step(0.0, max(d.x, d.y));
}

fn soft(d: f32, w: f32) -> f32 {
    return 1.0 - smoothstep(-w, w, d);
}

fn sd_box(q: vec2<f32>, c: vec2<f32>, h: vec2<f32>) -> f32 {
    let d = abs(q - c) - h;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

fn sd_round(q: vec2<f32>, c: vec2<f32>, h: vec2<f32>, r: f32) -> f32 {
    return sd_box(q, c, h - vec2<f32>(r)) - r;
}

fn sd_seg(q: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = q - a;
    let ba = b - a;
    return length(pa - ba * clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0));
}

fn rot(q: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(c * q.x - s * q.y, s * q.x + c * q.y);
}

@vertex
fn vertex(v: PlaneIn) -> PlaneOut {
    return plane_vertex(v);
}
