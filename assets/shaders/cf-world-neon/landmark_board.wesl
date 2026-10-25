// La tour « Carte mère » (`board-tower.ts`) : la façade est une carte vue de dessus, tout procédural dans un fragment.
//
// Huit faisceaux de cinq pistes de cuivre courent à la verticale, par tronçons de 14 m : à chaque jonction un faisceau peut
// glisser d'une piste en biais, ou s'arrêter. Des puces occupent une grille de sites et masquent les pistes dessous ; un
// socket de processeur trône au centre. Les signaux sont des fronts lumineux dont la phase ne dépend que de la hauteur : ils
// suivent les pistes, y compris dans leurs décalages, et montent de bas en haut.
//
// Paramètres : b.x = défilement des signaux (intégré côté CPU : `time × speed` de Sillage téléportait les signaux à chaque
// variation des basses).

const SPAN: f32 = 36.0;
const HEIGHT: f32 = 84.0;
const BUNDLES: i32 = 8;
const SEGMENT: f32 = 14.0;
const SOCKET_Y: f32 = 40.0;

fn rect(q: vec2<f32>, c: vec2<f32>, h: vec2<f32>) -> f32 {
    let d = abs(q - c) - h;
    return 1.0 - step(0.0, max(d.x, d.y));
}

@vertex
fn vertex(v: PlaneIn) -> PlaneOut {
    return plane_vertex(v);
}

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let time = p.frame.x;
    let pulse = p.frame.y;
    let intensity = p.audio.z;
    let beat = p.audio.y;
    let scroll = p.b.x;
    let copper = color(C_AMBER);
    let gold = color(C_SUN);
    let sig_a = color(C_MINT);
    let sig_b = color(C_AZURE);
    let ink = color(C_INK);
    let ice = color(C_ICE);
    let q = vec2<f32>((in.uv.x - 0.5) * SPAN, in.uv.y * HEIGHT);
    var col = ink * 0.85 + color(C_VIOLET) * 0.012;

    // Pistes : huit faisceaux de cinq, décalés d'une piste aux jonctions.
    let seg = floor(q.y / SEGMENT);
    let seg_f = q.y / SEGMENT - seg;
    var trace = 0.0;
    var signal = 0.0;
    for (var b = 0; b < BUNDLES; b++) {
        let fb = f32(b);
        let here = step(h11(fb * 7.3 + seg * 3.1), 0.64);
        let next = step(h11(fb * 7.3 + (seg + 1.0) * 3.1), 0.64);
        let shift_here = floor(h11(fb * 3.3 + seg * 5.7) * 3.0) - 1.0;
        let shift_next = floor(h11(fb * 3.3 + (seg + 1.0) * 5.7) * 3.0) - 1.0;
        let jog = smoothstep(0.82, 1.0, seg_f) * next;
        let cx = -SPAN * 0.5 + (fb + 0.5) * (SPAN / f32(BUNDLES)) + mix(shift_here, shift_next, jog) * 0.7;
        let lx = q.x - cx;
        let lane = clamp(floor(lx / 0.7 + 0.5), -2.0, 2.0);
        let line = sstep(0.12, 0.07, abs(lx - lane * 0.7)) * step(abs(lx), 1.75) * here;
        trace = max(trace, line);
        let lane_id = fb * 5.0 + lane;
        let phase = fract(q.y * 0.025 - scroll * (0.6 + h11(lane_id) * 0.8) + h11(lane_id * 1.7));
        signal = max(signal, line * exp(-(1.0 - phase) * 16.0));
    }

    // Vias : petits anneaux dorés sur une grille irrégulière.
    let vc = floor(q / 3.0);
    let vf = fract(q / 3.0) - 0.5;
    let via = step(0.9, h21(vc + 3.0)) * sstep(0.16, 0.12, abs(length(vf) - 0.14));

    // Puces : un site par case de 18 × 15 m, boîtier noir, pattes sur les longs côtés.
    let cs = vec2<f32>(18.0, 15.0);
    let ci = floor((q + vec2<f32>(SPAN * 0.5, 0.0)) / cs);
    let cc = (ci + 0.5) * cs - vec2<f32>(SPAN * 0.5, 0.0) + (vec2<f32>(h21(ci + 1.0), h21(ci + 2.0)) - 0.5) * 4.0;
    let half_size = vec2<f32>(3.0 + h21(ci + 3.0) * 3.0, 2.2 + h21(ci + 4.0) * 2.6);
    let has_chip = step(0.42, h21(ci + 5.0)) * (1.0 - rect(q, vec2<f32>(0.0, SOCKET_Y), vec2<f32>(14.0)));
    let chip = rect(q, cc, half_size) * has_chip;
    var pins = rect(q, cc, half_size + vec2<f32>(0.0, 0.5)) * (1.0 - rect(q, cc, half_size + vec2<f32>(0.0, -0.05))) * step(0.5, fract(q.x / 0.9));
    pins += rect(q, cc, half_size + vec2<f32>(0.5, 0.0)) * (1.0 - rect(q, cc, half_size + vec2<f32>(-0.05, 0.0))) * step(0.5, fract(q.y / 0.9));
    pins *= has_chip;
    let notch = has_chip * (1.0 - step(0.45, length(q - (cc + half_size * vec2<f32>(-0.75, 0.7)))));

    // Condensateurs : disques à anneau, un peu partout hors puces.
    let kc = floor(q / 3.2);
    let kp = fract(q / 3.2) - 0.5;
    let cap_on = step(0.88, h21(kc + 9.0)) * (1.0 - chip);
    let cap = cap_on * (sstep(0.26, 0.22, length(kp)) - sstep(0.15, 0.11, length(kp)));

    // Socket du processeur : cadre, grille de broches, couvercle qui scintille.
    let sp = q - vec2<f32>(0.0, SOCKET_Y);
    let socket = rect(q, vec2<f32>(0.0, SOCKET_Y), vec2<f32>(7.5));
    let lid = rect(q, vec2<f32>(0.0, SOCKET_Y), vec2<f32>(5.2));
    let grid = socket * (1.0 - lid) * sstep(0.2, 0.15, length(fract(sp / 0.8) - 0.5));
    let shimmer = 0.5 + 0.5 * sin(sp.x * 0.7 + sp.y * 0.4 + time * 0.9 + beat * 1.5);

    col += copper * trace * 0.55 * (1.0 - chip) * (1.0 - socket);
    col += gold * (via * 0.5 + cap * 0.55 + pins * 0.45 + grid * 0.5);
    col = mix(col, ink * 0.5 + ice * 0.025, max(chip, socket * (1.0 - lid)) * 0.9);
    col += ice * notch * 0.2;
    col = mix(col, ink * 0.7 + ice * (0.05 + 0.1 * shimmer) + p.mood.rgb * 0.03 * pulse, lid);
    col += mix(sig_b, sig_a, pulse) * signal * (1.0 - chip) * (1.0 - socket) * (1.2 + intensity * 2.8 + pulse * 1.2);
    // Le cœur du processeur respire avec la grosse caisse.
    col += sig_a * lid * (0.05 + pulse * 0.35 + intensity * 0.12);

    return vec4<f32>(city_fog(col, in.world), 1.0);
}
