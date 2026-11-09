//! Orage électrique (`events/storm.ts`) : des éclairs frappent le sommet des tours de chaque côté de la route, au rythme
//! des kicks (plus nombreux et plus violents quand le morceau monte). Les coups restent à 18 m au moins de l'axe : la
//! voiture, même volante, ne risque rien et peut les traverser des yeux sans que rien ne réagisse.
//!
//! # Un éclair qui ressemble à un éclair
//!
//! Sillage posait une ligne brisée de points ronds, tirée au hasard par à-coups de 9 m : un collier de perles. Un vrai
//! coup de foudre a trois traits que l'on retrouve ici :
//!
//! - **un canal tortueux à toutes les échelles** : le tracé est fractal (déplacement du point milieu, cinq niveaux), donc
//!   des grands coudes et, dedans, des plus petits ([`fractal`]) ;
//! - **des ramifications** : des branches plus fines partent du canal vers le bas et vers le côté, brisées de la même
//!   façon ([`Branch`]) ;
//! - **deux temps** : un précurseur faible et fin qui descend du nuage en 0,16 s (`LEADER_S`), puis l'arc en retour, un
//!   canal blanc très lumineux qui monte vite et retombe en s'attardant, avec une seconde décharge atténuée sur le même
//!   canal ([`strength`]).
//!
//! Chaque tronçon est un ruban face à la caméra (`PanelKind::Bolt`) : noyau étroit et blanc, halo large teinté, de
//! largeur constante à l'écran quelle que soit la distance ([`HALO_PER_M`]). Il reste de la place dans les points
//! lumineux pour les impacts et l'éclairage des nuages.
//!
//! # Règle 7 (WCAG 2.3.1)
//!
//! Le blanchissement de la ville n'est pas calculé ici : l'événement **demande** un éclair au moment de l'arc en retour
//! (`SceneFx::lightning_request`) et le limiteur dit ce qui est accordé. L'éclat du tracé suit ce niveau accordé
//! (`granted`) : si le limiteur refuse (déjà trois éclairs dans la seconde, flashs coupés, respiration), le canal reste un
//! trait pâle au lieu d'un flash. La seconde décharge ne descend jamais sous 55 % du premier pic : une bosse, pas un
//! stroboscope.

use bevy::color::LinearRgba;
use bevy::math::DVec3;
use cf_render::SceneColor;

use crate::pilot::rand::Rand;
use crate::traffic::event::{EventCtx, TrafficEvent};
use crate::traffic::events::common::{index, range, smooth};
use crate::traffic::panels::PanelKind;

const LIFE_S: f32 = 22.0;
const MAX_BOLTS: usize = 2;
/// Le précurseur descend du nuage en ce temps, s.
const LEADER_S: f32 = 0.16;
/// Durée de l'arc en retour et de ses séquelles, s.
const RETURN_S: f32 = 0.8;
/// Vie d'un coup, précurseur compris, s.
const BOLT_LIFE_S: f32 = LEADER_S + RETURN_S;
/// Un coup ne frappe jamais à moins de 18 m de l'axe de la route.
const MIN_X_M: f64 = 18.0;

/// Niveaux de subdivision du canal principal : 2⁵ = 32 tronçons.
const MAIN_LEVELS: usize = 5;
const MAIN_POINTS: usize = (1 << MAIN_LEVELS) + 1;
/// Niveaux d'une ramification : 2³ = 8 tronçons.
const BRANCH_LEVELS: usize = 3;
const BRANCH_POINTS: usize = (1 << BRANCH_LEVELS) + 1;
const MAX_BRANCHES: usize = 6;

/// Demi-largeur du halo par mètre de distance à la caméra : à 70 m, 1,1 m ; à 300 m, 4,8 m. À l'écran, un halo de
/// 20 px de large environ, quelle que soit la distance.
const HALO_PER_M: f64 = 0.016;
/// Éclat du noyau avant le niveau accordé (au-delà de 1, le bloom s'en empare).
const CORE_GAIN: f32 = 6.0;

/// Une ramification du canal principal.
#[derive(Debug, Clone, Copy)]
struct Branch {
    points: [DVec3; BRANCH_POINTS],
    /// Part du canal principal déjà tracée quand la branche se met à descendre, `0..1` : le précurseur ne la dessine pas
    /// avant.
    root_at: f32,
}

