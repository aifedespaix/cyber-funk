//! Le mouvement des deux voitures pendant un changement de DJ : pur, sans Bevy.
//!
//! L'ancienne voiture est un double visuel qui prend sa place dès le début : elle freine jusqu'à sa vitesse de croisière
//! seule, puis la nouvelle (la voiture réelle, pilotée) arrive de loin derrière, se range au centre de la route pendant
//! que l'ancienne se déporte sur le côté, et elles roulent côte à côte avec de petites variations de vitesse. La voiture
//! réelle est **menée par la frise** jusqu'à ce que l'ancienne ait disparu (le pilote ne sait pas rejoindre une voiture à
//! un instant donné), puis rendue au pilote à la vitesse qu'elle a.
//!
//! Repère : la route va vers −z, son axe est `x = 0`. « Derrière » est +z. Les variations de vitesse sont des sinusoïdes
//! dont on connaît le déplacement exact : aucune dérive numérique, quel que soit le pas de temps.

use crate::run::handover::{ArrivalStages, DoubleStages};
use crate::run::variant::{HandoverVariant, Wobble};

/// Écart latéral supplémentaire de l'ancienne pendant sa désynchronisation (m).
const FADE_DRIFT_M: f64 = 0.9;
/// Part de sa vitesse que l'ancienne perd en se désynchronisant.
const FADE_BRAKE: f32 = 0.3;
/// Balancement latéral de la nouvelle voiture (m).
const SWAY_M: f64 = 0.12;

/// Ce que le monde doit poser cette image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionOut {
    /// Position (x, z) du double de l'ancienne voiture, s'il existe.
    pub double: Option<(f64, f64)>,
    /// Vitesse du double le long de la route (m/s).
    pub double_speed_mps: f32,
    /// Position (x, z) et vitesse (m/s) à imposer à la voiture réelle, tant que la frise la mène.
    pub real: Option<(f64, f64, f32)>,
    /// La frise vient de rendre la voiture réelle au pilote (une seule image).
    pub release: bool,
}

impl MotionOut {
    /// Rien à poser.
    pub const IDLE: Self = Self {
        double: None,
        double_speed_mps: 0.0,
        real: None,
        release: false,
    };
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Anchor {
    x: f64,
    speed_mps: f32,
}

/// Le mouvement de passation : un point d'ancrage pris à la première image, puis le calcul des deux positions.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HandoverMotion {
    anchor: Option<Anchor>,
    /// Position le long de la route de l'ancienne, hors variation (m, intégrée).
    z_base: f64,
    previous_real_z: Option<f64>,
    released: bool,
}

/// Déplacement (m, vers l'avant) dû à une variation de vitesse sinusoïdale de `speed_mps` : l'intégrale exacte, nulle à
/// `t = 0`.
fn wobble_offset(w: &Wobble, speed_mps: f32, t_s: f32) -> f64 {
    let tau = std::f64::consts::TAU;
    let (period, phase) = (f64::from(w.period_s), f64::from(w.phase_rad));
    -f64::from(speed_mps) * f64::from(w.amp) * period / tau
        * ((tau * f64::from(t_s) / period + phase).cos() - phase.cos())
}

