//! Le passage d'un immeuble-vitrine devant la voiture (`LandmarkEvent` de `traffic/events/landmark.ts`) : où on le pose,
//! quelle zone de la ville on dégage, quand on le range, ce qu'on filme et ce qu'on esquive. Pur : la scène des immeubles
//! ([`LandmarkStage`]) et l'image du trafic suffisent, aucun `World`.
//!
//! « C'est une structure : jamais deux en même temps, jamais avec la route croisée ni la méga-tour. » L'événement du trafic
//! ([`super::event::LandmarkEvent`]) n'est qu'un adaptateur mince autour de ce passage.

use bevy::math::DVec3;

use super::frame::{Drive, FlashPolicy, LandmarkBeat, LandmarkFrame};
use super::landmark::{LandmarkKind, Placement};
use super::part::RoadBand;
use super::stage::LandmarkStage;
use crate::facade::tint_rank;
use crate::pilot::flight::Obstruction;
use crate::scenery::Clearing;
use crate::traffic::TrafficFrame;

/// Distance à laquelle l'immeuble apparaît devant la voiture, et à laquelle il disparaît derrière elle (`AHEAD`, `BEHIND`).
pub const AHEAD_M: f64 = 400.0;
/// Distance derrière la voiture à laquelle l'immeuble est rangé.
pub const BEHIND_M: f64 = 160.0;
/// Marge de la zone dégagée de la ville au-delà de l'emprise de l'immeuble, m.
const CLEARING_MARGIN_M: f64 = 14.0;

/// Comment démarrer un passage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PassStart {
    /// Côté de la route.
    pub side: i8,
    /// Rang de teinte du corps, si l'immeuble n'impose pas la sienne (0..5 : le cycle).
    pub tint: u8,
    /// Échelle du passage.
    pub scale: f32,
    /// Mise au point : distance exacte du centre devant la voiture (m), sinon la distance d'apparition.
    pub debug_center: Option<f32>,
}

/// Le passage d'un immeuble.
#[derive(Debug, Clone, Copy)]
pub struct LandmarkPass {
    kind: LandmarkKind,
    place: Placement,
    span: f64,
    band: Option<RoadBand>,
    last_kick_at: f64,
    clearing: Clearing,
}

impl LandmarkPass {
    /// Pose l'immeuble `kind` devant la voiture (`car_z`) et démarre son passage.
    pub fn begin(stage: &mut LandmarkStage, kind: LandmarkKind, car_z: f64, start: PassStart) -> Self {
        let landmark = stage.landmark(kind);
        let spans_road = landmark.spans_road();
        let scale = if spans_road { 1.0 } else { start.scale };
        let span = f64::from(landmark.span() * scale);
        let z = match start.debug_center {
            Some(center) => car_z - f64::from(center),
            None => car_z - f64::from(landmark.lead().unwrap_or(AHEAD_M as f32)) - span / 2.0,
        };
        // La couleur que l'immeuble impose à son corps (le rouge d'ERROR, le vert du mur de code…) l'emporte sur celle que
        // la ville a tirée.
        let tint = landmark
            .body_tint()
            .and_then(tint_rank)
            .map_or(start.tint, |rank| rank as u8);
        let band = landmark.road_band();
        let place = Placement {
            z,
            side: start.side,
            tint,
            scale,
            spans_road,
        };
        let margin = span / 2.0 + CLEARING_MARGIN_M;
        let clearing = Clearing {
            z0: z - margin,
            z1: z + margin,
            side: if spans_road { 0 } else { start.side },
        };
        stage.show(kind, place);
        Self {
            kind,
            place,
            span,
            band,
            last_kick_at: f64::NEG_INFINITY,
            clearing,
        }
    }

    /// Lequel.
    #[must_use]
    pub fn kind(&self) -> LandmarkKind {
        self.kind
    }

    /// Où il est posé.
    #[must_use]
    pub fn placement(&self) -> &Placement {
        &self.place
    }

    /// La zone sans immeuble que le passage réclame.
    #[must_use]
    pub fn clearing(&self) -> Clearing {
        self.clearing
    }

    /// Centre approximatif de la structure (le sol à mi-hauteur de sa largeur) et sa taille : pour les plans du mode cinéma
    /// (sujet `Landmark`).
    #[must_use]
    pub fn focus(&self) -> (DVec3, f32) {
        let x = self.place.root_position().x;
        (DVec3::new(x, self.span * 0.4, self.place.z), self.span as f32)
    }

