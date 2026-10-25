//! Un robot danseur (`mecha/robot.ts`) : une hiérarchie de pivots emboîtés, sans squelette. Hanches → torse (torsion) → tête
//! (hochement) ; torse → épaule → coude, deux fois. Chaque os est un boîtier au matériau « panneau », donc une rotation de
//! pivot articule tout le bras. Les poses viennent d'une chorégraphie propre au robot.
//!
//! Sillage emboîtait des `Group` de three.js ; ici la cinématique directe est calculée en `Mat4` à chaque image
//! ([`Robot::paint`]) et chaque os devient une pièce à pose dans le repère de l'immeuble (aucune entité de plus que ses
//! quatorze pièces, aucune hiérarchie profonde à propager).
//!
//! Écarts avec Sillage (règle 7 de `CLAUDE.md`) : la déchirure du visage à chaque changement (`uSwap = 1`, retombée en 1/3 s)
//! passe par un limiteur de flashs **partagé par le chœur** ; quand les flashs sont coupés ou en respiration, le visage reste
//! calme ou souriant (jamais cyborg ni croix) et ne se déchire pas ; les fenêtres des os lisent des aigus et des basses lissés.

use bevy::math::Vec3;
use cf_render::SceneColor;

mod kinematics;

use super::dance::{Choreography, Level, Pose};
use crate::landmarks::frame::LandmarkFrame;
use crate::landmarks::math::damp;
use crate::landmarks::part::{Look, MeshSpec, PartSpec};

const LEG_H: f32 = 34.0;
/// Les jambes plongent bien sous la route : on ne voit jamais les pieds, le robot semble sortir de l'abîme.
const LEG_DOWN: f32 = 420.0;
const TORSO_W: f32 = 34.0;
const TORSO_H: f32 = 36.0;
const TORSO_D: f32 = 22.0;
const UPPER: f32 = 20.0;
const FORE: f32 = 18.0;
/// Portée maximale des bras tendus : l'emprise d'un robot bras écartés, avant mise à l'échelle.
pub const REACH: f32 = TORSO_W / 2.0 + 4.0 + UPPER + FORE + 6.0;
const DROP_BEATS: u32 = 16;
/// Un visage ne change pas plus d'une fois par seconde, sauf drop et fin de titre.
const FACE_HOLD_S: f64 = 1.0;
/// Retombée de la déchirure d'un changement de visage, s.
pub const TEAR_DECAY_S: f32 = 0.33;
/// Les os d'un robot : jambes (2), torse, cou, tête, bras (4 par côté).
pub const BLOCKS: usize = 13;
/// Pièces d'un robot : les os plus l'écran de la tête.
pub const PARTS: usize = BLOCKS + 1;

/// Les visages de l'écran, dans l'ordre des indices du shader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// Deux points et un trait.
    Calm = 0,
    /// Smiley jaune.
    Happy = 1,
    /// Visière cyborg.
    Cyborg = 2,
    /// Yeux en croix.
    Cross = 3,
    /// Clin d'œil (fin de titre).
    Wink = 4,
}

impl Face {
    /// Humeur du visage : la couleur des néons et de la lumière qu'il projette.
    fn mood(self) -> SceneColor {
        match self {
            Self::Calm => SceneColor::Mint,
            Self::Happy | Self::Wink => SceneColor::Sun,
            Self::Cyborg => SceneColor::Alarm,
            Self::Cross => SceneColor::Rose,
        }
    }

    /// Un visage agressif : il passe par le limiteur.
    #[must_use]
    pub fn aggressive(self) -> bool {
        matches!(self, Self::Cyborg | Self::Cross)
    }
}

