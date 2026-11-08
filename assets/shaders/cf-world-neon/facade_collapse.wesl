// Désynchronisation des immeubles (arrêt du show) : chaque tour rétrécit vers sa base enfouie, à son rythme, avec un
// glitch par paliers. Fonctions partagées par `facade.wesl` et `facade_prepass.wesl` (le passage de profondeur de
// l'occlusion doit voir la même tour que le dessin). Miroir Rust : `facade/collapse.rs`.

// Profondeur à laquelle les tours plongent sous la route (m) : `BASE_DEPTH_M` de `scenery/layout.rs`. Le rétrécissement se
// fait autour de cette base, jamais autour de y = 0 : la base ne remonte pas au-dessus de la route.
const COLLAPSE_BASE_DEPTH_M: f32 = 384.0;
// Période du décalage d'origine du monde (m) : `ORIGIN_STEP` (28 800) est un multiple de 1 200, ce qui garde le hasard
// d'une tour stable quand l'origine flottante saute.
const COLLAPSE_CELL_PERIOD_M: f32 = 1200.0;

fn collapse_hash(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 0.00583715))));
}

// Décalage propre à une tour, 0..1, tiré de la position de sa base (les pièces d'une même tour tombent ensemble).
fn tower_offset(origin_xz: vec2<f32>) -> f32 {
    let wrapped = origin_xz - COLLAPSE_CELL_PERIOD_M * floor(origin_xz / COLLAPSE_CELL_PERIOD_M);
    return collapse_hash(floor(wrapped / 2.0 + 0.5));
}

// Avancement de la chute de cette tour : les tours à fort décalage partent plus tard et finissent à temps.
fn collapse_t(collapse: f32, off: f32) -> f32 {
    return clamp(collapse * 1.6 - off * 0.6, 0.0, 1.0);
}

// Échelle verticale, 1 puis 0. Avec `glitch`, elle saute par paliers à 8 Hz. Le glitch n'existe que si le mouvement
// rapide est permis (`motion.y` > 0, qualité normale) : coupé avec les flashs, comme tout mouvement rapide (règle 7).
fn collapse_scale(t: f32, off: f32, time: f32, glitch: bool) -> f32 {
    var s = 1.0 - t * t * (3.0 - 2.0 * t);
    if glitch && t > 0.0 && t < 1.0 {
        let flick = collapse_hash(vec2<f32>(floor(time * 8.0), off * 91.0));
        let stepped = clamp(floor(s * 7.0 + 0.5) / 7.0 + (flick - 0.5) * 0.28, 0.0, 1.0);
        s = mix(s, stepped, step(0.55, flick));
    }
    return s;
}

// Décalage horizontal d'une tranche de 5 m de hauteur (m) : le glitch coupe la tour en bandes qui glissent.
fn collapse_shift(y: f32, t: f32, off: f32, time: f32, glitch: bool) -> f32 {
    if !glitch {
        return 0.0;
    }
    let amp = 4.0 * t * (1.0 - t);
    return (collapse_hash(vec2<f32>(floor(y / 5.0), floor(time * 8.0) + off * 17.0)) - 0.5) * 1.6 * amp;
}

// Applique la chute à un sommet (coordonnées du monde rendu). `collapse` = 0 : identité.
fn collapse_apply(wp: vec4<f32>, origin_xz: vec2<f32>, collapse: f32, time: f32, glitch: bool) -> vec4<f32> {
    if collapse <= 0.0 {
        return wp;
    }
    let off = tower_offset(origin_xz);
    let t = collapse_t(collapse, off);
    let s = collapse_scale(t, off, time, glitch);
    var out = wp;
    out.x += collapse_shift(wp.y, t, off, time, glitch);
    out.y = -COLLAPSE_BASE_DEPTH_M + (wp.y + COLLAPSE_BASE_DEPTH_M) * s;
    return out;
}
