//! Escadrille de drones : une trentaine de drones arrivent de derrière en colonne, comme des voitures qui se suivent, en
//! tirebouchonnant le long de leur route. Ils doublent la voiture, freinent au niveau d'un de ses flancs et dessinent une
//! **forme** dans un plan parallèle à la route (anneau, cœur, étoile, flèche, smiley, infini, losange) ; elle tient quelques
//! secondes en respirant au tempo, puis les drones repartent droit devant, en accélérant, et s'éteignent au loin.
//!
//! Le tirage change la forme, le côté, l'ampleur, la hauteur, le nombre de drones et la durée de la tenue : deux passages ne
//! se ressemblent pas.
//!
//! # Mécanique
//!
//! Chaque drone `i` a sa case dans la forme ([`slots`]) et un retard `i × LAG_S` sur le premier : il suit le même chemin que
//! lui, plus tard (la colonne). Sa position est un mélange de deux positions, la colonne et sa case, que la part `morph`
//! fait passer de l'une à l'autre au moment où le drone arrive. Au départ, chaque drone accélère à son tour, du dernier au
//! premier. Tout est une fonction du temps ([`Formation::position`]) : rien à simuler, rien qui dérive.
//!
//! # Règles tenues
//!
//! - les drones sont des points lumineux (`Glints`) : aucun véhicule, aucune entité, 64 points au plus ;
//! - la respiration de la forme suit le pouls que le limiteur laisse passer (`SceneFx::kick`), jamais le kick brut
//!   (règle 7) ; l'éclat de chaque drone reste quasi constant.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec2, Vec3};

use crate::pilot::flight::Band;
use crate::traffic::event::{EventCtx, TrafficEvent};
use crate::traffic::events::common::{index, range, smooth};

/// Retard entre deux drones consécutifs de la colonne, s.
const LAG_S: f32 = 0.085;
/// Durée de l'approche du premier drone, s.
const APPROACH_S: f32 = 7.0;
/// Distance de départ derrière sa case, m.
const START_BEHIND_M: f32 = 210.0;
/// Rayon des tirebouchons de la colonne, m.
const HELIX_R_M: f32 = 1.8;
/// Accélération au départ, m/s².
const LEAVE_ACCEL: f32 = 7.5;
/// Durée de la fuite après le départ du dernier, s.
const LEAVE_S: f32 = 6.0;

type Slots = Vec<Vec2>;

/// Les formes possibles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    Ring,
    Heart,
    Star,
    Arrow,
    Smiley,
    Infinity,
    Diamond,
}

pub(crate) const SHAPES: [Shape; 7] = [
    Shape::Ring,
    Shape::Heart,
    Shape::Star,
    Shape::Arrow,
    Shape::Smiley,
    Shape::Infinity,
    Shape::Diamond,
];

/// `count` points régulièrement répartis (par longueur) sur le contour fermé de `verts`.
fn sample_loop(verts: &[Vec2], count: usize, phase: f32, out: &mut Slots) {
    let n = verts.len();
    let edge = |i: usize| verts[(i + 1) % n] - verts[i];
    let total: f32 = (0..n).map(|i| edge(i).length()).sum();
    for k in 0..count {
        let mut want = total * (k as f32 + phase) / count as f32;
        let mut point = verts[0];
        for (i, vert) in verts.iter().enumerate() {
            let len = edge(i).length();
            if want <= len || i == n - 1 {
                point = *vert + edge(i) * (want / len.max(1e-6)).min(1.0);
                break;
            }
            want -= len;
        }
        out.push(point);
    }
}

/// `count` points sur un arc de cercle de centre `c`, rayon `r`, de `a0` à `a1` (le dernier point est à `a1` si l'arc est
/// ouvert, et on s'arrête avant `a1` s'il est fermé).
fn sample_arc(c: Vec2, r: f32, a0: f32, a1: f32, count: usize, closed: bool, out: &mut Slots) {
    let steps = if closed { count } else { count.saturating_sub(1).max(1) };
    for k in 0..count {
        let a = a0 + (a1 - a0) * k as f32 / steps as f32;
        out.push(c + Vec2::new(a.cos(), a.sin()) * r);
    }
}

