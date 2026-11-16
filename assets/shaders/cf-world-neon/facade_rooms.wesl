// Fenêtres habitées (chantier C4 ; DA §5, idée de rupture n° 1) : chaque fenêtre ouvre sur une pièce en volume, calculée
// dans le shader de façade, sans géométrie.
//
// Ce texte est ajouté au bout de `facade.wesl` (voir `FACADE_SOURCE` dans `material.rs`) : il lit `h21`, `BAY_WIDTH` et
// `FLOOR_HEIGHT` de ce fichier. Miroir Rust : `facade/rooms.rs` (testé, constantes comparées).
//
// Principe : la cellule (une travée sur un étage) est la face avant d'une boîte unité. Le rayon de vue, en espace tangent
// (x le long de la façade, y vers le haut, z vers l'intérieur), entre par le point `p = (f.x, f.y, 0)` et touche une paroi
// à `t = min_axe((step(0, d) − p)/d)`. La paroi touchée donne la lumière : plafond clair (sa rampe lumineuse), sol sombre,
// mur du fond et sa porte entre les deux. Deux plans intermédiaires portent les silhouettes : le mobilier (z = 0,6) et, dans une pièce
// sur quatre, un habitant (z = 0,45) qui fait les cent pas. Une pièce sur cinq a un store.
//
// Rien ici ne suit la musique : la pièce module la lumière de la vitre (qui, elle, reste plafonnée à 0,9·θ_b), et
// l'habitant se déplace en dizaines de secondes. Aucune variation brusque de luminance (règle 7).

// Profondeur d'une pièce, en mètres : de 2,2 à 4,5 selon son hachage.
const ROOM_DEPTH_MIN_M: f32 = 2.2;
const ROOM_DEPTH_MAX_M: f32 = 4.5;
// Lumière de chaque paroi (la vitre en aplat vaut 1).
const ROOM_BACK: f32 = 0.9;
const ROOM_CEILING: f32 = 1.0;
const ROOM_FLOOR: f32 = 0.35;
const ROOM_SIDE: f32 = 0.55;
// Ce que la rampe du plafond ajoute dans son axe, et sa demi-largeur (en part de la travée).
const ROOM_LAMP: f32 = 0.7;
const ROOM_LAMP_HALF: f32 = 0.09;
// Ce qu'il reste de lumière au fond de la pièce.
const ROOM_DEPTH_FALL: f32 = 0.5;
// La porte du mur du fond : ce qu'elle garde de la lumière du mur.
const ROOM_DOOR: f32 = 0.5;
// Demi-largeur du montant central de la vitre (en part de la travée) : il fixe le plan du verre, la pièce glisse derrière.
const ROOM_MULLION_HALF: f32 = 0.018;
// Ce qu'une silhouette (mobilier, habitant, lame de store) laisse passer.
const ROOM_SHADOW: f32 = 0.22;
// Plans des silhouettes, en part de la profondeur.
const ROOM_FURNITURE_Z: f32 = 0.6;
const ROOM_PERSON_Z: f32 = 0.45;
// Part des pièces habitées, et des pièces à store.
const ROOM_PERSON_SHARE: f32 = 0.25;
const ROOM_BLIND_SHARE: f32 = 0.2;
// Période de l'aller-retour d'un habitant (s) : elle divise 2 400 s, la durée au bout de laquelle le temps reboucle.
const ROOM_PACE_S: f32 = 48.0;
// La pièce se fond en aplat entre ces deux distances (m).
const ROOM_FADE_NEAR_M: f32 = 90.0;
const ROOM_FADE_FAR_M: f32 = 150.0;

// Le rayon `d` (unités de boîte, d.z > 0) entré en `p` : paramètre de sortie de la boîte unité.
fn room_exit(p: vec3<f32>, d: vec3<f32>) -> vec3<f32> {
    // Un axe parallèle à la paroi ne la touche jamais : on écarte la division par zéro.
    let safe = select(d, vec3<f32>(1.0e-5), abs(d) < vec3<f32>(1.0e-5));
    return (step(vec3<f32>(0.0), safe) - p) / safe;
}

