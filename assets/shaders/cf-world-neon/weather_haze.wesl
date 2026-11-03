// La brume basse : de fins voiles de brouillard qui rampent à ras de la chaussée, entre la route et le ciel. Là où
// `weather_mist` cache le vide sous la route, celle-ci donne de l'épaisseur à l'air : la route n'est plus un tracé net,
// elle baigne dans une vapeur qui file, s'épaissit au loin et se teinte des néons.
//
// Plusieurs plans superposés (un par hauteur) partagent ce shader : `layer.x` est le numéro de la couche, qui décale le
// motif et sa vitesse, pour que les voiles se croisent (parallaxe) au lieu de se confondre. Blend alpha, sans lumière ni
// ombre. Le motif est calculé en position monde rendue ; il dérive dans le sens de la route (vers +z, vers nous) plus vite
// que le vent latéral, ce qui donne la sensation de vitesse.

import bevy_pbr::render::forward_io::VertexOutput;
import bevy_pbr::render::mesh_view_bindings::view;

struct Haze {
    // Couleur de la brume (lumière linéaire) : encre tirée vers la teinte d'ambiance.
    tint: vec4<f32>,
    // Teinte d'ambiance vive, pour les nappes les plus denses.
    glow: vec4<f32>,
    // x : temps (s), y : pouls du kick, z : densité (0 à 1, la qualité et la pluie), w : 1 = bruit allégé.
    frame: vec4<f32>,
    // x : numéro de la couche, y : opacité maximale de la couche, z : hauteur du plan (m), w : vitesse de la voiture (m/s).
    layer: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> haze: Haze;

fn h2(p_in: vec2<f32>) -> f32 {
    var p = fract(p_in * vec2<f32>(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

fn n2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    var f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(h2(i), h2(i + vec2<f32>(1.0, 0.0)), f.x),
        mix(h2(i + vec2<f32>(0.0, 1.0)), h2(i + vec2<f32>(1.0, 1.0)), f.x),
        f.y,
    );
}

fn fbm(p_in: vec2<f32>, octaves: i32) -> f32 {
    var p = p_in;
    var v = 0.0;
    var a = 0.5;
    for (var i = 0; i < octaves; i++) {
        v += a * n2(p);
        p = p * 2.03 + 7.1;
        a *= 0.5;
    }
    return v;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let time = haze.frame.x;
    let layer = haze.layer.x;
    let world = in.world_position.xz;

    // Les voiles sont étirés dans le sens de la route (x resserré, z allongé) : des filaments, pas des taches.
    let stretch = vec2<f32>(0.055, 0.016) * (1.0 + layer * 0.35);
    // Chaque couche dérive à sa vitesse, un peu de travers : elles se croisent.
    let drift = vec2<f32>(sin(layer * 2.1 + time * 0.05) * 0.8, 1.6 + layer * 0.9) * time;
    let p = (world + drift) * stretch + vec2<f32>(layer * 13.7, layer * 5.3);

    // Relief du nuage : bruit « bouffant » (la valeur absolue plisse les crêtes), plus net que le bruit lisse.
    var cloud: f32;
    if haze.frame.w > 0.5 {
        cloud = fbm(p, 3);
    } else {
        // Un second échelon, lent, qui plisse le premier : la brume respire.
        let warp = vec2<f32>(n2(p * 0.7 + time * 0.03), n2(p * 0.7 - time * 0.04)) - 0.5;
        let q = p + warp * 1.4;
        let billow = 1.0 - abs(fbm(q * 1.6 + 3.0, 3) * 2.0 - 1.0);
        cloud = mix(fbm(q, 4), billow, 0.45);
    }

    // Couverture à grande échelle : de vrais bancs de brume épais, séparés de trouées presque claires. Elle varie d'une
    // couche à l'autre et dérive lentement, donc la route traverse tantôt un banc, tantôt une éclaircie.
    let coverage_p = (world + drift * 0.35) * vec2<f32>(0.0045, 0.0016) + vec2<f32>(layer * 31.0, layer * 9.0);
    let coverage = smoothstep(0.30, 0.68, fbm(coverage_p, 3));
    // Dans un banc la brume est dense et pleine ; dans une trouée il n'en reste que de fins lambeaux.
    let thickness = mix(0.3, 1.0, coverage);
    let body = smoothstep(mix(0.52, 0.38, coverage), mix(0.76, 0.62, coverage), cloud);

    // Distance à l'œil : rien tout contre la caméra (sinon un mur laiteux), plein effet à mi-distance, fondu au loin
    // pour ne pas dessiner le bord du plan.
    let dist = length(in.world_position.xyz - view.world_position);
    let near = smoothstep(2.0, 16.0, dist);
    let far = 1.0 - smoothstep(180.0, 420.0, dist);

    // Une route qui file plus vite lève plus de vapeur.
    let speed_k = 0.75 + 0.25 * clamp(haze.layer.w / 60.0, 0.0, 1.0);
    let alpha = body * thickness * near * far * haze.layer.y * haze.frame.z * speed_k;

    // Les nappes les plus denses s'allument de la teinte d'ambiance, au rythme du kick.
    let lit = smoothstep(0.55, 0.95, cloud) * coverage * (0.35 + haze.frame.y * 0.45);
    let col = mix(haze.tint.rgb, haze.glow.rgb, lit);
    return vec4<f32>(col, clamp(alpha, 0.0, 0.5));
}