/// Ce qui distingue un robot de ses voisins : place, taille, couleur et façon de danser.
#[derive(Debug, Clone, Copy)]
pub struct RobotSpec {
    /// Côté de la route : le robot fait face à l'axe.
    pub side: i8,
    /// Distance à l'axe (repère de l'immeuble), m.
    pub x: f32,
    /// Position le long de la route, m.
    pub z: f32,
    /// Échelle.
    pub scale: f32,
    /// Couleur du robot.
    pub accent: SceneColor,
    /// Décalage dans la séquence de poses : deux robots ne sont jamais sur la même figure.
    pub beat_offset: u32,
    /// Danse une intensité plus bas (−1) ou plus haut (+1) que le morceau.
    pub level_shift: i8,
    /// Échange le bras gauche et le bras droit.
    pub mirror: bool,
    /// 2 : ne change de pose qu'un temps sur deux.
    pub every: u32,
    /// Gain de la torsion.
    pub twist_gain: f32,
    /// Vitesse du regard.
    pub look_speed: f32,
    /// Indice de la lumière dynamique réservée à ce robot.
    pub slot: usize,
    /// Graine des fenêtres de ses os.
    pub seed: u32,
}

/// Les pièces d'un robot, dans l'ordre de [`Robot::paint`].
#[must_use]
pub fn parts() -> Vec<PartSpec> {
    let mut out = Vec::with_capacity(PARTS);
    for name in ["jambe gauche", "jambe droite", "torse", "cou", "tête"] {
        out.push(PartSpec::new(name, MeshSpec::Cube, Look::MechaPanel));
    }
    out.push(PartSpec::new(
        "écran",
        MeshSpec::Quad { w: 20.0, h: 16.0 },
        Look::MechaFace,
    ));
    for name in [
        "épaule gauche",
        "bras gauche",
        "avant-bras gauche",
        "main gauche",
        "épaule droite",
        "bras droit",
        "avant-bras droit",
        "main droite",
    ] {
        out.push(PartSpec::new(name, MeshSpec::Cube, Look::MechaPanel));
    }
    out
}

/// Un robot danseur.
#[derive(Debug)]
pub struct Robot {
    spec: RobotSpec,
    dance: Choreography,
    drop_beats: u32,
    aggro: Face,
    current: Face,
    last_change: f64,
    tone: Vec3,
    swap: f32,
    tear_until: f64,
    pose: Pose,
    pulse: f32,
    bar: f32,
    time: f32,
    high: f32,
    bass: f32,
    chest: Vec3,
}

impl Robot {
    /// Un robot au repos.
    #[must_use]
    pub fn new(spec: RobotSpec) -> Self {
        Self {
            spec,
            dance: Choreography::default(),
            drop_beats: 0,
            aggro: if spec.beat_offset.is_multiple_of(2) {
                Face::Cyborg
            } else {
                Face::Cross
            },
            current: Face::Happy,
            last_change: -10.0,
            tone: Vec3::ZERO,
            swap: 0.0,
            tear_until: f64::NEG_INFINITY,
            pose: Pose::default(),
            pulse: 0.0,
            bar: 0.0,
            time: 0.0,
            high: 0.0,
            bass: 0.0,
            chest: Vec3::ZERO,
        }
    }

    /// Le visage actuel.
    #[must_use]
    pub fn face(&self) -> Face {
        self.current
    }

    /// La déchirure du visage, 0..1.
    #[must_use]
    pub fn tear(&self) -> f32 {
        self.swap
    }

    /// Le robot revient au repos (`reset`).
    pub fn reset(&mut self) {
        self.drop_beats = 0;
        self.current = Face::Happy;
        self.last_change = -10.0;
        self.swap = 0.0;
        self.tear_until = f64::NEG_INFINITY;
        self.tone = Vec3::ZERO;
    }

    /// Un kick (`kick`).
    pub fn kick(&mut self, frame: &LandmarkFrame) {
        let spec = &self.spec;
        let count = frame.beat.count.wrapping_add(spec.beat_offset);
        self.drop_beats = self.drop_beats.saturating_sub(1);
        if count.is_multiple_of(spec.every) {
            let base = if frame.drive.section == cf_analysis::Section::Peak || self.drop_beats > 0 {
                Level::Peak
            } else if frame.drive.section == cf_analysis::Section::Calm {
                Level::Calm
            } else {
                Level::Groove
            };
            let rank = Level::ALL.iter().position(|l| *l == base).unwrap_or(1);
            let shifted = Level::ALL[match spec.level_shift {
                -1 => rank.saturating_sub(1),
                1 => (rank + 1).min(2),
                _ => rank,
            }];
            let period = frame.beat.bpm.map_or(0.5, |bpm| 60.0 / bpm);
            self.dance
                .on_beat(count, shifted, frame.time, period * spec.every as f32);
        }
        // En haut d'intensité, le visage alterne entre cyborg et yeux en croix toutes les seize mesures.
        if count.is_multiple_of(DROP_BEATS) {
            self.toggle_aggro();
        }
    }