    /// La structure à esquiver, si elle enjambe la route.
    #[must_use]
    pub fn obstruction(&self) -> Option<Obstruction> {
        let band = self.band?;
        Some(Obstruction {
            z_near: self.place.z + f64::from(band.half_depth),
            z_far: self.place.z - f64::from(band.half_depth),
            lo: f64::from(band.lo),
            hi: f64::from(band.hi),
        })
    }

    /// Le passage est-il fini ? L'immeuble est dépassé de plus de [`BEHIND_M`].
    #[must_use]
    pub fn finished(&self, car_z: f64) -> bool {
        self.place.z - self.span / 2.0 > car_z + BEHIND_M
    }

    /// L'image du trafic devient celle d'un immeuble (`LandmarkFrame`) ; la phase est recalée sur le tempo.
    fn frame(&mut self, stage: &LandmarkStage, frame: &TrafficFrame<'_>, dt: f32) -> LandmarkFrame {
        let b = frame.beat;
        if b.kick {
            self.last_kick_at = frame.time;
        }
        // Sans tempo connu, la phase suit simplement la retombée du dernier kick.
        let phase = b.bpm.map_or(1.0 - b.pulse, |bpm| {
            (((frame.time - self.last_kick_at) as f32) * bpm / 60.0).clamp(0.0, 1.0)
        });
        LandmarkFrame {
            time: frame.time,
            dt,
            beat: LandmarkBeat {
                kick: b.kick,
                bass: b.bass,
                mid: b.mid,
                high: b.high,
                loud: b.loud,
                // Le pouls brut : la scène le remplace par celui du limiteur de flashs.
                pulse: b.pulse,
                count: b.count,
                bpm: b.bpm,
                beat: stage.inputs.beat,
            },
            drive: Drive {
                drop: frame.drive.drop,
                intensity: frame.drive.intensity,
                section: frame.drive.section,
                mood: stage.inputs.mood,
                outro: false,
            },
            car: frame.car,
            calm: frame.calm,
            phase,
            policy: frame.show.map_or(FlashPolicy::NONE, |s| FlashPolicy {
                flashes: s.flashes,
                breathing: s.breathing,
            }),
            colors: stage.inputs.colors,
        }
    }

    /// Avance le passage d'une image. Faux quand l'immeuble est dépassé.
    pub fn update(&mut self, stage: &mut LandmarkStage, frame: &TrafficFrame<'_>, dt: f32) -> bool {
        if self.finished(frame.car.z) {
            return false;
        }
        let mut f = self.frame(stage, frame, dt);
        stage.step(&mut f);
        true
    }

