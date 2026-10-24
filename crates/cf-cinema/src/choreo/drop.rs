//! La frappe du drop : coupe franche vers le plan prévu par `DropConductor`, la cascade, le bullet-time ou un plan
//! énergique ; parfois une salve de coupes d'une mesure.

use super::{
    BULLET_CHANCE, Choreography, HIT_GUARD, PENDING_SECONDS, SALVO_GAP, SALVO_GAP_SPAN, SALVO_HISTORY, ShowSide,
};
use crate::cutter::Decision;
use crate::editor::{Editor, PickMode};
use crate::shot::ShotSpec;
use crate::types::TransitionKind;
use crate::world::CinemaFrame;

/// Plan de la cascade en cours (`stunt-cam`) et celui de l'Ascension.
const STUNT_CAM: &str = "stunt-cam";
const ASCENSION_CAM: &str = "ascension-cam";
/// Mesure la plus courte qu'on suppose pour dimensionner une salve (BPM) : en dessous, le tempo est douteux.
const MIN_BPM: f32 = 60.0;
const DEFAULT_BPM: f32 = 120.0;

impl Choreography {
    /// Le drop frappe.
    pub(super) fn on_drop(
        &mut self,
        frame: &CinemaFrame<'_>,
        elapsed: f64,
        spec: Option<usize>,
        editor: &mut Editor,
        catalog: &[Box<dyn ShotSpec>],
        show: &dyn ShowSide,
    ) -> Option<Decision> {
        self.prepared = false;
        self.salvo_left = 0;
        self.pending = None;
        if frame.show.breathing || elapsed < HIT_GUARD {
            return None;
        }

        let plan = show.drop_plan();
        let plan_shot = plan.and_then(|p| p.shot.as_ref()).map(cf_show::model::ShotId::as_str);
        let stunt = plan
            .and_then(|p| p.stunt.as_ref())
            .map(cf_show::model::StuntKind::as_str);
        let mut id = plan_shot;
        if let Some(kind) = stunt {
            id = Some(if kind == "ascension" {
                ASCENSION_CAM
            } else {
                id.unwrap_or(STUNT_CAM)
            });
        } else if id == Some(STUNT_CAM) && frame.stunt.is_some_and(|s| s.kind == "ascension") {
            id = Some(ASCENSION_CAM);
        }

        if let Some(id) = id {
            let wanted = editor.find(id);
            if wanted.is_some() && spec == wanted {
                return None;
            }
            if let Some(index) = wanted
                && editor.can_use(frame, show, catalog, index, false)
            {
                if stunt.is_none() {
                    self.maybe_salvo(frame, show);
                }
                return Some(Decision {
                    spec: Some(index),
                    kind: Some(TransitionKind::Cut),
                    counted: plan_shot == Some(id),
                    ..Decision::of(PickMode::Drop)
                });
            }
            if stunt.is_some() {
                // La cascade n'a pas encore son plan (la voiture n'a pas décollé) : on le reprend dès qu'il est prêt.
                self.pending = wanted;
                self.pending_until = frame.time + PENDING_SECONDS;
                return None;
            }
        } else if spec
            .and_then(|i| catalog.get(i))
            .is_some_and(|s| s.id() == "dolly-zoom")
        {
            // Le travelling compensé finit sur le drop : on le laisse aller au bout.
            return None;
        }

        if let Some(bullet) = editor.find("bullet-time")
            && spec != Some(bullet)
            && self.rng.next_f32() < BULLET_CHANCE
            && editor.can_use(frame, show, catalog, bullet, false)
        {
            return Some(Decision {
                spec: Some(bullet),
                kind: Some(TransitionKind::Cut),
                ..Decision::of(PickMode::Drop)
            });
        }
        self.maybe_salvo(frame, show);
        Some(Decision {
            kind: Some(TransitionKind::Cut),
            ..Decision::of(PickMode::Drop)
        })
    }

    /// Une salve de coupes d'une mesure, au plus toutes les 3 à 4 minutes, plus souvent quand le spectacle chauffe.
    fn maybe_salvo(&mut self, frame: &CinemaFrame<'_>, show: &dyn ShowSide) {
        let now = frame.time;
        if !show.gates().is_some_and(|g| g.salvo) || now - self.last_salvo < self.salvo_gap {
            return;
        }
        if f64::from(self.rng.next_f32()) > 0.45 + 0.45 * f64::from(show.heat().clamp(0.0, 1.0)) {
            return;
        }
        // 3 ou 4 coupes d'une mesure, la coupe du drop comprise.
        self.salvo_left = 2 + u32::from(self.rng.next_f32() < 0.5);
        let bpm = frame.beat.bpm.filter(|b| *b > 0.0).unwrap_or(DEFAULT_BPM).max(MIN_BPM);
        let bar = f64::from(240.0 / bpm);
        self.salvo_until = now + bar * f64::from(self.salvo_left + 1) + 3.0;
        self.last_salvo = now;
        self.salvo_gap = SALVO_GAP + f64::from(self.rng.next_f32()) * SALVO_GAP_SPAN;
        if self.salvo_at.len() == SALVO_HISTORY {
            self.salvo_at.remove(0);
        }
        self.salvo_at.push(now);
    }
}