#[derive(Debug, Clone, Copy)]
struct Bolt {
    born: f64,
    /// Points du tracé, du ciel vers le sol.
    path: [DVec3; MAIN_POINTS],
    branches: [Branch; MAX_BRANCHES],
    branch_count: usize,
    tint: LinearRgba,
    /// L'arc en retour a été demandé au limiteur.
    requested: bool,
}

/// Un tracé fractal de `from` à `to`, de `N = 2ᵏ + 1` points : à chaque niveau, le milieu de chaque tronçon est écarté
/// perpendiculairement à lui, d'au plus `rough × longueur`, `rough` diminuant de `decay` d'un niveau au suivant.
fn fractal<const N: usize>(from: DVec3, to: DVec3, rough: f64, decay: f64, rng: &mut Rand) -> [DVec3; N] {
    const { assert!(N >= 3 && (N - 1).is_power_of_two()) };
    let mut points = [from; N];
    points[N - 1] = to;
    let mut step = N - 1;
    let mut amp = rough;
    while step > 1 {
        let half = step / 2;
        let mut cursor = 0;
        while cursor + step < N {
            let (upper, lower) = (points[cursor], points[cursor + step]);
            let axis = lower - upper;
            let len = axis.length();
            let dir = if len > 1e-9 { axis / len } else { DVec3::NEG_Y };
            // Écart tiré dans le plan perpendiculaire au tronçon, plus fort à l'horizontale : la foudre ondule plus qu'elle
            // ne monte et ne descend.
            let mut jitter = DVec3::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0) * 0.45, rng.range(-1.0, 1.0));
            jitter -= dir * jitter.dot(dir);
            points[cursor + half] =
                (upper + lower) * 0.5 + jitter.normalize_or_zero() * (len * amp * rng.range(0.4, 1.0));
            cursor += step;
        }
        step = half;
        amp *= decay;
    }
    points
}

/// Repousse un point au-delà de `MIN_X_M` de l'axe : jamais sur la chaussée.
fn off_road(mut p: DVec3, side: f64) -> DVec3 {
    if p.x.abs() < MIN_X_M {
        let sign = if p.x.abs() > 0.0 { p.x.signum() } else { side };
        p.x = sign * MIN_X_M;
    }
    p
}

/// L'éclat du canal à l'âge `age` du coup, 0 → 1 environ, avant le niveau accordé par le limiteur. Précurseur : un filet
/// pâle qui s'allume en `LEADER_S`. Arc en retour : montée en 0,06 s sur ce filet (le canal ne s'éteint jamais entre les
/// deux), retombée de constante 0,17 s, et une seconde décharge à 0,2 s, de la moitié du pic.
fn strength(age: f32) -> f32 {
    if age < LEADER_S {
        return 0.22 * smooth(0.0, 0.04, age);
    }
    let a = age - LEADER_S;
    // Le filet du précurseur s'éteint doucement sous l'arc en retour.
    let base = 0.22 * (1.0 - smooth(0.0, 0.45, a));
    let rise = smooth(0.0, 0.06, a);
    let main = (-a / 0.17).exp();
    let second = 0.5 * (-((a - 0.2) / 0.06).powi(2)).exp();
    (base + rise * (main + second)) * (1.0 - smooth(0.5, RETURN_S, a))
}

#[derive(Debug)]
pub(crate) struct StormEvent {
    age: f32,
    bolts: Vec<Bolt>,
    wait: f32,
    white: LinearRgba,
    cold: LinearRgba,
}

