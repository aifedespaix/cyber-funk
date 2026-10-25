//! La chorégraphie d'un robot (`mecha/dance.ts`) : à chaque kick, la pose cible est la suivante de la séquence (selon
//! l'intensité), et le robot l'atteint en un mouvement saccadé, calé sur le tempo détecté. Le mouvement se pose avant la fin
//! du temps, ce qui donne l'impression d'une machine qui frappe chaque temps.

use crate::landmarks::math::clamp01;

/// Une pose : angles en radians. Épaules et coudes sont « vers l'extérieur » : 0 pend le long du corps, π/2 est à
/// l'horizontale, π pointe vers le ciel ; un coude positif relève l'avant-bras. Les bras gauche et droit se symétrisent à
/// l'application.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pose {
    /// Épaule gauche.
    pub left_shoulder: f32,
    /// Coude gauche.
    pub left_elbow: f32,
    /// Épaule droite.
    pub right_shoulder: f32,
    /// Coude droit.
    pub right_elbow: f32,
    /// Torsion du torse.
    pub twist: f32,
    /// Flexion des jambes, 0 ou 1.
    pub squat: f32,
}

const D: f32 = std::f32::consts::PI / 180.0;

const fn pose(ls: f32, le: f32, rs: f32, re: f32, twist: f32, squat: f32) -> Pose {
    Pose {
        left_shoulder: ls * D,
        left_elbow: le * D,
        right_shoulder: rs * D,
        right_elbow: re * D,
        twist: twist * D,
        squat,
    }
}

/// Séquences de poses géométriques : angles droits, en T, en V, en cadre. Une pose par temps.
const CALM: [Pose; 4] = [
    pose(20.0, 10.0, 20.0, 10.0, 0.0, 0.0),
    pose(30.0, 20.0, 30.0, 20.0, 0.0, 0.0),
    pose(25.0, 10.0, 25.0, 10.0, 0.0, 0.0),
    pose(35.0, 25.0, 35.0, 25.0, 0.0, 0.0),
];
const GROOVE: [Pose; 8] = [
    pose(90.0, 0.0, 90.0, 0.0, 0.0, 0.0),
    pose(90.0, 90.0, 90.0, 90.0, 0.0, 0.0),
    pose(150.0, 0.0, 30.0, 0.0, 12.0, 0.0),
    pose(30.0, 0.0, 150.0, 0.0, -12.0, 0.0),
    pose(90.0, -90.0, 90.0, -90.0, 0.0, 0.0),
    pose(135.0, 45.0, 135.0, 45.0, 0.0, 1.0),
    pose(60.0, 120.0, 60.0, 120.0, 8.0, 0.0),
    pose(90.0, 0.0, 90.0, 0.0, -8.0, 1.0),
];
const PEAK: [Pose; 8] = [
    pose(170.0, 0.0, 170.0, 0.0, 0.0, 1.0),
    pose(90.0, 90.0, 90.0, 90.0, 15.0, 0.0),
    pose(170.0, 0.0, 10.0, 0.0, -15.0, 0.0),
    pose(10.0, 0.0, 170.0, 0.0, 15.0, 0.0),
    pose(120.0, -60.0, 120.0, -60.0, 0.0, 1.0),
    pose(180.0, 0.0, 180.0, 0.0, 0.0, 1.0),
    pose(90.0, 0.0, 170.0, 0.0, 0.0, 0.0),
    pose(170.0, 0.0, 90.0, 0.0, 0.0, 0.0),
];

/// Intensité d'une séquence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Calme.
    Calm,
    /// Groove.
    Groove,
    /// Sommet.
    Peak,
}

impl Level {
    /// Les trois niveaux, du plus calme au plus vif.
    pub const ALL: [Self; 3] = [Self::Calm, Self::Groove, Self::Peak];

    fn sequence(self) -> &'static [Pose] {
        match self {
            Self::Calm => &CALM,
            Self::Groove => &GROOVE,
            Self::Peak => &PEAK,
        }
    }
}

