// La façade « Lave de code » (`lava-building.ts`) : des 0 et des 1 en fusion qui coulent sur un magma. Deux rendus dans ce
// fichier : la façade (`LAVA_FACADE`) et la flaque au pied (`LAVA_POOL`) ; le matériau choisit l'entrée par le fichier
// assemblé (voir `render::material`), pas par un drapeau : chacun a son `vertex` et son `fragment`.
//
// Un seul fragment sur un plan, sans texture. Le magma est un bruit fractal qui tombe à la vitesse du flux, avec des
// fissures incandescentes (bruit crête). Les chiffres sont des signes distance dessinés dans une grille de colonnes, plus
// vifs là où le magma est chaud. Une éruption (un drop) fait monter une vague de chaleur du pied à la crête.
//
// Paramètres : a.x = éruption (0..1, **déjà bornée** par le limiteur de flashs), b.x = défilement du flux (intégré côté
// CPU, voir l'écart de la PR : `time × flow` de Sillage téléportait tout le motif à chaque variation des basses).

const SPAN: f32 = 40.0;
const HEIGHT: f32 = 76.0;

// Dégradé de chaleur, de la croûte noire au blanc incandescent : toutes les teintes viennent de la palette.
fn lava_ramp(h: f32) -> vec3<f32> {
    var c = color(C_INK);
    c = mix(c, color(C_ALARM) * 0.55, smoothstep(0.05, 0.35, h));
    c = mix(c, color(C_ROSE) * 1.1, smoothstep(0.3, 0.6, h));
    c = mix(c, color(C_AMBER) * 1.4, smoothstep(0.55, 0.8, h));
    c = mix(c, color(C_SUN) * 1.8, smoothstep(0.75, 1.0, h));
    return mix(c, color(C_ICE) * 2.2, smoothstep(1.0, 1.35, h));
}

fn sd_seg(q: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = q - a;
    let ba = b - a;
    return length(pa - ba * clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0));
}

// Un chiffre binaire dans une cellule 0–1 : anneau pour 0, barre et empattement pour 1.
fn digit(uv: vec2<f32>, bit: f32) -> f32 {
    let q = uv - 0.5;
    let ring = abs(length(q * vec2<f32>(1.5, 0.95)) - 0.33) - 0.075;
    let bar = max(abs(q.x) - 0.075, abs(q.y) - 0.38);
    let flag = sd_seg(q, vec2<f32>(-0.2, 0.2), vec2<f32>(0.0, 0.38)) - 0.07;
    let foot = max(abs(q.x) - 0.2, abs(q.y + 0.38) - 0.06);
    let d = select(ring, min(min(bar, flag), foot), bit > 0.5);
    return 1.0 - smoothstep(-0.01, 0.03, d);
}