impl StormEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        Self {
            age: 0.0,
            bolts: Vec::with_capacity(MAX_BOLTS),
            wait: range(ctx.rng, 0.6, 1.4),
            white: ctx.frame.named(SceneColor::Ice),
            cold: ctx.frame.named(SceneColor::Azure),
        }
    }

    fn strike(&mut self, ctx: &mut EventCtx<'_>) {
        let side = if ctx.rng.coin() { -1.0 } else { 1.0 };
        let x = f64::from(side * range(ctx.rng, 18.0, 75.0));
        let z = ctx.frame.car.z - f64::from(range(ctx.rng, 70.0, 320.0));
        let top = DVec3::new(
            x + f64::from(range(ctx.rng, -25.0, 25.0)),
            f64::from(range(ctx.rng, 250.0, 285.0)),
            z,
        );
        let ground_y = f64::from(range(ctx.rng, 25.0, 95.0));
        let foot = DVec3::new(
            x + f64::from(range(ctx.rng, -14.0, 14.0)),
            ground_y,
            z + f64::from(range(ctx.rng, -14.0, 14.0)),
        );
        let mut path: [DVec3; MAIN_POINTS] = fractal(top, foot, 0.2, 0.55, ctx.rng);
        for p in &mut path {
            *p = off_road(*p, f64::from(side));
        }

        let branch_count = 3 + index(ctx.rng, MAX_BRANCHES - 2);
        let empty = Branch {
            points: [DVec3::ZERO; BRANCH_POINTS],
            root_at: 0.0,
        };
        let mut branches = [empty; MAX_BRANCHES];
        for branch in branches.iter_mut().take(branch_count) {
            // Une branche naît au milieu haut du canal et file vers le bas et le côté.
            let root = 5 + index(ctx.rng, MAIN_POINTS - 12);
            let start = path[root];
            let down = (path[root + 1] - path[root]).normalize_or_zero();
            let angle = f64::from(range(ctx.rng, 0.0, std::f32::consts::TAU));
            let lateral = DVec3::new(angle.cos(), 0.0, angle.sin());
            let dir =
                (down * 0.6 + lateral * f64::from(range(ctx.rng, 0.7, 1.2)) + DVec3::NEG_Y * 0.25).normalize_or_zero();
            let length = f64::from(range(ctx.rng, 28.0, 75.0));
            let mut end = start + dir * length;
            // Une branche ne passe pas sous la pointe du canal.
            end.y = end.y.max(ground_y + 6.0);
            let mut points: [DVec3; BRANCH_POINTS] = fractal(start, end, 0.28, 0.55, ctx.rng);
            for p in &mut points {
                *p = off_road(*p, f64::from(side));
            }
            *branch = Branch {
                points,
                root_at: root as f32 / (MAIN_POINTS - 1) as f32,
            };
        }

        let tint = if ctx.rng.coin() { self.white } else { self.cold };
        self.bolts.push(Bolt {
            born: ctx.frame.time,
            path,
            branches,
            branch_count,
            tint,
            requested: false,
        });
    }
}

/// Pose les rubans d'une ligne brisée. `reveal` (`0..1`) : part tracée, depuis le premier point ; `width` : échelle de
/// la largeur ; `param` : 0 canal principal, 0,5 ramification.
#[allow(clippy::too_many_arguments)] // Ligne, caméra, avancée, largeur, motif, teinte, éclat : un seul appel, pas de structure utile.
fn ribbons(
    ctx: &mut EventCtx<'_>,
    camera: DVec3,
    points: &[DVec3],
    reveal: f32,
    width: f32,
    param: f32,
    tint: LinearRgba,
    gain: f32,
) {
    let segments = points.len() - 1;
    let shown = reveal * segments as f32;
    for (i, pair) in points.windows(2).enumerate() {
        // Le dernier tronçon visible est coupé net à l'avancée du précurseur : on ne le raccourcit pas, il apparaît d'un coup.
        if i as f32 >= shown {
            break;
        }
        let (a, b) = (pair[0], pair[1]);
        let mid = (a + b) * 0.5;
        let axis = (b - a).as_vec3();
        let len = axis.length();
        if len < 1e-3 {
            continue;
        }
        let dir = axis / len;
        let to_camera = (camera - mid).as_vec3();
        let dist = to_camera.length().max(1.0);
        // Le ruban est perpendiculaire à la fois au tronçon et à la vue : il fait toujours face à la caméra.
        let mut side = dir.cross(to_camera / dist);
        side = if side.length() > 0.05 {
            side.normalize()
        } else {
            dir.any_orthonormal_vector()
        };
        let half_w = ((f64::from(dist) * HALO_PER_M) as f32 * width).clamp(0.35, 9.0);
        ctx.scene.panels.quad(
            PanelKind::Bolt,
            mid,
            dir * (len * 0.5 + half_w * 0.35),
            side * half_w,
            tint,
            gain,
            param,
        );
    }
}

impl TrafficEvent for StormEvent {
    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.age += dt;
        let envelope = (self.age / 2.0).min(1.0) * ((LIFE_S - self.age) / 2.5).min(1.0);
        if self.age > LIFE_S {
            return false;
        }