/// Les cases d'une forme : `count` points `(a, b)`, `a` vers l'avant de la route et `b` vers le haut, dans un carré de
/// rayon `r` (m) autour du centre.
pub(crate) fn slots(shape: Shape, count: usize, r: f32) -> Slots {
    let mut out = Vec::with_capacity(count);
    match shape {
        Shape::Ring => sample_arc(Vec2::ZERO, r, 0.0, TAU, count, true, &mut out),
        Shape::Heart => {
            // La courbe classique, rééchantillonnée par longueur : au pli du haut et à la pointe du bas, un échantillonnage
            // par angle entasserait les drones.
            let verts: Vec<Vec2> = (0..240)
                .map(|k| {
                    let t = k as f32 / 240.0 * TAU;
                    let x = 16.0 * t.sin().powi(3);
                    let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
                    Vec2::new(x, y + 1.5) * (r / 16.5)
                })
                .collect();
            sample_loop(&verts, count, 0.0, &mut out);
        }
        Shape::Star => {
            let verts: Vec<Vec2> = (0..10)
                .map(|i| {
                    let a = FRAC_PI_2 + i as f32 * PI / 5.0;
                    Vec2::new(a.cos(), a.sin()) * if i % 2 == 0 { r } else { r * 0.42 }
                })
                .collect();
            sample_loop(&verts, count, 0.0, &mut out);
        }
        Shape::Arrow => {
            let verts = [
                Vec2::new(1.0, 0.0),
                Vec2::new(0.1, 0.9),
                Vec2::new(0.1, 0.38),
                Vec2::new(-1.0, 0.38),
                Vec2::new(-1.0, -0.38),
                Vec2::new(0.1, -0.38),
                Vec2::new(0.1, -0.9),
            ]
            .map(|v| v * r);
            sample_loop(&verts, count, 0.0, &mut out);
        }
        Shape::Smiley => {
            // Le visage (55 %), deux yeux de deux drones, le reste pour le sourire.
            let face = count * 55 / 100;
            let eyes = 4.min(count.saturating_sub(face));
            let smile = count - face - eyes;
            sample_arc(Vec2::ZERO, r, 0.0, TAU, face, true, &mut out);
            for cx in [-0.34, 0.34] {
                for dy in [0.34, 0.52].into_iter().take(eyes / 2) {
                    out.push(Vec2::new(cx, dy) * r);
                }
            }
            sample_arc(
                Vec2::new(0.0, 0.05 * r),
                0.58 * r,
                PI * 1.12,
                PI * 1.88,
                smile,
                false,
                &mut out,
            );
        }
        Shape::Infinity => {
            let verts: Vec<Vec2> = (0..240)
                .map(|k| {
                    let t = k as f32 / 240.0 * TAU;
                    let d = 1.0 + t.sin().powi(2);
                    Vec2::new(t.cos() / d, t.sin() * t.cos() / d) * r * 1.15
                })
                .collect();
            // Décalé d'un tiers de pas : le croisement du huit tombe entre deux drones, jamais sous l'un d'eux.
            sample_loop(&verts, count, 0.37, &mut out);
        }
        Shape::Diamond => {
            let verts = [
                Vec2::new(r, 0.0),
                Vec2::new(0.0, r * 0.9),
                Vec2::new(-r, 0.0),
                Vec2::new(0.0, -r * 0.9),
            ];
            sample_loop(&verts, count, 0.0, &mut out);
        }
    }
    debug_assert_eq!(out.len(), count);
    out
}

