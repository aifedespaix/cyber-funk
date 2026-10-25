// L'arche de data glitchée (`voxel-arch.ts`) : un portique monumental qui enjambe la route, fait de 1 500 voxels flottants.
//
// Un seul maillage fusionné (un cube par voxel) dont toute l'animation est dans le vertex shader. Chaque voxel connaît sa
// place sur l'arche (`tangent.xyz`), sa teinte (`tangent.w`) et un cycle personnel (`color` : phase, agitation, sens de
// rotation, taille) : il s'assemble, tient, puis se désassemble en tournant autour de l'arche en spirale, rapetissé. Au passage
// de la voiture, les voxels proches se dispersent et se recomposent derrière ; un drop fait éclater l'ensemble.
//
// Paramètres : a.x = éclatement (0..1, **déjà borné** par le limiteur de flashs), a.y = éclatement accordé (les voxels
// qui se défont ne clignotent en rouge que pendant l'éclatement, donc seulement si le limiteur l'a accordé), a.z = position de la voiture le long de l'arche (m), c.x = horloge du
// cycle (intégrée côté CPU : `time × (1 + intensité/2)` de Sillage téléportait les voxels à chaque variation de l'intensité),
// c.y = horloge de rotation (temps modulo 60 s : la rotation de Sillage croissait avec l'heure, jusqu'à tourner à des
// dizaines de tours par seconde en fin de set).
//
// Écart avec Sillage (règle 7) : le clignotement rouge changeait 12 fois par seconde ; ici 3, et seulement si permis.

const SPRING: f32 = 19.0;

struct VoxelIn {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(4) place: vec4<f32>,
    @location(5) rnd: vec4<f32>,
}

struct VoxelOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world: vec3<f32>,
    @location(2) tone: f32,
    @location(3) away: f32,
    @location(4) seed: f32,
    @location(5) shade: f32,
}

@vertex
fn vertex(v: VoxelIn) -> VoxelOut {
    let time = p.frame.x;
    let intensity = p.audio.z;
    let burst = p.a.x;
    let car_z = p.a.z;
    let m = get_world_from_local(v.instance_index);
    let orbiter = v.rnd.x < 0.0;
    let cycle = fract(p.c.x + abs(v.rnd.x));
    var built = smoothstep(0.0, 0.22, cycle) * (1.0 - smoothstep(0.74, 0.98, cycle));
    // Au passage de la voiture, les voxels proches se dispersent, puis se recomposent derrière.
    let crossing = 1.0 - smoothstep(6.0, 34.0, abs(car_z));
    built *= 1.0 - crossing * (0.6 + 0.4 * v.rnd.y);
    built *= 1.0 - burst * (0.7 + 0.3 * v.rnd.y);
    if orbiter {
        built = 0.0;
    }
    let away = 1.0 - built;

    let centre = vec3<f32>(0.0, SPRING, 0.0);
    let d = v.place.xyz - centre;
    // Tourbillon autour du cœur de l'arche : l'écart, la rotation et l'altitude croissent avec la dispersion.
    let dir = select(-1.0, 1.0, v.rnd.z > 0.5);
    let spin_t = p.c.y;
    let ang = away * (1.8 + v.rnd.y * 5.0) * dir + spin_t * 0.5 * away * dir;
    let ca = cos(ang);
    let sa = sin(ang);
    var swirl = vec3<f32>(d.x * ca - d.y * sa, d.x * sa + d.y * ca, d.z);
    let widen = 1.0 + away * (0.5 + v.rnd.y * 1.4);
    swirl = vec3<f32>(swirl.x * widen, swirl.y * widen, swirl.z);
    swirl.z += away * ((v.rnd.y - 0.5) * 30.0 + sin(time * 1.3 + v.rnd.w * 6.28) * 3.0);

    let size = (0.55 + v.rnd.w * 0.75) * (0.25 + 0.75 * built) * (1.0 + p.frame.y * 0.3);
    let spin = away * 3.0 * dir + spin_t * 0.7 * away;
    var q = v.position * size;
    q = vec3<f32>(q.x * cos(spin) - q.z * sin(spin), q.y, q.x * sin(spin) + q.z * cos(spin));
    var wp = m * vec4<f32>(centre + swirl + q, 1.0);
    wp.y *= city_grow(wp.xyz);
    var out: VoxelOut;
    out.uv = v.uv;
    out.tone = v.place.w;
    out.seed = v.rnd.y;
    out.away = away;
    out.shade = 0.6 + 0.4 * abs(v.normal.y) + 0.15 * abs(v.normal.x);
    out.world = wp.xyz;
    out.clip = position_world_to_clip(wp.xyz);
    return out;
}

@fragment
fn fragment(in: VoxelOut) -> @location(0) vec4<f32> {
    let time = p.frame.x;
    let pulse = p.frame.y;
    let e = min(in.uv, 1.0 - in.uv);
    let edge = smoothstep(0.0, 0.14, min(e.x, e.y));
    let base = mix(mix(color(C_MINT), color(C_VIOLET), smoothstep(0.0, 0.5, in.tone)), color(C_ROSE), smoothstep(0.5, 1.0, in.tone));
    // Données corrompues : un voxel qui se défait passe en rouge (seulement quand les flashs sont permis, trois fois par
    // seconde au plus).
    let flicker = step(0.55, h11(floor(time * 3.0) + in.seed * 40.0)) * p.a.y; // 3 Hz, amplitude = niveau du limiteur (a.y)
    var col = base * in.shade * (0.55 + 0.35 * edge);
    col = mix(col, color(C_ALARM) * 1.1, smoothstep(0.15, 0.7, in.away) * flicker * 0.75);
    col += color(C_ICE) * (1.0 - edge) * (0.25 + pulse * 0.5);
    return vec4<f32>(col * city_fade(in.world) * (0.7 + (1.0 - in.away) * 0.5), 0.0);
}