/// Nombre de crans d'un mouvement : le robot avance par à-coups d'actionneur, pas en douceur.
const STEPS: f32 = 5.0;
/// Un mouvement dure au plus cette durée, ou la moitié d'un temps si le tempo est rapide.
const MAX_MOVE: f32 = 0.26;

fn lerp_pose(a: &Pose, b: &Pose, t: f32) -> Pose {
    let l = |x: f32, y: f32| x + (y - x) * t;
    Pose {
        left_shoulder: l(a.left_shoulder, b.left_shoulder),
        left_elbow: l(a.left_elbow, b.left_elbow),
        right_shoulder: l(a.right_shoulder, b.right_shoulder),
        right_elbow: l(a.right_elbow, b.right_elbow),
        twist: l(a.twist, b.twist),
        squat: l(a.squat, b.squat),
    }
}

/// La chorégraphie d'un robot.
#[derive(Debug, Clone)]
pub struct Choreography {
    current: Pose,
    from: Pose,
    to: Pose,
    started_at: f64,
    duration: f32,
}

impl Default for Choreography {
    fn default() -> Self {
        Self {
            current: GROOVE[0],
            from: GROOVE[0],
            to: GROOVE[0],
            started_at: 0.0,
            duration: MAX_MOVE,
        }
    }
}

impl Choreography {
    /// Un kick : `index` = numéro du kick ; `period_s` = durée d'un temps.
    pub fn on_beat(&mut self, index: u32, level: Level, time: f64, period_s: f32) {
        // Au calme, on ne change de pose qu'un temps sur deux : une respiration.
        if level == Level::Calm && index % 2 == 1 {
            return;
        }
        let sequence = level.sequence();
        self.from = self.current;
        self.to = sequence[index as usize % sequence.len()];
        self.started_at = time;
        self.duration = MAX_MOVE.min(period_s * 0.5);
    }

    /// La pose à l'instant `time`.
    pub fn update(&mut self, time: f64) -> Pose {
        let t = clamp01(((time - self.started_at) as f32) / self.duration.max(1e-3));
        let eased = ((1.0 - (1.0 - t).powi(3)) * STEPS).floor() / STEPS;
        self.current = lerp_pose(&self.from, &self.to, if t >= 1.0 { 1.0 } else { eased });
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_pose_atteint_sa_cible_avant_la_fin_du_temps() {
        let mut dance = Choreography::default();
        dance.on_beat(2, Level::Groove, 10.0, 0.5);
        let target = GROOVE[2];
        assert_eq!(dance.update(10.0 + 0.26), target);
    }

    #[test]
    fn le_mouvement_avance_par_crans() {
        let mut dance = Choreography::default();
        dance.on_beat(2, Level::Groove, 0.0, 0.5);
        let mut values = Vec::new();
        for i in 0..=26 {
            values.push(dance.update(f64::from(i) * 0.01).left_shoulder);
        }
        let mut distinct = values.clone();
        distinct.dedup();
        assert!(
            distinct.len() <= 7,
            "au plus six crans plus l'arrivée : {}",
            distinct.len()
        );
    }

    #[test]
    fn au_calme_on_ne_change_de_pose_qu_un_temps_sur_deux() {
        let mut dance = Choreography::default();
        dance.on_beat(1, Level::Calm, 0.0, 0.5);
        assert_eq!(dance.update(1.0), GROOVE[0], "le temps impair est ignoré");
        dance.on_beat(2, Level::Calm, 1.0, 0.5);
        assert!((dance.update(2.0).left_shoulder - CALM[2].left_shoulder).abs() < 1e-5);
    }

    #[test]
    fn un_tempo_rapide_raccourcit_le_mouvement() {
        let mut dance = Choreography::default();
        dance.on_beat(0, Level::Peak, 0.0, 0.2);
        assert_eq!(dance.update(0.11), PEAK[0], "la moitié d'un temps de 0,2 s");
    }

    #[test]
    fn les_sequences_ont_les_longueurs_de_sillage() {
        assert_eq!((CALM.len(), GROOVE.len(), PEAK.len()), (4, 8, 8));
        assert!((GROOVE[2].left_shoulder - 150.0 * D).abs() < 1e-6);
    }
}
