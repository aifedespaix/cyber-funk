// L'atmosphère de la ville (chantier C3 ; DA §2.1 et §3.2, P2 §1.3) : le bloc d'éclairage de scène `SceneLight` et le
// brouillard analytique par fragment. Pas de passe plein écran : les couches n'ont pas de profondeur commune au
// compositeur, donc chaque shader du monde applique lui-même la brume à son fragment.
//
// Ce texte est ajouté au bout des shaders de façade, d'enseigne, de trafic, de repères et de route : tous déclarent le
// même bloc à `@binding(9)`. Il n'importe rien : la position de la caméra est passée en paramètre.
//
// Les positions sont en repère rendu (relatives à l'origine flottante de `cf-render`). Bloc à zéro (tests, démos sans
// `AtmospherePlugin`) : `scene.fog.w` vaut 0 et `atmo_fog` rend le brouillard d'avant, à l'identique.
//
// Les formules ont leur miroir en Rust (`atmosphere/formula.rs`), testé contre une intégration numérique : toute
// modification se reporte des deux côtés (un test compare les constantes).

// Même ordre de champs que `SceneLightParams` (Rust, `ShaderType`). Tout en vec4.
struct SceneLight {
    // xyz : direction vers la lumière principale (le côté de rue dominant), w : rapport principale/remplissage.
    dir_key: vec4<f32>,
    // rgb : lumière principale, w : plafond d'émissif d'une fenêtre (0,9·θ_b).
    c_key: vec4<f32>,
    // rgb : remplissage (la brume éclairée), w : densité visuelle Δ.
    c_fill: vec4<f32>,
    // xyz : direction vers le contre-jour (l'axe de la route).
    dir_rim: vec4<f32>,
    // rgb : contre-jour.
    c_rim: vec4<f32>,
    // x : σ_t·ρ0 au sol (1/m), y : hauteur d'échelle H_f (m), z : anisotropie g, w : part d'atmosphère (0 : brouillard d'avant).
    fog: vec4<f32>,
    // rgb : couleur propre de la brume, w : part des lumières principale et de contre-jour qu'elle diffuse vers l'œil.
    c_fog: vec4<f32>,
    // rgb : teinte de Rayleigh des lointains (normalisée), w : sa force.
    rayleigh: vec4<f32>,
    // Âges (s) des quatre derniers kicks, du plus récent au plus ancien (60 : jamais).
    kick_age: vec4<f32>,
    // Coquille de kick. x : vitesse du front (m/s), y : épaisseur σ (m), z : durée de vie τ (s), w : profondeur 0..1.
    shell: vec4<f32>,
    // xyz : source de la coquille (le sujet), w : altitude du sol.
    source: vec4<f32>,
    // Les quatre tubes proches. xyz : position, w : inutilisé.
    tube_pos: array<vec4<f32>, 4>,
    // rgb : couleur déjà multipliée par l'intensité et par σ_s·ρ.
    tube_col: array<vec4<f32>, 4>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(9) var<uniform> scene: SceneLight;

const ATMO_U_SERIES: f32 = 0.001;
const ATMO_U_MAX: f32 = 30.0;
const ATMO_DEPTH_BELOW_MAX: f32 = 2.0;
const ATMO_TUBE_RADIUS_M: f32 = 1.5;
const ATMO_SHELL_FLOOR: f32 = 0.5;

// (1 − e^{−u})/u, stable autour de 0 (rayon horizontal).
fn atmo_slab(u_in: f32) -> f32 {
    // Très plongeant : e^{−u} déborderait. Très montant : e^{−u} est nul, il reste 1/u.
    let u = max(u_in, -ATMO_U_MAX);
    if u > ATMO_U_MAX {
        return 1.0 / u;
    }
    if abs(u) < ATMO_U_SERIES {
        return 1.0 - 0.5 * u;
    }
    return (1.0 - exp(-u)) / u;
}

// Épaisseur optique d'un rayon dans une brume exponentielle en hauteur.
fn atmo_depth(origin: vec3<f32>, dir: vec3<f32>, dist: f32) -> f32 {
    let h = max(scene.fog.y, 1.0);
    let above = max((origin.y - scene.source.w) / h, -ATMO_DEPTH_BELOW_MAX);
    return scene.fog.x * exp(-above) * dist * atmo_slab(dist * dir.y / h);
}

// Henyey-Greenstein ramenée à un pic de 1 : monter g resserre le lobe sans relever son sommet (pas de flash).
fn atmo_phase(cos_theta: f32, g_in: f32) -> f32 {
    let g = clamp(g_in, 0.0, 0.95);
    let base = max(1.0 + g * g - 2.0 * g * clamp(cos_theta, -1.0, 1.0), 0.0001);
    let one_minus = 1.0 - g;
    return one_minus * one_minus * one_minus * one_minus / (base * sqrt(base));
}

// Lumière diffusée par un tube le long du rayon, en forme close.
fn atmo_tube(origin: vec3<f32>, dir: vec3<f32>, dist: f32, light: vec3<f32>) -> f32 {
    let q = origin - light;
    let b = dot(dir, q);
    let h = sqrt(max(dot(q, q) - b * b, ATMO_TUBE_RADIUS_M * ATMO_TUBE_RADIUS_M));
    return (atan((dist + b) / h) - atan(b / h)) / h;
}

// Coquille de vide que chaque kick pousse devant lui, à la distance r de la source.
fn atmo_dispersion(r: f32) -> f32 {
    var sum = 0.0;
    for (var i = 0; i < 4; i++) {
        let age = scene.kick_age[i];
        let x = (r - scene.shell.x * age) / max(scene.shell.y, 0.1);
        sum += exp(-x * x) * exp(-age / max(scene.shell.z, 0.01));
    }
    return max(1.0 - scene.shell.w * sum, ATMO_SHELL_FLOOR);
}

// Transmittance entre la caméra et le point p.
fn atmo_transmit(p: vec3<f32>, cam: vec3<f32>) -> f32 {
    let ray = p - cam;
    let dist = max(length(ray), 0.001);
    let tau = atmo_depth(cam, ray / dist, dist) * atmo_dispersion(distance(p, scene.source.xyz));
    return exp(-max(tau, 0.0));
}

// Perspective atmosphérique d'un fragment opaque : C·T + C_brume·(1 − T), désaturation au loin, tubes proches.
fn atmo_apply(col: vec3<f32>, p: vec3<f32>, cam: vec3<f32>) -> vec3<f32> {
    let ray = p - cam;
    let dist = max(length(ray), 0.001);
    let dir = ray / dist;
    let tau = atmo_depth(cam, dir, dist) * atmo_dispersion(distance(p, scene.source.xyz));
    let t = exp(-max(tau, 0.0));
    // La brume : sa couleur propre, plus ce qu'elle diffuse vers l'œil de la lumière principale et du contre-jour. Le
    // contre-jour est dans l'axe de la route : c'est lui qui découpe les silhouettes (DA §1.2, règle 2).
    let scatter = scene.c_key.rgb * atmo_phase(dot(dir, scene.dir_key.xyz), scene.fog.z)
        + scene.c_rim.rgb * atmo_phase(dot(dir, scene.dir_rim.xyz), scene.fog.z);
    var mist = scene.c_fog.rgb + scatter * scene.c_fog.w;
    // Rayleigh ne sert qu'à teinter : la brume tire vers l'azur avec la distance.
    mist *= mix(vec3<f32>(1.0), scene.rayleigh.rgb, scene.rayleigh.w * (1.0 - t));
    let luma = dot(col, vec3<f32>(0.2126, 0.7152, 0.0722));
    let muted = mix(vec3<f32>(luma), col, 0.4 + 0.6 * t);
    var out = muted * t + mist * (1.0 - t);
    for (var i = 0; i < 4; i++) {
        out += scene.tube_col[i].rgb * atmo_tube(cam, dir, dist, scene.tube_pos[i].xyz);
    }
    return out;
}

// Point d'entrée des shaders opaques. `col` est la couleur déjà passée par le fondu de distance du shader (vers sa
// teinte de fond, presque noire) : ce fondu efface le détail au loin et laisse une silhouette, que la brume découpe
// ensuite (DA §1.2 : « contour seul » à l'horizon, plus sombre que la brume derrière). Bloc à zéro : `col` ressort tel quel.
fn atmo_fog(col: vec3<f32>, p: vec3<f32>, cam: vec3<f32>) -> vec3<f32> {
    if scene.fog.w <= 0.0 {
        return col;
    }
    return mix(col, atmo_apply(col, p, cam), scene.fog.w);
}

// Point d'entrée des shaders additifs (hologrammes, faisceaux) : la lumière émise est seulement atténuée.
fn atmo_fade(p: vec3<f32>, cam: vec3<f32>) -> f32 {
    return mix(1.0, atmo_transmit(p, cam), scene.fog.w);
}
