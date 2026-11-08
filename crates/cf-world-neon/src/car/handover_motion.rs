//! Le mouvement des deux voitures pendant un changement de DJ : pur, sans Bevy.
//!
//! L'ancienne voiture est un double visuel qui prend sa place au moment de l'échange ; la voiture réelle (la nouvelle,
//! pilotée) est posée derrière et à gauche, puis **menée par la frise** jusqu'au dépassement, parce que le pilote ne
//! sait pas rattraper une voiture à un instant donné. Au dépassement la frise la rend au pilote, à la vitesse qu'elle a.
//!
//! Repère : la route va vers −z. « Derrière » est donc +z, la droite est +x.

use crate::run::handover::{ArrivalStages, DoubleStages, HANDOVER_SWAP_S, OVERTAKE_S};

/// Déport latéral de l'ancienne voiture vers le côté droit de la route (m).
pub const PULL_RIGHT_M: f64 = 3.5;

/// Ce que le monde doit poser cette image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionOut {
    /// Position (x, z) du double de l'ancienne voiture, s'il existe.
    pub double: Option<(f64, f64)>,
    /// Position (x, z) et vitesse (m/s) à imposer à la voiture réelle, tant que la frise la mène.
    pub real: Option<(f64, f64, f32)>,
    /// La frise vient de rendre la voiture réelle au pilote (une seule image).
    pub release: bool,
}