/// Ce que le tirage décide, une fois à la naissance.
#[derive(Debug, Clone)]
pub(crate) struct Formation {
    pub(crate) shape: Shape,
    slots: Slots,
    /// Côté de la forme : +1 ou −1 en x.
    pub(crate) side: f32,
    /// Distance latérale au centre de la voiture, m.
    lateral: f32,
    /// Hauteur du centre de la forme, m.
    centre_y: f32,
    /// Position de la forme le long de la route par rapport à la voiture, m, vers l'avant (> 0 : devant elle).
    ahead: f32,
    /// Durée de la tenue, s.
    hold_s: f32,
    /// Penchant de la forme sur son plan, rad.
    tilt: f32,
}

impl Formation {
    pub(crate) fn roll(ctx: &mut EventCtx<'_>) -> Self {
        let shape = SHAPES[index(ctx.rng, SHAPES.len())];
        let count = 28 + index(ctx.rng, 9);
        let r = range(ctx.rng, 4.6, 5.6);
        let side = if ctx.rng.coin() { 1.0 } else { -1.0 };
        Self {
            shape,
            slots: slots(shape, count, r),
            side,
            lateral: range(ctx.rng, 11.0, 13.5),
            // Le bas de la forme reste à plus de 2,5 m du sol.
            centre_y: range(ctx.rng, r + 3.5, r + 8.0),
            ahead: range(ctx.rng, 12.0, 24.0),
            hold_s: range(ctx.rng, 5.0, 7.5),
            tilt: range(ctx.rng, -0.2, 0.2),
        }
    }

    pub(crate) fn count(&self) -> usize {
        self.slots.len()
    }

    /// Instant où le premier drone repart, s.
    fn leave_s(&self) -> f32 {
        APPROACH_S + LAG_S * self.count() as f32 + 0.8 + self.hold_s
    }

    /// Durée totale, s.
    pub(crate) fn total(&self) -> f32 {
        self.leave_s() + LAG_S * 0.5 * self.count() as f32 + LEAVE_S
    }

    /// Position du drone `i` à `t`, dans le repère de la voiture : x à droite, y en haut, z vers l'arrière.
    pub(crate) fn position(&self, i: usize, t: f32) -> Vec3 {
        let lag = i as f32 * LAG_S;
        let own = t - lag;
        let progress = (own / APPROACH_S).clamp(0.0, 1.0);
        let behind = START_BEHIND_M * (1.0 - progress) * (1.0 - progress);
        let centre = Vec3::new(self.side * self.lateral, self.centre_y, -self.ahead);
        let morph = smooth(APPROACH_S - 2.0, APPROACH_S + 0.8, own);

        // La colonne : tirebouchon autour de l'axe de marche, qui se resserre en arrivant.
        let spin = t * 2.1 + i as f32 * 0.9;
        let helix = HELIX_R_M * (1.0 - morph);
        let column = centre + Vec3::new(helix * spin.cos(), helix * spin.sin(), behind);

        // La case : dans le plan de la forme, qui respire et penche un peu.
        let breath = 1.0 + 0.025 * (t * 1.6).sin();
        let slot = self.slots[i] * breath;
        let (sin, cos) = self.tilt.sin_cos();
        let along = slot.x * cos - slot.y * sin;
        let height = slot.x * sin + slot.y * cos;
        let target = centre + Vec3::new(0.0, height, -along);

        let mut pos = column.lerp(target, morph);

        // Le départ, du dernier au premier : chacun accélère droit devant.
        let leave_at = self.leave_s() + (self.count() - 1 - i) as f32 * LAG_S * 0.5;
        let tau = (t - leave_at).max(0.0);
        pos.z -= 0.5 * LEAVE_ACCEL * tau * tau;
        pos.y += 0.04 * tau * tau;
        pos
    }

    /// Éclat des drones à `t` : ils s'allument en arrivant et s'éteignent au loin.
    pub(crate) fn envelope(&self, t: f32) -> f32 {
        smooth(0.0, 1.2, t) * (1.0 - smooth(self.total() - 2.2, self.total(), t))
    }
}

#[derive(Debug)]
pub(crate) struct DroneFormationEvent {
    age: f32,
    formation: Formation,
    tints: [LinearRgba; 6],
    base: usize,
}

