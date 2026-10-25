// L'écran de la tête d'un robot (`mecha/face.ts`) : un panneau de LED (40 × 32) sur lequel un shader dessine le visage avec
// des fonctions de distance, sans texture. Calme : deux points et un trait. Rythme normal : smiley jaune. Haute intensité :
// visière cyborg au regard qui balaie et dents de scie, ou yeux en croix. Fin de titre : clin d'œil. À chaque changement de
// visage l'image se déchire un instant.
//
// Paramètres : a.x = le visage (0 calme, 1 smiley, 2 cyborg, 3 croix, 4 clin d'œil), a.y = déchirure (0..1, **bornée** par le
// limiteur de flashs). Écart avec Sillage (règle 7) : la déchirure change 3 fois par seconde (20 dans Sillage) et tout est proportionnel à `swap`.

fn sd_box(q: vec2<f32>, b: vec2<f32>) -> f32 {
    let d = abs(q) - b;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

fn sd_seg(q: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = q - a;
    let ba = b - a;
    return length(pa - ba * clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0));
}

fn fill(d: f32) -> f32 {
    return 1.0 - smoothstep(-0.02, 0.03, d);
}

@vertex
fn vertex(v: PlaneIn) -> PlaneOut {
    return plane_vertex(v);
}

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let time = p.frame.x;
    let face = i32(p.a.x + 0.5);
    let swap = p.a.y;
    let res = vec2<f32>(40.0, 32.0);
    let slice = floor(in.uv.y * 16.0);
    let shift = (h11(slice + floor(time * 3.0)) - 0.5) * 0.25 * swap;
    let uv = vec2<f32>(in.uv.x + shift, in.uv.y);
    let q = ((floor(uv * res) + 0.5) / res - 0.5) * vec2<f32>(2.5, 2.0);
    var col = color(C_INK) * 1.2;

    if face == 0 {
        let eyes = fill(min(length(q - vec2<f32>(-0.42, 0.18)), length(q - vec2<f32>(0.42, 0.18))) - 0.07);
        let mouth = fill(sd_box(q - vec2<f32>(0.0, -0.32), vec2<f32>(0.28, 0.035)));
        col += color(C_MINT) * (eyes + mouth) * 1.3;
    } else if face == 1 || face == 4 {
        let disc = fill(length(q) - 0.9);
        let eye_l = fill(length((q - vec2<f32>(-0.32, 0.22)) * vec2<f32>(1.0, 0.62)) - 0.13);
        var eye_r = fill(length((q - vec2<f32>(0.32, 0.22)) * vec2<f32>(1.0, 0.62)) - 0.13);
        if face == 4 {
            // Clin d'œil : l'œil droit devient un arc fermé.
            let w = q - vec2<f32>(0.32, 0.12);
            eye_r = fill(max(abs(length(w) - 0.16) - 0.035, -w.y));
        }
        let smile = fill(max(abs(length(q - vec2<f32>(0.0, 0.12)) - 0.55) - 0.045, q.y + 0.1));
        col += mix(vec3<f32>(0.0), color(C_SUN) * 1.5, disc) * (1.0 - max(max(eye_l, eye_r), smile));
    } else if face == 2 {
        let slit = fill(sd_box(q - vec2<f32>(0.0, 0.2), vec2<f32>(0.8, 0.13)));
        let iris = fill(sd_box(q - vec2<f32>(sin(time * 2.4) * 0.6, 0.2), vec2<f32>(0.07, 0.13)));
        let brow = max(
            fill(sd_seg(q, vec2<f32>(-0.85, 0.55), vec2<f32>(-0.1, 0.38)) - 0.05),
            fill(sd_seg(q, vec2<f32>(0.85, 0.55), vec2<f32>(0.1, 0.38)) - 0.05),
        );
        let jaw = fill(sd_box(q - vec2<f32>(0.0, -0.42), vec2<f32>(0.7, 0.18)));
        let teeth = jaw * (0.35 + 0.65 * step(0.5, fract(q.x * 4.5)));
        col += color(C_ALARM) * (slit * 0.35 + iris * 2.0 + brow + teeth) * 1.3;
    } else {
        var cross = 0.0;
        for (var s = 0; s < 2; s++) {
            let c = vec2<f32>(select(0.42, -0.42, s == 0), 0.2);
            cross = max(cross, max(
                fill(sd_seg(q - c, vec2<f32>(-0.15), vec2<f32>(0.15)) - 0.05),
                fill(sd_seg(q - c, vec2<f32>(-0.15, 0.15), vec2<f32>(0.15, -0.15)) - 0.05),
            ));
        }
        let grin = fill(sd_box(q - vec2<f32>(0.0, -0.4), vec2<f32>(0.5, 0.16)));
        let tongue = fill(sd_box(q - vec2<f32>(0.18, -0.5), vec2<f32>(0.14, 0.1)));
        col += color(C_ROSE) * (cross + grin * 0.45 + tongue) * 1.5;
    }

    // Dalle de LED : un point par case, balayage lent.
    let led = fract(uv * res);
    col *= (0.3 + 0.7 * step(0.12, led.x) * step(0.12, led.y)) * (0.85 + 0.15 * sin(time * 8.0 + in.uv.y * 40.0));
    return vec4<f32>(city_fog(col, in.world), 1.0);
}
