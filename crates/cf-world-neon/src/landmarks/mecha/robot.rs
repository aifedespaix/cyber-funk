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

use bevy::math::{EulerRot, Mat4, Quat, Vec3, Vec4};
use bevy::transform::components::Transform;
use cf_render::SceneColor;

use super::dance::{Choreography, Level, Pose};
use crate::landmarks::frame::LandmarkFrame;
use crate::landmarks::math::damp;
use crate::landmarks::part::{Look, MeshSpec, PartPaint, PartSpec};

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
            aggro: if spec.beat_offset % 2 == 0 {
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
        if count % spec.every == 0 {
            let base = if frame.drive.section == cf_analysis::Section::Peak || self.drop_beats > 0 {
                Level::Peak
            } else if frame.drive.section == cf_analysis::Section::Calm {
                Level::Calm
            } else {
                Level::Groove
            };
            let index = Level::ALL.iter().position(|l| *l == base).unwrap_or(1) as i32 + i32::from(spec.level_shift);
            let shifted = Level::ALL[index.clamp(0, 2) as usize];
            let period = frame.beat.bpm.map_or(0.5, |bpm| 60.0 / bpm);
            self.dance
                .on_beat(count, shifted, frame.time, period * spec.every as f32);
        }
        // En haut d'intensité, le visage alterne entre cyborg et yeux en croix toutes les seize mesures.
        if count % DROP_BEATS == 0 {
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

    /// Écrit la pose et les paramètres des quatorze pièces dans `out` (dans l'ordre de [`parts`]) et retient la position
    /// de la poitrine pour la lumière.
    pub fn paint(&mut self, out: &mut [PartPaint]) {
        let spec = self.spec;
        let side = f32::from(spec.side);
        let (pose, pulse, bar, time) = (self.pose, self.pulse, self.bar, self.time);
        let (ls, le, rs, re) = if spec.mirror {
            (
                pose.right_shoulder,
                pose.right_elbow,
                pose.left_shoulder,
                pose.left_elbow,
            )
        } else {
            (
                pose.left_shoulder,
                pose.left_elbow,
                pose.right_shoulder,
                pose.right_elbow,
            )
        };
        let twist = (if spec.mirror { -pose.twist } else { pose.twist }) * spec.twist_gain;
        let torso_y =
            twist + (bar * std::f32::consts::FRAC_PI_2).sin() * 0.1 * spec.twist_gain + (time * 0.9).sin() * 0.02;
        let torso_rot = Quat::from_euler(EulerRot::XYZ, -0.05 * pulse, torso_y, 0.0);
        let head_rot = Quat::from_euler(
            EulerRot::XYZ,
            -0.2 * pulse,
            ((bar * std::f32::consts::FRAC_PI_4) * spec.look_speed).sin() * 0.35,
            (bar * std::f32::consts::PI).sin() * 0.06,
        );
        let drop = pose.squat * 2.5 + pulse * 1.2;
        let hips_y = LEG_H - drop;
        let leg_scale = (LEG_H - drop) / LEG_H;

        let facing = Quat::from_rotation_y(if spec.side > 0 {
            -std::f32::consts::FRAC_PI_2
        } else {
            std::f32::consts::FRAC_PI_2
        });
        let group = Mat4::from_scale_rotation_translation(
            Vec3::splat(spec.scale),
            facing,
            Vec3::new(side * spec.x, 0.0, spec.z),
        );
        let t = |x: f32, y: f32, z: f32| Mat4::from_translation(Vec3::new(x, y, z));
        let sc = |x: f32, y: f32, z: f32| Mat4::from_scale(Vec3::new(x, y, z));

        let torso = group * t(0.0, hips_y, 0.0) * Mat4::from_quat(torso_rot);
        let head = torso * t(0.0, TORSO_H + 4.0, 0.0) * Mat4::from_quat(head_rot);
        let mut seed = spec.seed;
        let mut put = |slot: usize, matrix: Mat4, size: Vec3, out: &mut [PartPaint], count_seed: bool| {
            let this_seed = seed;
            if count_seed {
                seed += 1;
            }
            if let Some(part) = out.get_mut(slot) {
                *part = PartPaint {
                    visible: true,
                    transform: Transform::from_matrix(matrix * Mat4::from_scale(size)),
                    mood: self.tone,
                    a: (size * 0.5).extend(this_seed as f32),
                    c: Vec4::new(self.high, self.bass, 0.0, 0.0),
                    ..PartPaint::default()
                };
            }
        };
        // Jambes : deux piliers qui plongent sous la route, mis à l'échelle depuis le sol pour fléchir sur les temps forts.
        for (slot, x) in [(0, -9.0), (1, 9.0)] {
            put(
                slot,
                group * t(x, 0.0, 0.0) * sc(1.0, leg_scale, 1.0) * t(0.0, (LEG_H - LEG_DOWN) / 2.0, 0.0),
                Vec3::new(10.0, LEG_H + LEG_DOWN, 14.0),
                out,
                true,
            );
        }
        put(
            2,
            torso * t(0.0, TORSO_H / 2.0, 0.0),
            Vec3::new(TORSO_W, TORSO_H, TORSO_D),
            out,
            true,
        );
        put(
            3,
            torso * t(0.0, TORSO_H + 2.0, 0.0),
            Vec3::new(8.0, 4.0, 8.0),
            out,
            true,
        );
        put(4, head * t(0.0, 10.0, 0.0), Vec3::new(24.0, 20.0, 18.0), out, true);
        // L'écran : un plan sur la face de la tête, sans échelle (le maillage fait déjà 20 × 16).
        if let Some(screen) = out.get_mut(5) {
            *screen = PartPaint {
                visible: true,
                transform: Transform::from_matrix(head * t(0.0, 10.0, 9.06)),
                mood: self.tone,
                a: Vec4::new(f32::from(self.current as u8), self.swap, 0.0, 0.0),
                ..PartPaint::default()
            };
        }
        // Épaule → bras → coude → avant-bras → main : trois pivots emboîtés par côté. Le bras gauche tourne dans le sens
        // inverse du droit (`rotation.z = −angle` à gauche).
        for (base, sign, shoulder_angle, elbow_angle) in [(6, -1.0_f32, -ls, -le), (10, 1.0, rs, re)] {
            let shoulder =
                torso * t(sign * (TORSO_W / 2.0 + 4.0), TORSO_H - 5.0, 0.0) * Mat4::from_rotation_z(shoulder_angle);
            let elbow = shoulder * t(0.0, -UPPER, 0.0) * Mat4::from_rotation_z(elbow_angle);
            put(base, shoulder, Vec3::splat(9.0), out, true);
            put(
                base + 1,
                shoulder * t(0.0, -UPPER / 2.0, 0.0),
                Vec3::new(7.0, UPPER, 7.0),
                out,
                true,
            );
            put(
                base + 2,
                elbow * t(0.0, -FORE / 2.0, 0.0),
                Vec3::new(6.2, FORE, 6.2),
                out,
                true,
            );
            put(base + 3, elbow * t(0.0, -FORE - 3.0, 0.0), Vec3::splat(9.0), out, true);
        }
        self.chest = (torso * t(0.0, TORSO_H * 0.6, TORSO_D / 2.0 + 3.0)).transform_point3(Vec3::ZERO);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> RobotSpec {
        RobotSpec {
            side: 1,
            x: 29.0,
            z: -100.0,
            scale: 1.2,
            accent: SceneColor::Sun,
            beat_offset: 0,
            level_shift: 0,
            mirror: false,
            every: 1,
            twist_gain: 1.0,
            look_speed: 1.0,
            slot: 0,
            seed: 9,
        }
    }

    fn live() -> LandmarkFrame {
        let mut frame = LandmarkFrame::rest();
        frame.policy = crate::landmarks::frame::FlashPolicy {
            flashes: true,
            breathing: false,
        };
        frame
    }

    #[test]
    fn un_robot_a_quatorze_pieces_dont_l_ecran() {
        let list = parts();
        assert_eq!(list.len(), PARTS);
        assert_eq!(list.iter().filter(|p| p.look == Look::MechaFace).count(), 1);
        assert_eq!(list.iter().filter(|p| p.look == Look::MechaPanel).count(), BLOCKS);
    }

    #[test]
    fn la_pose_place_les_os_les_graines_se_suivent_et_les_jambes_plongent() {
        let mut robot = Robot::new(spec());
        let frame = live();
        robot.step(&frame, true, &mut || false, 0.0);
        let mut out = vec![PartPaint::default(); PARTS];
        robot.paint(&mut out);
        let seeds: Vec<f32> = [0, 1, 2, 3, 4, 6, 7, 8, 9, 10, 11, 12, 13]
            .iter()
            .map(|i| out[*i].a.w)
            .collect();
        assert_eq!(seeds, (9..22).map(|s| s as f32).collect::<Vec<_>>());
        // Les pieds sont à des centaines de mètres sous la route.
        let leg = &out[0];
        let bottom = leg.transform.translation.y - leg.transform.scale.y / 2.0;
        assert!(bottom < -400.0, "pieds à {bottom}");
        for part in &out {
            assert!(part.transform.translation.is_finite() && part.transform.scale.is_finite());
        }
    }

    #[test]
    fn les_bras_miroir_echangent_les_epaules() {
        let mut a = Robot::new(spec());
        let mut b = Robot::new(RobotSpec { mirror: true, ..spec() });
        let mut frame = live();
        frame.time = 1.0;
        for robot in [&mut a, &mut b] {
            robot.kick(&frame);
            robot.step(&frame, true, &mut || false, 0.0);
            frame.time += 0.3;
            robot.step(&frame, true, &mut || false, 0.0);
        }
        let (mut pa, mut pb) = (vec![PartPaint::default(); PARTS], vec![PartPaint::default(); PARTS]);
        a.paint(&mut pa);
        b.paint(&mut pb);
        // Les mains ne sont pas aux mêmes endroits (les deux poses de la séquence ne sont pas symétriques).
        assert!(
            pa[9].transform.translation.distance(pb[9].transform.translation) > 1e-3
                || pa[13].transform.translation.distance(pb[13].transform.translation) > 1e-3
        );
    }

    #[test]
    fn au_repos_le_visage_reste_calme_ou_souriant_et_ne_se_dechire_jamais() {
        let mut robot = Robot::new(spec());
        let mut frame = LandmarkFrame::rest();
        frame.drive.section = cf_analysis::Section::Peak;
        robot.drop_hit();
        for i in 0..600 {
            frame.time = f64::from(i) / 60.0;
            robot.step(&frame, false, &mut || true, 1.0);
            assert!(!robot.face().aggressive());
            assert!(robot.tear().abs() < f32::EPSILON);
        }
    }

    #[test]
    fn au_sommet_permis_le_visage_devient_agressif_et_le_changement_demande_la_dechirure() {
        let mut robot = Robot::new(spec());
        let mut frame = live();
        frame.drive.section = cf_analysis::Section::Peak;
        let mut requests = 0;
        for i in 0..120 {
            frame.time = f64::from(i) / 60.0;
            robot.step(
                &frame,
                true,
                &mut || {
                    requests += 1;
                    true
                },
                0.5,
            );
        }
        assert!(robot.face().aggressive());
        assert_eq!(requests, 1, "un seul changement, une seule demande");
    }

    #[test]
    fn un_visage_ne_change_pas_plus_d_une_fois_par_seconde() {
        let mut robot = Robot::new(spec());
        let mut frame = live();
        let mut changes = 0;
        let mut last = robot.face();
        for i in 0..1200 {
            frame.time = f64::from(i) / 60.0;
            // La section alterne à chaque image : le visage doit rester stable entre deux tenues d'une seconde.
            frame.drive.section = if i % 2 == 0 {
                cf_analysis::Section::Calm
            } else {
                cf_analysis::Section::Groove
            };
            robot.step(&frame, true, &mut || false, 0.0);
            if robot.face() != last {
                changes += 1;
                last = robot.face();
            }
        }
        assert!(changes <= 20, "{changes} changements en 20 s");
    }

    #[test]
    fn la_lumiere_est_a_hauteur_de_poitrine_et_large() {
        let mut robot = Robot::new(spec());
        let frame = live();
        robot.step(&frame, true, &mut || false, 0.0);
        let mut out = vec![PartPaint::default(); PARTS];
        robot.paint(&mut out);
        let (position, radius, _, power) = robot.light(&frame);
        assert!((radius - 130.0 * 1.2).abs() < 1e-3);
        assert!(power >= 1.6);
        assert!(position.y > 30.0 && position.y < 80.0 * 1.2, "{position:?}");
    }
}
