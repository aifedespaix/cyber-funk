// Enseigne de Neon City : une cellule de l'atlas habillée par le shader.
//
// Remplace le `CanvasTexture` repeint à chaque image de Sillage (`signs/painter.ts`). La cellule est une texture cuite au
// chargement, en quatre canaux de couverture : R = accent de la publicité, G = glace, B = couleur propre à la cellule,
// A = encre (voile sombre sous les couleurs). Ce shader y ajoute, dans l'ordre de `painter.ts`, ce qui se calculait par
// image : le voile d'accent, le bandeau défilant, le cadre, la trame, et le flux d'une caméra (dessiné ici, voir la PR).
//
// Rien de ce qui clignote n'est calculé ici : le pouls (`state.x`), le flash du panneau (`state.y`) et le niveau du canal
// R soumis au limiteur (`state.z`) arrivent déjà bornés par le `FlashLimiter` (règle 7 de `CLAUDE.md`).
//
// Mélange « alpha prémultiplié » : une sortie à alpha nul s'additionne à la scène (hologramme, projection, néon).

import bevy_pbr::render::forward_io::VertexOutput;

// Même ordre de champs que `SignParams` (Rust, `ShaderType`).
struct Sign {
    // xy : coin de la cellule dans la page d'art (px), zw : taille de la page.
    cell: vec4<f32>,
    // x : haut du bandeau dans sa page, y : largeur de la page, z : longueur de la boucle, w : position de lecture (px).
    strip: vec4<f32>,
    // x : hauteur de la page de bandeaux, y : 1 s'il y a un bandeau, z : 1 si la cellule n'a qu'un canal, w : le rendu.
    mode: vec4<f32>,
    // Accent, w : opacité.
    accent: vec4<f32>,
    ice: vec4<f32>,
    // Couleur du canal B, w : drapeaux de la cellule.
    second: vec4<f32>,
    ink: vec4<f32>,
    // Violet nuit, w : graine du panneau.
    velvet: vec4<f32>,
    // x : pouls limité, y : flash du panneau, z : niveau du canal R soumis au limiteur, w : horloge (s).
    state: vec4<f32>,
    // x : km/h, y : numéro de caméra, z : plan (0 large, 1 face, 2 profil), w : 1 si la caméra a un flux.
    feed: vec4<f32>,
    // x : 1 si les deux faces se voient, y : surcroît de lumière par flash, z : 1 en mode calme.
    misc: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> sign: Sign;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var art: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var art_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var strips: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(4) var strips_sampler: sampler;

// Mise en page de la cellule : recopiée de `paint` (un test Rust compare les deux).
const CANVAS_W: f32 = 512.0;
const CANVAS_H: f32 = 256.0;
const IMAGE_H: f32 = 216.0;
const TICKER_H: f32 = 40.0;
const FEED_X: f32 = 10.0;
const FEED_Y: f32 = 10.0;
const FEED_W: f32 = 492.0;
const FEED_H: f32 = 202.0;
const BAND_Y: f32 = 216.0;
const DIGITS_X: f32 = 4.0;
const DIGITS_Y: f32 = 236.0;
const DIGITS_H: f32 = 20.0;
const DIGIT_SLOT: f32 = 13.0;
const LABEL_X0: f32 = 4.0;
const LABEL_W0: f32 = 56.0;
const LABEL_X1: f32 = 64.0;
const LABEL_W1: f32 = 70.0;
const LABEL_X2: f32 = 140.0;
const LABEL_W2: f32 = 150.0;

const FLAG_GATE_R: u32 = 1u;
const FLAG_VERDICT: u32 = 2u;

const LOOK_HOLO: i32 = 0;
const LOOK_METAL: i32 = 1;
const LOOK_GHOST: i32 = 2;
const LOOK_NEON: i32 = 3;
const LOOK_CAMERA: i32 = 4;

// `dst` est prémultiplié : rgb déjà multiplié par l'alpha. Composition « source over ».
fn over(dst: vec4<f32>, color: vec3<f32>, a: f32) -> vec4<f32> {
    let s = clamp(a, 0.0, 1.0);
    return vec4<f32>(color * s + dst.rgb * (1.0 - s), s + dst.a * (1.0 - s));
}

// Un pixel de la cellule, en pixels de cellule (0..512, 0..256), sans déborder sur la voisine.
fn cell_px(q: vec2<f32>) -> vec4<f32> {
    let c = clamp(q, vec2<f32>(0.5), vec2<f32>(CANVAS_W, CANVAS_H) - vec2<f32>(0.5));
    return textureSampleLevel(art, art_sampler, (sign.cell.xy + c) / sign.cell.zw, 0.0);
}

// Les quatre canaux de couverture ; une cellule à un canal (projection) ne remplit que R.
fn coverage(q: vec2<f32>) -> vec4<f32> {
    let c = cell_px(q);
    if sign.mode.z > 0.5 {
        return vec4<f32>(c.r, 0.0, 0.0, 0.0);
    }
    return c;
}

// Le bandeau : couverture du texte à l'abscisse `x_px` du panneau et à la ligne `y_px` du bandeau.
fn ticker_cov(x_px: f32, y_px: f32) -> f32 {
    let period = max(sign.strip.z, 1.0);
    let s = x_px + sign.strip.w;
    let x = s - period * floor(s / period);
    let uv = vec2<f32>((x + 0.5) / sign.strip.y, (sign.strip.x + clamp(y_px, 0.5, TICKER_H - 0.5)) / sign.mode.x);
    return textureSampleLevel(strips, strips_sampler, uv, 0.0).r;
}

fn hash21(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let r = q + dot(q, q + 45.32);
    return fract(r.x * r.y);
}

// Le visuel de la cellule : voile sombre, puis accent, glace, couleur propre. Le verdict (« GAME OVER ») n'apparaît que
// sous le flash du panneau : une plaque d'accent et le texte en découpe sombre.
fn product(dst: vec4<f32>, q: vec2<f32>) -> vec4<f32> {
    var c = dst;
    let flags = u32(round(sign.second.w));
    let cov = coverage(q);
    let verdict = (flags & FLAG_VERDICT) != 0u;
    if !verdict {
        c = over(c, sign.ink.rgb, cov.a);
    }
    var r = cov.r;
    if (flags & FLAG_GATE_R) != 0u {
        r = r * sign.state.z;
    }
    c = over(c, sign.accent.rgb, r);
    c = over(c, sign.ice.rgb, cov.g);
    c = over(c, sign.second.rgb, cov.b);
    if verdict {
        let flash = sign.state.y;
        c = over(c, sign.accent.rgb, flash * 0.9);
        c = over(c, sign.ink.rgb, cov.a * flash);
    }
    return c;
}

// Le bandeau défilant, sous l'image : fond d'encre à 85 %, texte d'accent qui vire à la glace sur le pouls.
fn banner(dst: vec4<f32>, p: vec2<f32>) -> vec4<f32> {
    if p.y < BAND_Y || sign.mode.y < 0.5 {
        return dst;
    }
    var c = over(dst, sign.ink.rgb, 0.85);
    let color = mix(sign.accent.rgb, sign.ice.rgb, sign.state.x * 0.9);
    c = over(c, color, ticker_cov(p.x, p.y - BAND_Y));
    return c;
}

// Cadre d'accent de `width` pixels (`strokeRect`).
fn frame(dst: vec4<f32>, p: vec2<f32>, width: f32) -> vec4<f32> {
    let inside = p.x >= width && p.x <= CANVAS_W - width && p.y >= width && p.y <= CANVAS_H - width;
    if inside {
        return dst;
    }
    return over(dst, sign.accent.rgb, 1.0);
}

// Trame : une ligne d'encre de 1,5 pixel tous les `step` pixels.
fn scan(dst: vec4<f32>, p: vec2<f32>, alpha: f32, step_px: f32) -> vec4<f32> {
    let m = p.y - step_px * floor(p.y / step_px);
    if m < 1.5 {
        return over(dst, sign.ink.rgb, alpha);
    }
    return dst;
}

// Un chiffre de la rangée de chiffres de la cellule (voir `paint::cam`), étiré dans la boîte (origin, size).
fn digit(dst: vec4<f32>, p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, d: i32, color: vec3<f32>) -> vec4<f32> {
    let l = (p - origin) / size;
    if l.x < 0.0 || l.x >= 1.0 || l.y < 0.0 || l.y >= 1.0 {
        return dst;
    }
    let q = vec2<f32>(DIGITS_X + f32(d) * DIGIT_SLOT + l.x * DIGIT_SLOT, DIGITS_Y + l.y * DIGITS_H);
    return over(dst, color, cell_px(q).g);
}

// Le flux d'une caméra, dessiné ici : route en perspective, voiture, trame ; neige sans signal. Cadence réduite (12 i/s).
fn feed_color(p: vec2<f32>) -> vec3<f32> {
    let f = (p - vec2<f32>(FEED_X, FEED_Y)) / vec2<f32>(FEED_W, FEED_H);
    let tq = floor(sign.state.w * 12.0) / 12.0;
    var col = mix(sign.ink.rgb, sign.velvet.rgb, f.y);
    if sign.feed.w < 0.5 {
        // Sans signal : de la neige, qui change six fois par seconde.
        let n = hash21(floor(p / 8.0) + vec2<f32>(floor(sign.state.w * 6.0) * 17.0, 3.0));
        col = col + sign.ice.rgb * n * 0.2;
    } else {
        let shot = i32(round(sign.feed.z));
        let horizon = 0.42;
        if f.y > horizon {
            let t = (f.y - horizon) / (1.0 - horizon);
            let half = 0.04 + 0.46 * t;
            let dx = abs(f.x - 0.5);
            if dx < half {
                col = mix(col, sign.velvet.rgb * 1.6 + sign.ink.rgb * 0.4, 0.8);
                let dash = fract(0.35 / max(t, 0.02) - tq * 2.0);
                if dx < 0.006 + 0.02 * t && dash < 0.5 {
                    col = sign.ice.rgb * 0.8;
                }
            }
            if abs(dx - half) < 0.006 + 0.01 * t {
                col = sign.accent.rgb;
            }
        }
        // La voiture : large depuis le portique, de face devant le capot, ou de profil.
        var center = vec2<f32>(0.5, 0.6);
        var half_size = vec2<f32>(0.05, 0.035);
        if shot == 1 {
            center = vec2<f32>(0.5, 0.58);
            half_size = vec2<f32>(0.17, 0.12);
        } else if shot == 2 {
            center = vec2<f32>(0.5, 0.64);
            half_size = vec2<f32>(0.26, 0.07);
        }
        let d = abs(f - center);
        if d.x < half_size.x && d.y < half_size.y {
            col = sign.accent.rgb;
            if f.y < center.y - half_size.y * 0.3 {
                col = mix(sign.ink.rgb, sign.ice.rgb, 0.25);
            }
        }
        if shot == 0 {
            // Réticule qui encadre la voiture.
            let r = abs(f - vec2<f32>(0.5, 0.58));
            let edge = max(r.x / 0.09, r.y / 0.07);
            if abs(edge - 1.0) < 0.05 {
                col = sign.accent.rgb;
            }
        } else if f.y < 0.1 || f.y > 0.9 {
            // Bandes de cinéma et filet d'accent.
            col = mix(col, sign.ink.rgb, 0.85);
            if abs(f.y - 0.1) < 0.006 || abs(f.y - 0.9) < 0.006 {
                col = sign.accent.rgb;
            }
        }
    }
    // Trame de balayage.
    if fract(p.y / 4.0) < 0.375 {
        col = col * 0.72;
    }
    return col;
}

fn camera(p: vec2<f32>) -> vec4<f32> {
    // Cadre opaque, fenêtre vidéo découpée.
    var c = vec4<f32>(sign.ink.rgb, 1.0);
    let in_window = p.x >= FEED_X && p.x < FEED_X + FEED_W && p.y >= FEED_Y && p.y < FEED_Y + FEED_H;
    if in_window {
        c = vec4<f32>(feed_color(p), 1.0);
        // Le flash du radar, borné par le limiteur.
        c = over(c, sign.ice.rgb, sign.state.y);
    }
    if p.y < IMAGE_H {
        c = product(c, p);
    }
    let live = sign.feed.w > 0.5;
    // Étiquette : REC (plan large), LIVE (autres plans), SANS SIGNAL (pas de flux).
    var label_x = LABEL_X2;
    var label_w = LABEL_W2;
    if live {
        if i32(round(sign.feed.z)) == 0 {
            label_x = LABEL_X0;
            label_w = LABEL_W0;
        } else {
            label_x = LABEL_X1;
            label_w = LABEL_W1;
        }
    }
    let l = p - vec2<f32>(FEED_X + select(14.0, 30.0, live), FEED_Y + 11.0);
    if l.x >= 0.0 && l.y >= 0.0 && l.x < label_w && l.y < 18.0 {
        c = over(c, sign.accent.rgb, cell_px(vec2<f32>(label_x + l.x, BAND_Y + 1.0 + l.y)).r);
    }
    if live {
        // Voyant d'enregistrement : il respire doucement.
        let blink = 0.55 + 0.45 * sin(sign.state.w * 3.14159);
        if distance(p, vec2<f32>(FEED_X + 18.0, FEED_Y + 20.0)) < 6.0 {
            c = over(c, sign.accent.rgb, blink);
        }
    }
    // Numéro de caméra (deux chiffres), toujours.
    let id = i32(clamp(round(sign.feed.y), 0.0, 99.0));
    let id_y = FEED_Y + 11.0;
    c = digit(c, p, vec2<f32>(466.4, id_y), vec2<f32>(10.8, 18.0), id / 10, sign.accent.rgb);
    c = digit(c, p, vec2<f32>(477.2, id_y), vec2<f32>(10.8, 18.0), id % 10, sign.accent.rgb);
    if live {
        // Vitesse, sans zéro devant.
        let kmh = i32(clamp(round(sign.feed.x), 0.0, 999.0));
        let top = FEED_Y + FEED_H - 8.0 - 26.0;
        if kmh >= 100 {
            c = digit(c, p, vec2<f32>(363.2, top), vec2<f32>(15.6, 26.0), kmh / 100, sign.ice.rgb);
        }
        if kmh >= 10 {
            c = digit(c, p, vec2<f32>(378.8, top), vec2<f32>(15.6, 26.0), (kmh / 10) % 10, sign.ice.rgb);
        }
        c = digit(c, p, vec2<f32>(394.4, top), vec2<f32>(15.6, 26.0), kmh % 10, sign.ice.rgb);
    }
    return banner(c, p);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> @location(0) vec4<f32> {
    // Un panneau à caisson n'a pas d'image au dos : le caisson la cache.
    if sign.misc.x < 0.5 && !is_front {
        discard;
    }
    let p = in.uv * vec2<f32>(CANVAS_W, CANVAS_H);
    let look = i32(round(sign.mode.w));
    let opacity = sign.accent.w;
    let pulse = sign.state.x;
    var c = vec4<f32>(0.0);
    var additive = true;
    if look == LOOK_HOLO {
        c = over(c, sign.accent.rgb, 0.08 + pulse * 0.05);
        if p.y < IMAGE_H {
            c = product(c, p);
        }
        c = banner(c, p);
        c = frame(c, p, 3.0);
        c = scan(c, p, 0.35, 4.0);
    } else if look == LOOK_METAL {
        // Caisson rétro-éclairé : fond opaque, trame très discrète.
        additive = false;
        c = vec4<f32>(sign.ink.rgb, 1.0);
        c = over(c, sign.accent.rgb, 0.14 + pulse * 0.06);
        if p.y < IMAGE_H {
            c = product(c, p);
        }
        c = banner(c, p);
        c = frame(c, p, 5.0);
        c = scan(c, p, 0.12, 6.0);
    } else if look == LOOK_GHOST {
        // Les lettres respirent au tempo.
        c = over(c, sign.accent.rgb, coverage(p).r * (0.55 + pulse * 0.35));
    } else if look == LOOK_NEON {
        let cov = coverage(p);
        let bright = (0.75 + pulse * 0.25) / 0.75;
        // Les tubes morts respirent lentement, jamais en stroboscope.
        let dead = 0.35 + 0.65 * (0.5 + 0.5 * sin(sign.state.w * 0.9 + sign.velvet.w * 3.0));
        c = over(c, sign.accent.rgb, cov.r * bright);
        c = over(c, sign.ice.rgb, cov.g * bright);
        c = over(c, sign.accent.rgb, cov.b * dead * bright);
    } else {
        additive = false;
        c = camera(p);
    }
    let glow = 1.0 + sign.state.y * sign.misc.y;
    let k = opacity * glow;
    if additive {
        return vec4<f32>(c.rgb * k, 0.0);
    }
    return vec4<f32>(c.rgb * k, c.a * opacity);
}
