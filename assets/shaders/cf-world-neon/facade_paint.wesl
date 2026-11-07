// Graffitis muraux : un décalque peint sur la zone basse d'une façade, depuis l'atlas mural cuit à l'ouverture
// (`graffiti/atlas.rs`). Ajouté au bout de `facade.wesl`, qui déclare l'atlas (`wall_map`, `wall_sampler`) et sa table
// (`wall`) ; appelé seulement si le tag de la pièce porte un motif (`graffiti != 0u`), donc une façade sans graffiti est
// rendue exactement comme avant.
//
// Règle 7 de `CLAUDE.md` : le décalque est une peinture, pas une lumière. Son éclat ne dépend que de `gain` (éclairage
// ambiant, pluie, distance), jamais du kick ni de l'audio : aucune variation rapide de luminance.

// Largeur de départ du décalque, en mètres (réduite si la façade est plus étroite), et sa variation d'échelle par immeuble.
const WALL_PANEL_W: f32 = 7.0;
const WALL_SCALE_VAR: f32 = 0.5;
// Altitude du bas du décalque : de 0,8 m à 3 m, pour rester dans le rez-de-chaussée et les premiers étages.
const WALL_BOTTOM_MIN: f32 = 0.8;
const WALL_BOTTOM_VAR: f32 = 2.2;

// `uv` : abscisse le long de la face (m, centrée) et altitude (m). `face` : x numéro de face (1 : +x, 2 : -x, 3 : +z, 4 : -z),
// y largeur de la face (m), z graine de l'immeuble. `base` : la couleur déjà calculée. `gain` : part de lumière 0..1.
fn paint_wall(uv: vec2<f32>, face: vec3<f32>, graffiti: u32, base: vec3<f32>, gain: f32) -> vec3<f32> {
    let k = min(graffiti, 31u);
    let info = wall.info[k];
    if info.w < 0.5 || gain <= 0.0 {
        return base;
    }
    let seed = face.z;
    // Un seul graffiti par immeuble : une seule des quatre faces le porte.
    let chosen = 1.0 + floor(h21(vec2<f32>(seed * 0.713, 11.0)) * 4.0);
    if abs(face.x - chosen) > 0.5 {
        return base;
    }
    let rect = wall.rects[k];
    let aspect = rect.z / max(rect.w, 1.0);
    let scale = WALL_PANEL_W * (1.0 - WALL_SCALE_VAR * 0.5 + WALL_SCALE_VAR * h21(vec2<f32>(seed * 0.331, 23.0)));
    let panel_w = min(scale, face.y * 0.92);
    let panel_h = panel_w / aspect;
    let centre = (h21(vec2<f32>(seed * 0.517, 37.0)) - 0.5) * max(face.y - panel_w, 0.0);
    // Le texte se lit de gauche à droite vu de la rue : on retourne l'abscisse sur les faces +x et -z.
    let way = select(1.0, -1.0, face.x < 1.5 || face.x > 3.5);
    let lu = (uv.x - centre) * way / panel_w + 0.5;
    let bottom = WALL_BOTTOM_MIN + WALL_BOTTOM_VAR * h21(vec2<f32>(seed * 0.189, 51.0));
    let lv = (uv.y - bottom) / panel_h;
    if lu < 0.0 || lu > 1.0 || lv < 0.0 || lv > 1.0 {
        return base;
    }
    let px = rect.xy + vec2<f32>(lu, 1.0 - lv) * rect.zw;
    let texel = textureSampleLevel(wall_map, wall_sampler, px / wall.atlas.xy, i32(info.x), 0.0);
    // Alpha prémultiplié : la peinture recouvre la façade, elle ne s'y ajoute pas.
    return base * (1.0 - texel.a) + texel.rgb * gain;
}