// Lumière de la pièce vue par la vitre, au plus `ROOM_CEILING + ROOM_LAMP`. `f` : position dans la cellule (0..1),
// `ray` : rayon de vue en espace tangent (mètres, pas forcément unitaire), `cell` : (travée, étage), `seed` : immeuble et face.
fn room_shade(f: vec2<f32>, ray: vec3<f32>, cell: vec2<f32>, seed: f32, time: f32) -> f32 {
    let id = cell + vec2<f32>(seed * 0.17, seed * 0.31);
    let depth = mix(ROOM_DEPTH_MIN_M, ROOM_DEPTH_MAX_M, h21(id + 3.3));
    var d = ray / vec3<f32>(BAY_WIDTH, FLOOR_HEIGHT, depth);
    d.z = max(d.z, 1.0e-4);
    let p = vec3<f32>(f, 0.0);
    let t3 = room_exit(p, d);
    let t = min(t3.x, min(t3.y, t3.z));
    let q = p + d * t;

    // La paroi touchée.
    // Mur du fond : plus clair vers le plafond, avec une porte tirée au sort d'un côté ou de l'autre.
    let door_x = mix(0.2, 0.8, step(0.5, h21(id + 19.0)));
    let door = step(abs(q.x - door_x), 0.13) * step(q.y, 0.78);
    var shade = ROOM_BACK * (0.75 + 0.25 * q.y) * mix(1.0, ROOM_DOOR, door);
    if t3.y <= t3.x && t3.y <= t3.z {
        // Plafond (rayon montant) et sa rampe lumineuse dans l'axe de la pièce, ou sol.
        let lamp = 1.0 - smoothstep(ROOM_LAMP_HALF, 2.0 * ROOM_LAMP_HALF, abs(q.x - 0.5));
        shade = select(ROOM_FLOOR, ROOM_CEILING + ROOM_LAMP * lamp, d.y > 0.0);
    } else if t3.x < t3.z {
        // Le mur que la rampe éclaire de face est un peu plus clair que l'autre.
        shade = ROOM_SIDE * mix(0.8, 1.2, step(0.5, q.x));
    }
    // La lumière tombe avec la profondeur.
    shade *= mix(1.0, ROOM_DEPTH_FALL, q.z);

    // Mobilier : trois meubles de hauteurs tirées au sort, sur un plan à 60 % de la profondeur.
    let t_f = ROOM_FURNITURE_Z / d.z;
    if t_f < t {
        let g = p.xy + d.xy * t_f;
        let slot = floor(g.x * 3.0);
        let present = step(0.4, h21(id + vec2<f32>(slot * 5.1, 11.0)));
        let top = 0.12 + 0.45 * h21(id + vec2<f32>(slot * 3.7, 23.0));
        shade *= mix(1.0, ROOM_SHADOW, present * step(g.y, top));
    }
    // Habitant : un buste et une tête, sur un plan à 45 %, qui va et vient en 48 s.
    let t_p = ROOM_PERSON_Z / d.z;
    if t_p < t && h21(id + 71.0) < ROOM_PERSON_SHARE {
        let g = p.xy + d.xy * t_p;
        let x0 = 0.5 + 0.28 * sin(6.2831853 * (time / ROOM_PACE_S + h21(id + 5.0)));
        let body = step(abs(g.x - x0), 0.16) * step(g.y, 0.6);
        let head_v = (g - vec2<f32>(x0, 0.665)) / vec2<f32>(0.085, 0.046);
        let head = step(dot(head_v, head_v), 1.0);
        shade *= mix(1.0, ROOM_SHADOW, max(body, head));
    }
    // Store : des lames horizontales sur la vitre, baissées jusqu'à une hauteur tirée au sort.
    if h21(id + 131.0) < ROOM_BLIND_SHARE {
        let drop = 0.3 + 0.6 * h21(id + 137.0);
        let slat = step(0.5, fract(f.y * 16.0));
        shade *= mix(1.0, mix(1.0, ROOM_SHADOW, slat), step(1.0 - drop, f.y));
    }
    // Montant de la vitre.
    shade *= mix(1.0, ROOM_SHADOW, step(abs(f.x - 0.5), ROOM_MULLION_HALF));
    return shade;
}
