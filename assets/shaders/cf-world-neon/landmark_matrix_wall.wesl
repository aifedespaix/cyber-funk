// Le mur de code : quatre plans en profondeur, calculés dans le fragment, décalés par parallaxe selon la position de la
// caméra dans le repère de la façade (le rayon de vue est prolongé jusqu'à la profondeur du plan). Quand la voiture passe,
// les plans lointains glissent derrière les proches ; ils sont adoucis et ternes (flou de profondeur), le premier est net.
// La variante « binaire » ajoute des tranches décalées au rythme des temps et une barre de balayage.

@vertex
fn vertex(v: PlaneIn) -> PlaneOut {
    return plane_vertex(v);
}

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let time = p.frame.x;
    let pulse = p.frame.y;
    let beat = p.audio.y;
    let intensity = p.audio.z;
    let width = p.b.x;
    let variant = p.b.y;
    let flux = variant > 0.5 && variant < 1.5;
    let binary = variant > 1.5;
    let ray = in.local - in.cam;
    var col = color(C_INK) * 0.9;

    var depth = array<f32, 4>(0.0, 1.8, 4.0, 7.0);
    var cell_x = array<f32, 4>(1.5, 1.15, 0.9, 0.7);
    var cell_y = array<f32, 4>(2.1, 1.6, 1.25, 1.0);
    var sharp = array<f32, 4>(1.0, 0.7, 0.4, 0.2);
    var gain = array<f32, 4>(1.0, 0.7, 0.5, 0.35);
    for (var i = 0; i < 4; i++) {
        // Plan i : prolonger le rayon de vue jusqu'à `depth` mètres derrière la façade.
        var q = in.local.xy + ray.xy * (depth[i] / max(-ray.z, 0.2)) + vec2<f32>(width * 0.5, HEIGHT * 0.5);
        if binary {
            q.x += step(0.86, h11(floor(q.y / 5.0) + floor(beat * 0.5) * 3.1)) * (h11(floor(q.y / 5.0) + 7.0) - 0.5) * 9.0; // max 2 Hz (240 BPM)
        }
        let cell = vec2<f32>(cell_x[i], cell_y[i]) * select(1.0, 1.25, binary);
        // Pluie : colonnes qui tombent. Flux : rangées qui filent sur le côté, dans un sens ou dans l'autre.
        let across = select(q.x, q.y, flux);
        let along = select(q.y, q.x, flux);
        let lane = select(cell.x, cell.y, flux);
        let stride = select(cell.y, cell.x, flux);
        let column = floor(across / lane);
        let seed = h11(column * 1.7 + f32(i) * 31.0);
        let speed = 1.2 + seed * 3.0;
        let dir = select(1.0, select(-1.0, 1.0, seed > 0.5), flux);
        let row_f = along / stride + p.c.x * speed * dir;
        let id = floor(row_f);
        let k = fract(id / (10.0 + seed * 18.0) + seed * 5.0);
        let lum = pow(1.0 - k, 3.0) * (0.35 + 0.65 * step(0.25, seed));
        let head = step(k, 0.06);
        let gid = floor(h21(vec2<f32>(column, id)) * 40.0 + floor(time * (0.6 + seed * 2.0)) * h11(id + column)); // max 2.6 Hz
        let cell_uv = select(vec2<f32>(fract(q.x / cell.x), fract(row_f)), vec2<f32>(fract(row_f), fract(q.y / cell.y)), flux);
        let g = glyph(cell_uv, gid, 1.0 - sharp[i], variant);
        let bright = (lum + head * 0.9 + pulse * 0.25 * lum) * gain[i];
        col += flow_color(along / HEIGHT * 1.3 + time * 1.2 + column * 0.02, bright) * g * select(1.1, 1.6, sharp[i] > 0.5);
    }
    // Balayage : une barre lumineuse monte le long de la façade, plus vite avec l'intensité.
    if binary {
        let scan_y = fract(time * (0.08 + intensity * 0.1)) * HEIGHT;
        let scan = sstep(2.4, 0.0, abs(in.local.y + HEIGHT * 0.5 - scan_y));
        col += flow_color(in.local.y * 0.02 + time * 0.5, 0.45) * scan * (0.5 + pulse);
        col += color(C_ICE) * scan * scan * 0.25;
    }
    return vec4<f32>(city_fog(col, in.world), 1.0);
}