impl MotionOut {
    /// Rien à poser.
    pub const IDLE: Self = Self {
        double: None,
        real: None,
        release: false,
    };
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Anchor {
    x: f64,
    z_double: f64,
    speed_mps: f32,
}

/// Le mouvement de passation : un point d'ancrage pris à l'échange, puis le calcul des deux positions.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HandoverMotion {
    anchor: Option<Anchor>,
    previous_real_z: Option<f64>,
    released: bool,
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
        double: &DoubleStages,
        arrival: &ArrivalStages,
        real: (f64, f64, f32),
        dt_s: f32,
    ) -> MotionOut {
        if t_s < HANDOVER_SWAP_S {
            self.reset();
            return MotionOut::IDLE;
        }
        let anchor = *self.anchor.get_or_insert(Anchor {
            x: real.0,
            z_double: real.1,
            speed_mps: real.2.max(0.0),
        });
        // L'ancienne roule à sa vitesse d'avant l'échange, puis freine jusqu'à l'arrêt complet.
        let dz = f64::from(anchor.speed_mps * (1.0 - double.brake) * dt_s.max(0.0));
        let z_double = self.anchor.map_or(anchor.z_double, |a| a.z_double) - dz;
        if let Some(a) = &mut self.anchor {
            a.z_double = z_double;
        }
        let double_xz = (anchor.x + PULL_RIGHT_M * f64::from(double.pull_right), z_double);

        let scripted = t_s < OVERTAKE_S;
        if !scripted {
            let release = !self.released;
            self.released = true;
            self.previous_real_z = None;
            return MotionOut {
                double: Some(double_xz),
                real: None,
                release,
            };
        }
        let real_z = z_double + f64::from(arrival.gap_behind_m);
        let real_x = anchor.x - f64::from(arrival.lateral_m);
        let speed = match self.previous_real_z {
            Some(previous) if dt_s > 0.0 => ((previous - real_z) / f64::from(dt_s)) as f32,
            _ => anchor.speed_mps,
        };
        self.previous_real_z = Some(real_z);
        MotionOut {
            double: Some(double_xz),
            real: Some((real_x, real_z, speed.max(0.0))),
            release: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::handover::{HANDOVER_S, handover_stages};

    const DT: f32 = 1.0 / 60.0;

    /// Rejoue une passation entière : la voiture réelle roule à 16 m/s en x = 1, z = −1000 jusqu'à l'échange.
    fn replay() -> Vec<(f32, MotionOut)> {
        let mut motion = HandoverMotion::default();
        let mut real = (1.0_f64, -1000.0_f64, 16.0_f32);
        let mut frames = Vec::new();
        for k in 0..=(HANDOVER_S / DT) as u32 {
            let time = k as f32 * DT;
            let (_, double, arrival) = handover_stages(time, false);
            let out = motion.update(time, &double, &arrival, real, DT);
            match out.real {
                Some((x, z, v)) => real = (x, z, v),
                None => real.1 -= f64::from(real.2 * DT),
            }
            frames.push((time, out));
        }
        frames
    }

    #[test]
    fn nothing_moves_before_the_swap() {
        let mut motion = HandoverMotion::default();
        let (_, d, a) = handover_stages(0.2, false);
        assert_eq!(motion.update(0.2, &d, &a, (0.0, 0.0, 16.0), DT), MotionOut::IDLE);
    }

    #[test]
    fn at_the_swap_the_double_sits_where_the_old_car_was_and_the_new_one_is_60_m_behind() {
        let frames = replay();
        let (t, first) = frames
            .iter()
            .find(|(t, _)| *t >= HANDOVER_SWAP_S)
            .map(|(t, o)| (*t, *o))
            .expect("une image à l'échange");
        let (dx, dz) = first.double.expect("le double existe");
        // La voiture roule à 16 m/s depuis z = −1000 jusqu'à l'échange (au plus une image de plus).
        let expected = -1000.0 - 16.0 * f64::from(t);
        assert!(
            (dx - 1.0).abs() < 1e-6 && (dz - expected).abs() < 0.5,
            "{dx} {dz} / {expected}"
        );
        let (real_x, real_z, _) = first.real.expect("la réelle est menée");
        assert!(
            (real_z - dz - 60.0).abs() < 1e-3 && (real_x - (1.0 - 3.5)).abs() < 1e-6,
            "{real_x} {real_z}"
        );
    }

    #[test]
    fn the_new_car_overtakes_the_old_one_before_the_release() {
        let frames = replay();
        let last_scripted = frames.iter().rev().find(|(_, o)| o.real.is_some()).expect("menée");
        let (_, rz, _) = last_scripted.1.real.expect("menée");
        let (_, dz) = last_scripted.1.double.expect("double");
        assert!(
            rz < dz - 10.0,
            "la réelle est devant d'au moins 10 m : {rz} contre {dz}"
        );
    }

    #[test]
    fn the_release_happens_exactly_once_and_the_speed_stays_plausible() {
        let frames = replay();
        assert_eq!(frames.iter().filter(|(_, o)| o.release).count(), 1);
        for (t, o) in &frames {
            if let Some((_, _, v)) = o.real {
                assert!(v.is_finite() && (0.0..60.0).contains(&v), "{t}: {v}");
            }
        }
    }

    #[test]
    fn the_double_never_goes_backwards_and_ends_stopped_on_the_right() {
        let frames = replay();
        let mut previous = f64::MAX;
        for (_, o) in &frames {
            if let Some((_, z)) = o.double {
                // La route va vers −z : l'ancienne ne recule jamais.
                assert!(z <= previous + 1e-9, "{z} après {previous}");
                previous = z;
            }
        }
        let (_, end) = frames.last().expect("fin");
        let (x, _) = end.double.expect("double");
        assert!((x - (1.0 + PULL_RIGHT_M)).abs() < 1e-6, "{x}");
        let before_end = frames[frames.len() - 2].1.double.expect("double").1;
        let end_z = end.double.expect("double").1;
        assert!((before_end - end_z).abs() < 1e-6, "arrêtée à la fin");
    }

    #[test]
    fn a_zero_time_step_changes_nothing_and_never_divides_by_zero() {
        let mut motion = HandoverMotion::default();
        let (_, d, a) = handover_stages(3.0, false);
        let first = motion.update(3.0, &d, &a, (0.0, 0.0, 16.0), 0.0);
        let second = motion.update(3.0, &d, &a, (0.0, 0.0, 16.0), 0.0);
        let (_, _, v) = second.real.expect("menée");
        assert!(v.is_finite());
        assert_eq!(first.double, second.double);
    }
}