        self.wait -= dt * (0.7 + ctx.frame.drive.intensity * 1.1);
        let drop = ctx.frame.drive.drop;
        let kick = ctx.frame.beat.kick && ctx.frame.beat.bass > 0.55 && ctx.rng.unit() < 0.55;
        if (self.wait <= 0.0 || kick || drop) && self.bolts.len() < MAX_BOLTS && envelope > 0.4 {
            self.strike(ctx);
            self.wait = range(ctx.rng, 1.1, 3.2);
            // Un second coup juste derrière le premier : le grondement d'un orage vrai.
            if ctx.rng.unit() < 0.3 {
                self.wait = 0.12;
            }
        }

        // Ce que le limiteur a accordé à l'image précédente : il règle l'éclat du tracé comme celui de la ville.
        let granted = ctx.scene.fx.lightning.clamp(0.0, 1.0);
        let time = ctx.frame.time;
        self.bolts.retain(|bolt| ((time - bolt.born) as f32) < BOLT_LIFE_S);
        let camera = ctx.frame.car + DVec3::new(0.0, 2.3, 6.2);
        for bolt in &mut self.bolts {
            let age = (time - bolt.born) as f32;
            // L'arc en retour réclame son éclair au limiteur, une seule fois, quand le précurseur touche le sol.
            if age >= LEADER_S && !bolt.requested {
                bolt.requested = true;
                ctx.scene.fx.lightning_request = true;
            }
            let returning = age >= LEADER_S;
            // Refusé, l'éclair reste un filet pâle : 35 % au plus tant que le limiteur n'a rien accordé.
            let permitted = if returning {
                0.35 + 0.65 * (granted * 2.5).min(1.0)
            } else {
                1.0
            };
            let level = strength(age) * permitted * envelope;
            if level < 0.004 {
                continue;
            }
            let reveal = if returning { 1.0 } else { age / LEADER_S };
            // Le précurseur est mince et bleuté ; l'arc en retour, épais et blanc.
            let thick = if returning { 1.0 } else { 0.55 };
            let tint = if returning { bolt.tint } else { self.cold };
            let gain = CORE_GAIN * level;
            let path = bolt.path;
            ribbons(ctx, camera, &path, reveal, thick, 0.0, tint, gain);
            for branch in &bolt.branches[..bolt.branch_count] {
                // La branche ne s'allume qu'une fois le précurseur arrivé à son point d'attache.
                let local = ((reveal - branch.root_at) / (1.0 - branch.root_at).max(0.05)).clamp(0.0, 1.0);
                ribbons(ctx, camera, &branch.points, local, thick * 0.6, 0.5, tint, gain * 0.7);
            }

            let foot = path[MAIN_POINTS - 1];
            if returning {
                // Gerbe d'impact au pied du canal, et lueur du nuage où il prend naissance : le limiteur règle les deux.
                ctx.glints.add(foot, 38.0, self.white, 2.4 * level * permitted);
                ctx.glints.add(foot, 9.0, self.white, 4.0 * level);
                ctx.glints.add(path[0], 70.0, tint, 1.1 * level * permitted);
            }
        }
        true
    }

    fn end(&mut self, _ctx: &mut EventCtx<'_>) {
        self.bolts.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::events::test_rig::Rig;
    use crate::traffic::glints::GLINT_CAPACITY;
    use crate::traffic::panels::PANEL_CAPACITY;

    #[test]
    fn le_trace_fractal_relie_les_deux_extremites_et_ondule() {
        let mut rng = Rand::new(4);
        let from = DVec3::new(30.0, 270.0, -200.0);
        let to = DVec3::new(34.0, 50.0, -210.0);
        let path: [DVec3; MAIN_POINTS] = fractal(from, to, 0.2, 0.55, &mut rng);
        assert_eq!((path[0], path[MAIN_POINTS - 1]), (from, to));
        assert!(path.iter().all(|p| p.is_finite()));
        // Le canal s'écarte de la droite : un vrai coude, pas une ligne.
        let axis = (to - from).normalize();
        let widest = path
            .iter()
            .map(|p| {
                let rel = *p - from;
                (rel - axis * rel.dot(axis)).length()
            })
            .fold(0.0_f64, f64::max);
        assert!(widest > 5.0, "tracé trop droit : {widest} m");
    }

    #[test]
    fn l_eclat_a_deux_temps_sans_stroboscope() {
        // Précurseur pâle, puis l'arc en retour au sommet, puis retombée.
        assert!(strength(0.0) < 0.01);
        assert!(strength(LEADER_S - 0.01) < 0.3);
        let peak = (0..60)
            .map(|i| strength(LEADER_S + i as f32 * 0.005))
            .fold(0.0_f32, f32::max);
        assert!(peak > 0.85, "{peak}");
        // Entre le pic et la seconde décharge, l'éclat ne descend jamais sous 55 % du pic.
        let trough = (0..40)
            .map(|i| strength(LEADER_S + 0.06 + i as f32 * 0.004))
            .fold(f32::MAX, f32::min);
        assert!(trough > 0.55 * peak * 0.9, "creux {trough} pour un pic {peak}");
        assert!(strength(BOLT_LIFE_S) < 0.01);
        // Jamais de saut d'une image à l'autre supérieur à la moitié du pic à 60 i/s.
        let mut previous = strength(0.0);
        let mut t = 0.0;
        while t < BOLT_LIFE_S {
            t += 1.0 / 60.0;
            let s = strength(t);
            assert!(
                (s - previous).abs() < 0.5 * peak,
                "saut de {} à t = {t}",
                (s - previous).abs()
            );
            previous = s;
        }
    }

    #[test]
    fn les_coups_restent_loin_de_l_axe_et_demandent_un_eclair_au_limiteur() {
        let mut rig = Rig::new(21);
        let mut event = rig.start(StormEvent::new);
        let mut requests = 0;
        let mut strikes_seen = 0;
        let mut peak_panels = 0;
        for i in 0..(25 * 60) {
            let alive = rig.step(&mut event, 1.0 / 60.0, |frame| {
                frame.beat.kick = i % 28 == 0;
                frame.beat.bass = 0.8;
                frame.drive.intensity = 0.8;
            });
            requests += usize::from(rig.scene.fx.lightning_request);
            strikes_seen = strikes_seen.max(event.bolts.len());
            peak_panels = peak_panels.max(rig.scene.panels.light().len());
            assert!(rig.glints.used() <= GLINT_CAPACITY);
            for bolt in &event.bolts {
                assert!(
                    bolt.path.iter().all(|p| p.x.abs() >= MIN_X_M - 1e-9),
                    "un coup sur la route"
                );
                for branch in &bolt.branches[..bolt.branch_count] {
                    assert!(
                        branch.points.iter().all(|p| p.x.abs() >= MIN_X_M - 1e-9),
                        "une branche sur la route"
                    );
                }
            }
            if !alive {
                break;
            }
        }
        assert!(requests >= 4, "l'orage frappe : {requests} coups");
        assert!(strikes_seen <= MAX_BOLTS);
        // Un coup, c'est des dizaines de rubans, mais jamais plus que la couche n'en contient.
        assert!(peak_panels >= 40, "le tracé est dessiné : {peak_panels} rubans");
        assert!(peak_panels < PANEL_CAPACITY);
        // La façade n'est pas réglée par l'événement : tant qu'aucun limiteur ne répond, rien ne blanchit.
        assert_eq!(rig.scene.fx.lightning, 0.0);
    }

    #[test]
    fn la_demande_au_limiteur_attend_l_arc_en_retour() {
        let mut rig = Rig::new(9);
        let mut event = rig.start(StormEvent::new);
        let mut first_strike_frame = None;
        let mut first_request_frame = None;
        for i in 0..(10 * 60) {
            rig.step(&mut event, 1.0 / 60.0, |frame| frame.drive.drop = true);
            if first_strike_frame.is_none() && !event.bolts.is_empty() {
                first_strike_frame = Some(i);
            }
            if first_request_frame.is_none() && rig.scene.fx.lightning_request {
                first_request_frame = Some(i);
            }
        }
        let (strike, request) = (
            first_strike_frame.expect("un coup"),
            first_request_frame.expect("une demande"),
        );
        // Le précurseur dure 0,16 s, soit une dizaine d'images, avant que l'éclair soit demandé.
        assert!(
            request >= strike + 8,
            "demande trop tôt : coup {strike}, demande {request}"
        );
    }

    #[test]
    fn la_fin_vide_les_coups() {
        let mut rig = Rig::new(2);
        let mut event = rig.start(StormEvent::new);
        for _ in 0..600 {
            rig.step(&mut event, 1.0 / 60.0, |frame| frame.drive.drop = true);
        }
        assert!(!event.bolts.is_empty());
        rig.end(&mut event);
        assert!(event.bolts.is_empty());
    }
}
