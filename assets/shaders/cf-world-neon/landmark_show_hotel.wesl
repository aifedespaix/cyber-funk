// L'hôtel de verre (`showcase/hotel.ts`) : chaque chambre est une scène (rideaux, fêtes en couleur, silhouettes qui dansent
// au tempo), un reflet en diagonale, un hall éclairé, une couronne néon. Un drop allume toutes les chambres d'un coup, par
// le niveau borné du limiteur (règle 7) : `drop` monte en 0,1 s au plus et retombe en 0,8 s.

const SPAN: f32 = 36.0;
const HEIGHT: f32 = 88.0;
const ROOM_W: f32 = 4.5;
const ROOM_H: f32 = 5.5;

@fragment
fn fragment(in: PlaneOut) -> @location(0) vec4<f32> {
    let s = show();
    let time = s.time;
    let q = vec2<f32>((in.uv.x - 0.5) * SPAN, in.uv.y * HEIGHT);
    var col = s.ink * 0.85;

    let cell = floor(vec2<f32>((q.x + SPAN / 2.0) / ROOM_W, q.y / ROOM_H));
    let lcell = vec2<f32>(fract((q.x + SPAN / 2.0) / ROOM_W), fract(q.y / ROOM_H));
    let lp = (lcell - 0.5) * vec2<f32>(ROOM_W, ROOM_H);
    let half_room = vec2<f32>(ROOM_W / 2.0 - 0.4, ROOM_H / 2.0 - 0.6);
    let room = rect(lp, vec2<f32>(0.0), half_room);
    let rnd = h21(cell + 5.0);
    // Chaque chambre change d'humeur de temps en temps (toutes les ~8 s, décalées).
    let mood = h21(cell + floor(time / 8.0 + rnd * 8.0) * 1.7);
    let party = step(0.82, mood);
    let lit = step(0.35, mood) + s.drop;
    let tone = select(mix(s.b, s.d, h21(cell + 2.0) * 0.5), mix(s.c, s.b, h21(cell + 1.0)), party > 0.5);
    var interior = tone * (0.12 + 0.2 * s.mid) * lit;
    // Rideau à moitié tiré dans certaines pièces.
    let curtain = step(0.7, h21(cell + 8.0)) * step(lp.x, -0.2);
    interior *= 1.0 - curtain * 0.65;
    col = mix(col + s.a * 0.01, interior + s.a * 0.02, room);

    // Danseur : tête, buste, deux bras et deux jambes, qui se plient au tempo ; seulement dans les chambres en fête.
    if party > 0.5 || (s.drop > 0.3 && rnd > 0.55) {
        let ph = 3.14159 * (s.beat + s.phase) + rnd * 6.0;
        let sway = sin(ph) * 0.45;
        let arms = cos(ph) * 0.9;
        let hip = vec2<f32>(sway * 0.4, -0.75);
        let chest = hip + vec2<f32>(sway * 0.3, 1.4);
        let torso = sd_seg(lp, hip, chest) - 0.2;
        let head = length(lp - (chest + vec2<f32>(sway * 0.2, 0.55))) - 0.3;
        let arm_l = sd_seg(lp, chest, chest + vec2<f32>(-0.8, arms)) - 0.09;
        let arm_r = sd_seg(lp, chest, chest + vec2<f32>(0.8, -arms)) - 0.09;
        let leg_l = sd_seg(lp, hip, hip + vec2<f32>(-0.4 - sway * 0.2, -0.9)) - 0.1;
        let leg_r = sd_seg(lp, hip, hip + vec2<f32>(0.4 - sway * 0.2, -0.9)) - 0.1;
        let dancer = soft(min(min(torso, head), min(min(arm_l, arm_r), min(leg_l, leg_r))), 0.06) * room;
        col = mix(col, s.ink * 0.15, dancer * 0.85);
    }
    // Montants : un cadre sombre, une arête claire.
    col += s.a * soft(abs(sd_box(lp, vec2<f32>(0.0), half_room)) - 0.05, 0.05) * 0.18;

    // Reflet : une bande lumineuse qui traverse la tour en diagonale.
    let sheen = smoothstep(0.9, 1.0, sin((q.x * 0.5 + q.y * 0.35) - time * 0.5));
    col += s.d * 0.06 * sheen * (0.4 + 0.6 * room);
    // Hall : tout le pied est éclairé, avec une marquise.
    let lobby = step(q.y, 8.0);
    col = mix(col, s.b * (0.3 + 0.2 * s.pulse) + s.a * 0.05, lobby * 0.7);
    col += s.b * soft(abs(q.y - 8.0) - 0.3, 0.1) * (0.8 + s.pulse);
    // Couronne : une enseigne néon sur le toit.
    col += s.c * soft(abs(q.y - (HEIGHT - 2.5)) - 0.35, 0.12) * step(abs(q.x), 14.0) * (0.7 + s.pulse * 1.5);

    return vec4<f32>(city_fog(col, in.world), 1.0);
}