fn smooth(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl HandoverMotion {
    /// Repart de zéro (hors passation).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Avance d'une image. `t_s` : secondes depuis le début de la passation ; `real` : position (x, z) de la voiture
    /// réelle et sa vitesse (m/s) *avant* toute intervention.
    pub fn update(
        &mut self,
        t_s: f32,
        variant: &HandoverVariant,
        double: &DoubleStages,
        arrival: &ArrivalStages,
        real: (f64, f64, f32),
        dt_s: f32,
    ) -> MotionOut {
        let dt = dt_s.max(0.0);
        if self.anchor.is_none() {
            self.anchor = Some(Anchor {
                x: real.0,
                speed_mps: real.2.max(0.0),
            });
            self.z_base = real.1;
        }
        let Some(anchor) = self.anchor else {
            return MotionOut::IDLE;
        };
        // Vitesse de base de l'ancienne : elle freine jusqu'à sa vitesse seule, puis perd un peu en disparaissant.
        let base = anchor.speed_mps + (variant.lone_speed_mps - anchor.speed_mps) * double.slow;
        let speed = base * (1.0 - FADE_BRAKE * double.fade);
        self.z_base -= f64::from(speed * dt);
        let z_old = self.z_base - wobble_offset(&variant.old_wobble, variant.lone_speed_mps, t_s);
        // Latéral : de sa voie d'origine vers l'axe en freinant, puis sur le côté quand la nouvelle arrive.
        let to_axis = anchor.x * f64::from(1.0 - double.slow);
        let aside = f64::from(variant.side) * (f64::from(variant.lane_offset_m) * f64::from(double.lateral));
        let x_old = to_axis + aside + f64::from(variant.side) * FADE_DRIFT_M * f64::from(double.fade);
        let double_xz = (x_old, z_old);

        if !arrival.scripted {
            let release = !self.released;
            self.released = true;
            self.previous_real_z = None;
            return MotionOut {
                double: Some(double_xz),
                double_speed_mps: speed,
                real: None,
                release,
            };
        }
        let z_new = self.z_base + f64::from(arrival.gap_behind_m)
            - wobble_offset(&variant.new_wobble, variant.lone_speed_mps, t_s);
        // Elle se met au centre de la route au fur et à mesure qu'elle rejoint l'ancienne ; un léger balancement.
        let settle = smooth(1.0 - f64::from(arrival.gap_behind_m) / f64::from(variant.gap_m.max(1.0)));
        let x_new = SWAY_M * (f64::from(t_s) * 0.7).sin() * settle;
        let speed_new = match self.previous_real_z {
            Some(previous) if dt > 0.0 => ((previous - z_new) / f64::from(dt)) as f32,
            _ => anchor.speed_mps,
        };
        self.previous_real_z = Some(z_new);
        MotionOut {
            double: Some(double_xz),
            double_speed_mps: speed,
            real: Some((x_new, z_new, speed_new.max(0.0))),
            release: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::handover::handover_stages;

    const DT: f32 = 1.0 / 60.0;

    fn variant(number: u32) -> HandoverVariant {
        HandoverVariant::new(number, 1, None)
    }

    /// Rejoue une passation entière : la voiture réelle roule à 16 m/s en x = 1, z = −1000 jusqu'au début.
    fn replay(v: &HandoverVariant) -> Vec<(f32, MotionOut)> {
        let mut motion = HandoverMotion::default();
        let mut real = (1.0_f64, -1000.0_f64, 16.0_f32);
        let mut frames = Vec::new();
        for k in 0..=(v.duration_s() / DT) as u32 {
            let time = k as f32 * DT;
            let (_, double, arrival) = handover_stages(time, v, false);
            let out = motion.update(time, v, &double, &arrival, real, DT);
            match out.real {
                Some((x, z, s)) => real = (x, z, s),
                None => real.1 -= f64::from(real.2 * DT),
            }
            frames.push((time, out));
        }
        frames
    }

    #[test]
    fn the_double_starts_where_the_old_car_was() {
        let v = variant(0);
        let frames = replay(&v);
        let (_, first) = frames[0];
        let (x, z) = first.double.expect("le double existe dès le début");
        assert!(
            (x - 1.0).abs() < 1e-6 && (z + 1000.0 + 16.0 * f64::from(DT)).abs() < 0.5,
            "{x} {z}"
        );
    }

    #[test]
    fn the_new_car_starts_far_behind_and_ends_abreast() {
        for number in 0..30 {
            let v = variant(number);
            let m = v.marks();
            let frames = replay(&v);
            let at = |t: f32| frames.iter().find(|(f, _)| *f >= t).map(|(_, o)| *o).expect("image");
            let first = at(0.0);
            let (_, rz, _) = first.real.expect("menée");
            let (_, dz) = first.double.expect("double");
            assert!((rz - dz - f64::from(v.gap_m)).abs() < 1.0, "{rz} {dz} {}", v.gap_m);
            let abreast = at(m.fading);
            let (rx, rz, _) = abreast.real.expect("menée");
            let (dx, dz) = abreast.double.expect("double");
            assert!(
                (rz - dz).abs() < 6.0,
                "côte à côte à quelques mètres près : {}",
                rz - dz
            );
            assert!(
                rx.abs() < 0.2 && (dx - f64::from(v.side * v.lane_offset_m)).abs() < 0.2,
                "{rx} {dx}"
            );
        }
    }

    #[test]
    fn the_old_car_moves_aside_toward_its_own_side_and_never_leaves_the_road() {
        for number in 0..30 {
            let v = variant(number);
            for (_, o) in replay(&v) {
                let (x, _) = o.double.expect("double");
                assert!(x.abs() < 6.0, "reste sur la chaussée (demi-largeur 9 m) : {x}");
                if v.side > 0.0 {
                    assert!(x > -1.5);
                } else {
                    assert!(x < 1.5);
                }
            }
        }
    }

    #[test]
    fn the_release_happens_exactly_once_and_speeds_stay_plausible() {
        for number in 0..30 {
            let v = variant(number);
            let frames = replay(&v);
            assert_eq!(
                frames.iter().filter(|(_, o)| o.release).count(),
                1,
                "passation {number}"
            );
            for (t, o) in &frames {
                assert!(
                    o.double_speed_mps.is_finite() && (0.0..30.0).contains(&o.double_speed_mps),
                    "{t}"
                );
                if let Some((x, z, s)) = o.real {
                    assert!(x.is_finite() && z.is_finite());
                    // Pas de bond : la nouvelle voiture rattrape vite mais pas plus de ~150 km/h.
                    assert!(s.is_finite() && (0.0..45.0).contains(&s), "{t}: {s}");
                }
            }
        }
    }

    #[test]
    fn neither_car_ever_goes_backwards() {
        let v = variant(4);
        let mut prev_double = f64::MAX;
        let mut prev_real = f64::MAX;
        for (t, o) in replay(&v) {
            let (_, z) = o.double.expect("double");
            assert!(z <= prev_double + 1e-6, "{t}: le double recule");
            prev_double = z;
            if let Some((_, z, _)) = o.real {
                assert!(z <= prev_real + 1e-6, "{t}: la réelle recule");
                prev_real = z;
            }
        }
    }

    #[test]
    fn the_cars_do_not_ride_in_lockstep_side_by_side() {
        let v = variant(2);
        let m = v.marks();
        let gaps: Vec<f64> = replay(&v)
            .into_iter()
            .filter(|(t, o)| *t >= m.abreast && *t <= m.fading && o.real.is_some())
            .map(|(_, o)| o.real.map_or(0.0, |r| r.1) - o.double.map_or(0.0, |d| d.1))
            .collect();
        let (lo, hi) = gaps
            .iter()
            .fold((f64::MAX, f64::MIN), |(lo, hi), g| (lo.min(*g), hi.max(*g)));
        assert!(hi - lo > 0.3, "l'écart varie : {lo}..{hi}");
        assert!(hi - lo < 8.0, "mais elles restent ensemble : {lo}..{hi}");
    }

    #[test]
    fn a_zero_time_step_changes_nothing_and_never_divides_by_zero() {
        let v = variant(1);
        let mut motion = HandoverMotion::default();
        let (_, d, a) = handover_stages(3.0, &v, false);
        let first = motion.update(3.0, &v, &d, &a, (0.0, 0.0, 16.0), 0.0);
        let second = motion.update(3.0, &v, &d, &a, (0.0, 0.0, 16.0), 0.0);
        let (_, _, speed) = second.real.expect("menée");
        assert!(speed.is_finite());
        assert_eq!(first.double, second.double);
    }

    #[test]
    fn wobble_offset_is_zero_at_the_start_and_bounded() {
        let w = Wobble {
            amp: 0.05,
            period_s: 6.0,
            phase_rad: 1.0,
        };
        assert!(wobble_offset(&w, 10.0, 0.0).abs() < 1e-12);
        for k in 0..400 {
            assert!(wobble_offset(&w, 10.0, k as f32 * 0.1).abs() < 2.0);
        }
    }
}
