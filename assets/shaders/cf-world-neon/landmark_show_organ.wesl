// La cathédrale d'orgue (`showcase/organ.ts`) : 21 tuyaux dont chacun monte avec une bande du son (graves, médiums, aigus
// en alternance), sous une rosace de vitraux qui tournent. Aucune horloge propre : le souffle suit l'énergie de la bande.

const SPAN: f32 = 44.0;
const HEIGHT: f32 = 84.0;
const PIPES: f32 = 21.0;

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let s = show();
    let time = s.time;
    let q = vec2<f32>((in.uv.x - 0.5) * SPAN, in.uv.y * HEIGHT);
    var col = s.ink * 0.85;

    let w = SPAN / PIPES;
    let id = floor((q.x + SPAN * 0.5) / w);
    let cx = (id + 0.5) * w - SPAN * 0.5;
    let lx = q.x - cx;
    let c = (id + 0.5) / PIPES * 2.0 - 1.0;
    let top = HEIGHT * (0.26 + 0.4 * (1.0 - c * c) + 0.04 * h11(id));
    let band = gl_mod(id, 3.0);
    var energy = select(select(s.high, s.mid, band < 1.5), s.bass, band < 0.5);
    // Un souffle au repos : même sans musique le buffet respire.
    let idle = 0.2 + 0.12 * sin(time * 0.9 + id * 0.7);
    energy = clamp(energy * 1.25 + s.intensity * 0.35 + s.drop * 0.5 + idle, 0.0, 1.0);

    // Buffet : arcade gothique derrière les tuyaux, fond de pierre sombre.
    let arch = sstep(0.5, 0.0, abs(length(vec2<f32>(q.x * 0.8, q.y - HEIGHT * 0.5)) - 31.0));
    col += s.b * arch * 0.05;

    let inside = step(abs(lx), w * 0.4) * step(q.y, top);
    let curve = 0.5 + 0.5 * cos(lx / (w * 0.4) * 1.5);
    var metal = s.a * (0.05 + 0.3 * curve);
    // Le métal accroche la lumière en haut : un reflet qui glisse avec le temps.
    metal += s.a * 0.12 * smoothstep(0.9, 1.0, sin(q.y * 0.35 - time * 0.6 + id));
    col = mix(col, metal, inside);

    // Souffle : la lumière monte dans le tuyau jusqu'à sa hauteur d'énergie, plus vive à la pointe.
    let fill = top * energy;
    let air = inside * (1.0 - smoothstep(fill - 3.0, fill, q.y)) * (0.35 + 0.65 * curve);
    col += mix(s.b, s.c, energy) * air * (0.7 + s.pulse * 0.8);
    col += s.c * inside * exp(-abs(q.y - fill) * 1.1) * energy * 1.6;

    // Bouche du tuyau : une fente claire qui s'ouvre avec l'énergie.
    let mouth = rect(vec2<f32>(lx, q.y), vec2<f32>(0.0, top * 0.2), vec2<f32>(w * 0.3, 0.25 + energy * 0.9));
    col += s.d * mouth * inside * (0.4 + energy * 2.2);
    // Couronne de la pointe.
    col += s.a * inside * smoothstep(top - 0.5, top, q.y) * 0.8;

    // Rosace : douze fuseaux de vitrail qui tournent, un cœur qui bat.
    let rp = q - vec2<f32>(0.0, HEIGHT * 0.83);
    let r = length(rp);
    let ang = atan2(rp.y, rp.x) + time * 0.12 + s.drop * 1.5;
    let spoke = floor(ang / 0.5236);
    let glass = step(r, 9.0) * step(1.6, r);
    let hue = h11(spoke * 3.7);
    let pane = mix(mix(s.b, s.c, hue), mix(s.d, s.a, hue), step(0.5, h11(spoke + 9.0)));
    let lead = sstep(0.1, 0.0, abs(fract(ang / 0.5236) - 0.5) - 0.44) + sstep(0.3, 0.0, abs(r - 5.5) - 0.2) * 0.5;
    col = mix(col, pane * (0.35 + 0.5 * s.mid + s.pulse * 0.5) * (1.0 - lead * 0.8), glass);
    col += s.a * step(r, 1.6) * (0.5 + s.pulse * 2.0);
    col += s.a * sstep(0.35, 0.0, abs(r - 9.2)) * 0.7;

    return vec4<f32>(city_fog(col, in.world), 1.0);
}