    /// Range l'immeuble (y compris de force).
    pub fn end(&mut self, stage: &mut LandmarkStage) {
        if stage.active().is_some_and(|a| a.kind == self.kind) {
            stage.hide();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start(side: i8) -> PassStart {
        PassStart {
            side,
            tint: 2,
            scale: 1.0,
            debug_center: None,
        }
    }

    fn frame_at(z: f64, time: f64) -> TrafficFrame<'static> {
        TrafficFrame {
            car: DVec3::new(0.0, 0.0, z),
            time,
            ..TrafficFrame::default()
        }
    }

    #[test]
    fn l_immeuble_apparait_a_quatre_cents_metres_moins_la_demi_largeur() {
        let mut stage = LandmarkStage::new();
        let pass = LandmarkPass::begin(&mut stage, LandmarkKind::Lava, -1000.0, start(1));
        let span = f64::from(stage.landmark(LandmarkKind::Lava).span());
        assert!((pass.placement().z - (-1000.0 - 400.0 - span / 2.0)).abs() < 1e-9);
        assert_eq!(stage.active().map(|a| a.kind), Some(LandmarkKind::Lava));
    }

    #[test]
    fn l_enseigne_phare_apparait_plus_loin_et_le_rouge_est_impose() {
        let mut stage = LandmarkStage::new();
        let pass = LandmarkPass::begin(&mut stage, LandmarkKind::Error, 0.0, start(-1));
        let span = f64::from(stage.landmark(LandmarkKind::Error).span());
        assert!((pass.placement().z - (-620.0 - span / 2.0)).abs() < 1e-9);
        let alarm = tint_rank(cf_render::SceneColor::Alarm).map(|r| r as u8);
        assert_eq!(Some(pass.placement().tint), alarm);
    }

    #[test]
    fn la_mise_au_point_place_le_centre_a_la_distance_demandee() {
        let mut stage = LandmarkStage::new();
        let pass = LandmarkPass::begin(
            &mut stage,
            LandmarkKind::Board,
            -50.0,
            PassStart {
                debug_center: Some(120.0),
                ..start(1)
            },
        );
        assert!((pass.placement().z + 170.0).abs() < 1e-9);
    }

    #[test]
    fn la_zone_degagee_deborde_de_quatorze_metres_et_une_arche_la_prend_des_deux_cotes() {
        let mut stage = LandmarkStage::new();
        let pass = LandmarkPass::begin(&mut stage, LandmarkKind::Matrix, 0.0, start(1));
        let c = pass.clearing();
        let span = f64::from(stage.landmark(LandmarkKind::Matrix).span());
        assert!((c.z1 - c.z0 - (span + 28.0)).abs() < 1e-9);
        assert_eq!(c.side, 1);
        let arch = LandmarkPass::begin(&mut stage, LandmarkKind::Arch, 0.0, start(1));
        assert_eq!(arch.clearing().side, 0);
    }

    #[test]
    fn le_sujet_est_au_bord_de_la_route_a_quarante_pour_cent_de_la_largeur() {
        let mut stage = LandmarkStage::new();
        let pass = LandmarkPass::begin(&mut stage, LandmarkKind::Flux, 0.0, start(-1));
        let (position, size) = pass.focus();
        assert!((position.x + 17.0).abs() < 1e-9);
        assert!((f64::from(size) * 0.4 - position.y).abs() < 1e-3);
        assert!((position.z - pass.placement().z).abs() < 1e-9);
        let arch = LandmarkPass::begin(&mut stage, LandmarkKind::Arch, 0.0, start(1));
        assert!(arch.focus().0.x.abs() < 1e-9, "l'arche est au centre de la route");
    }

    #[test]
    fn seules_les_structures_qui_enjambent_publient_une_obstruction() {
        let mut stage = LandmarkStage::new();
        let lava = LandmarkPass::begin(&mut stage, LandmarkKind::Lava, 0.0, start(1));
        assert!(lava.obstruction().is_none());
        let arch = LandmarkPass::begin(&mut stage, LandmarkKind::Arch, 0.0, start(1));
        let o = arch.obstruction().expect("l'arche enjambe");
        assert!(o.z_near > o.z_far && o.lo < o.hi);
        assert!(
            (o.z_near - arch.placement().z - 5.2).abs() < 1e-6,
            "demi-profondeur 2,2 + 3"
        );
        let mecha = LandmarkPass::begin(&mut stage, LandmarkKind::Mecha, 0.0, start(1));
        let m = mecha.obstruction().expect("les bras des mecha");
        assert!((m.lo - 36.0).abs() < 1e-9 && (m.hi - 110.0).abs() < 1e-9);
    }

    #[test]
    fn l_immeuble_est_range_cent_soixante_metres_apres_son_bord() {
        let mut stage = LandmarkStage::new();
        let mut pass = LandmarkPass::begin(&mut stage, LandmarkKind::Board, 0.0, start(1));
        let z = pass.placement().z;
        let span = f64::from(stage.landmark(LandmarkKind::Board).span());
        let edge = z - span / 2.0;
        assert!(pass.update(&mut stage, &frame_at(edge + 100.0 - 1.0, 1.0), 1.0 / 60.0));
        assert!(!pass.update(&mut stage, &frame_at(edge - 161.0, 2.0), 1.0 / 60.0));
        pass.end(&mut stage);
        assert!(stage.active().is_none());
    }

    #[test]
    fn un_pas_ecrit_la_pose_des_pieces_et_la_phase_suit_le_tempo() {
        let mut stage = LandmarkStage::new();
        let mut pass = LandmarkPass::begin(&mut stage, LandmarkKind::Binary, 0.0, start(1));
        let mut frame = frame_at(0.0, 10.0);
        frame.beat.kick = true;
        frame.beat.bpm = Some(120.0);
        assert!(pass.update(&mut stage, &frame, 1.0 / 60.0));
        frame.beat.kick = false;
        frame.time = 10.25;
        assert!(pass.update(&mut stage, &frame, 1.0 / 60.0));
        // 0,25 s à 120 BPM : la moitié d'un temps.
        assert!((stage.uniform().phase - 0.5).abs() < 1e-4, "{}", stage.uniform().phase);
        assert!(stage.paint(LandmarkKind::Binary)[0].transform.translation.y > 0.0);
    }
}