    /// Un drop.
    pub fn drop_hit(&mut self) {
        self.drop_beats = DROP_BEATS;
        self.toggle_aggro();
    }

    fn toggle_aggro(&mut self) {
        self.aggro = if self.aggro == Face::Cyborg {
            Face::Cross
        } else {
            Face::Cyborg
        };
    }

    fn choose_face(&self, frame: &LandmarkFrame, permitted: bool) -> Face {
        if frame.drive.outro {
            return Face::Wink;
        }
        if permitted && (self.drop_beats > 0 || frame.drive.section == cf_analysis::Section::Peak) {
            return self.aggro;
        }
        if frame.drive.section == cf_analysis::Section::Calm {
            Face::Calm
        } else {
            Face::Happy
        }
    }

    /// Une image. `request_tear` demande au limiteur partagé du chœur une déchirure pour un changement de visage ; `tear_level`
    /// est le niveau borné de ce limiteur, 0..1.
    pub fn step(
        &mut self,
        frame: &LandmarkFrame,
        permitted: bool,
        request_tear: &mut impl FnMut() -> bool,
        tear_level: f32,
    ) {
        let spec = self.spec;
        self.pose = self.dance.update(frame.time);
        self.pulse = frame.beat.pulse;
        self.time = frame.time as f32;
        // Position continue dans la mesure : kicks comptés + phase recalée sur le BPM.
        self.bar = frame.beat.count.wrapping_add(spec.beat_offset) as f32 + frame.phase;
        self.high = damp(self.high, frame.beat.high, 6.0, frame.dt);
        self.bass = damp(self.bass, frame.beat.bass, 6.0, frame.dt);

        let wanted = self.choose_face(frame, permitted);
        let urgent = wanted == Face::Wink || self.drop_beats == DROP_BEATS;
        if wanted != self.current && (urgent || frame.time - self.last_change > FACE_HOLD_S) {
            self.current = wanted;
            self.last_change = frame.time;
            // La déchirure n'a lieu que si le limiteur partagé l'accorde ; sinon le visage change sans se déchirer.
            if permitted && request_tear() {
                self.tear_until = frame.time + f64::from(TEAR_DECAY_S);
            }
        }
        self.swap = if frame.time < self.tear_until && permitted {
            tear_level
        } else {
            0.0
        };
        // La couleur reste celle du robot ; l'humeur du visage ne la teinte que d'un tiers.
        let colors = &frame.colors;
        let target = colors.get(spec.accent).lerp(colors.get(self.current.mood()), 0.3);
        if self.tone == Vec3::ZERO {
            self.tone = colors.get(spec.accent);
        }
        let k = 1.0 - (-6.0 * frame.dt).exp();
        self.tone += (target - self.tone) * k;
    }

    /// La lumière du robot (une seule lumière réelle par robot : à hauteur de la poitrine, large) : position dans le repère
    /// de l'immeuble, rayon, couleur, puissance.
    #[must_use]
    pub fn light(&self, frame: &LandmarkFrame) -> (Vec3, f32, Vec3, f32) {
        let power = 1.0 + frame.beat.pulse * 1.6 + frame.beat.bass * 0.8;
        let s = self.spec.scale;
        (self.chest + Vec3::Y * 12.0 * s, 130.0 * s, self.tone, power * 1.6)
    }

    /// La teinte courante.
    #[must_use]
    pub fn tone(&self) -> Vec3 {
        self.tone
    }
}

#[cfg(test)]
mod tests;