impl DroneFormationEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        let formation = Formation::roll(ctx);
        tracing::debug!(
            target: "traffic",
            shape = ?formation.shape,
            drones = formation.count(),
            side = formation.side,
            "escadrille de drones"
        );
        let base = index(ctx.rng, ctx.frame.cycle.len());
        Self {
            age: 0.0,
            formation,
            tints: ctx.frame.cycle,
            base,
        }
    }

    /// Le centre de la forme est-il près de la voiture ?
    fn near(&self) -> bool {
        let t = self.age;
        t > APPROACH_S * 0.5 && t < self.formation.total() - LEAVE_S * 0.5
    }
}

impl TrafficEvent for DroneFormationEvent {
    fn proximity(&self, _ctx: &EventCtx<'_>) -> bool {
        self.near()
    }

    /// La forme occupe une tranche de la hauteur à côté de la voiture : on n'y monte pas tant qu'elle y est.
    fn claim(&self, ctx: &EventCtx<'_>) -> Option<Band> {
        self.near().then(|| {
            let y = ctx.frame.car.y + f64::from(self.formation.centre_y);
            Band {
                lo: y - 9.0,
                hi: y + 9.0,
            }
        })
    }

    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.age += dt;
        if self.age > self.formation.total() {
            return false;
        }
        let t = self.age;
        let car = ctx.frame.car;
        let envelope = self.formation.envelope(t);
        // Le pouls que le limiteur laisse passer : la forme respire, elle ne clignote pas.
        let pulse = ctx.scene.fx.kick;
        let n = self.formation.count();
        let hold_from = APPROACH_S + LAG_S * n as f32 + 0.8;
        for i in 0..n {
            let rel = self.formation.position(i, t);
            let at = car + rel.as_dvec3();
            let tint = self.tints[(self.base + i % 3) % self.tints.len()];
            let held = smooth(hold_from - 1.0, hold_from + 0.5, t);
            ctx.glints.add(at, 1.35, tint, (1.9 + pulse * 0.7 * held) * envelope);
            // Un filet de lumière derrière les drones qui filent (arrivée et départ) : on lit leur vitesse.
            let speed_hint = (rel.z - self.formation.position(i, t + 0.05).z) / 0.05; // m/s vers l'avant de la route
            if speed_hint.abs() > 14.0 {
                // `speed_hint` > 0 : le drone avance vers l'avant de la route (z décroît), sa traînée est derrière lui (+z).
                ctx.glints.add(
                    at + DVec3::new(0.0, 0.0, f64::from(speed_hint.signum()) * 1.6),
                    0.9,
                    tint,
                    envelope,
                );
            }
        }
        true
    }

    fn end(&mut self, _ctx: &mut EventCtx<'_>) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::events::test_rig::Rig;
    use crate::traffic::glints::GLINT_CAPACITY;

    fn formations(count: u32) -> Vec<Formation> {
        (0..count)
            .map(|seed| {
                let mut rig = Rig::new(u64::from(seed));
                rig.start(DroneFormationEvent::new).formation
            })
            .collect()
    }

    #[test]
    fn chaque_forme_donne_autant_de_cases_que_de_drones_toutes_distinctes() {
        for shape in SHAPES {
            for count in [28, 32, 36] {
                let cases = slots(shape, count, 5.0);
                assert_eq!(cases.len(), count, "{shape:?}");
                assert!(
                    cases.iter().all(|c| c.is_finite() && c.length() < 8.0),
                    "{shape:?} : {cases:?}"
                );
                // Deux drones ne partagent jamais la même case, à 15 cm près.
                for (i, a) in cases.iter().enumerate() {
                    for b in &cases[i + 1..] {
                        assert!(a.distance(*b) > 0.15, "{shape:?} : deux drones au même endroit");
                    }
                }
            }
        }
    }

    #[test]
    fn la_colonne_arrive_de_derriere_double_la_voiture_et_tient_la_forme_a_cote() {
        for f in formations(12) {
            // Au départ, loin derrière.
            assert!(f.position(0, 0.0).z > 150.0);
            // La forme tenue est devant la voiture et sur le côté choisi, au-dessus de la route.
            let t_hold = APPROACH_S + LAG_S * f.count() as f32 + 2.0;
            let hold = (0..f.count()).map(|i| f.position(i, t_hold)).collect::<Vec<_>>();
            assert!(hold.iter().all(|p| p.x * f.side > 10.0), "côté {} : {hold:?}", f.side);
            assert!(hold.iter().all(|p| p.y > 2.5), "trop bas");
            let mean_z = hold.iter().map(|p| p.z).sum::<f32>() / hold.len() as f32;
            assert!(
                mean_z < -5.0,
                "la forme est au niveau de la voiture, pas derrière : {mean_z}"
            );
            // Un drone de la colonne dépasse la voiture (z passe de positif à négatif).
            let crosses = (0..400).any(|k| f.position(0, k as f32 * 0.03).z < 0.0);
            assert!(crosses, "le premier drone ne double pas la voiture");
        }
    }

    #[test]
    fn la_forme_tenue_a_bien_la_silhouette_choisie() {
        // Un anneau tenu : tous les drones à la même distance du centre.
        let mut f = formations(1).remove(0);
        f.shape = Shape::Ring;
        f.slots = slots(Shape::Ring, 32, 5.0);
        f.tilt = 0.0;
        let t = APPROACH_S + LAG_S * 32.0 + 2.0;
        let centre = Vec3::new(f.side * f.lateral, f.centre_y, -f.ahead);
        for i in 0..32 {
            let d = f.position(i, t).distance(centre);
            assert!((d - 5.0).abs() < 0.4, "drone {i} à {d} du centre");
        }
    }

    #[test]
    fn les_drones_repartent_vers_l_avant_et_finissent_loin() {
        for f in formations(8) {
            let end = f.total();
            for i in 0..f.count() {
                let p = f.position(i, end - 0.1);
                assert!(p.z < -80.0, "le drone {i} n'a pas fui : z = {}", p.z);
            }
        }
    }

    #[test]
    fn le_tirage_varie_et_le_mouvement_est_continu() {
        let all = formations(24);
        let mut shapes: Vec<_> = all.iter().map(|f| f.shape as u8).collect();
        shapes.sort_unstable();
        shapes.dedup();
        assert!(shapes.len() >= 5, "formes tirées : {shapes:?}");
        assert!(all.iter().any(|f| f.side > 0.0) && all.iter().any(|f| f.side < 0.0));
        for f in &all {
            let dt = 1.0 / 60.0;
            for i in [0, f.count() / 2, f.count() - 1] {
                let mut t = 0.0;
                let mut before = f.position(i, 0.0);
                while t < f.total() {
                    t += dt;
                    let now = f.position(i, t);
                    // Le plus rapide est l'arrivée de la colonne (≈ 60 m/s) : un mètre par image au plus.
                    assert!(
                        now.distance(before) < 1.3,
                        "saut de {} m à t = {t}",
                        now.distance(before)
                    );
                    before = now;
                }
            }
        }
    }

    #[test]
    fn l_evenement_pose_un_point_par_drone_et_reserve_sa_tranche_quand_il_est_proche() {
        let mut rig = Rig::new(5);
        let mut event = rig.start(DroneFormationEvent::new);
        let n = event.formation.count();
        let mut peak = 0;
        let mut claimed = false;
        loop {
            if !rig.step(&mut event, 1.0 / 60.0, |_| {}) {
                break;
            }
            peak = peak.max(rig.glints.used());
            assert!(rig.glints.used() <= GLINT_CAPACITY);
            if event.near() {
                claimed = true;
            }
        }
        assert!(peak >= n, "{peak} points pour {n} drones");
        assert!(peak <= 2 * n, "{peak} points : trop pour {n} drones");
        assert!(claimed);
    }
}
