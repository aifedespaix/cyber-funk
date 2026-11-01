//! La colle du moteur de quartiers, sans Bevy dans la logique : l'instantané du gouverneur, la vitesse de la voiture, la
//! fin d'événement détectée d'une image à l'autre, l'avancement de la transition et la copie lisible
//! ([`DistrictState`]). Les systèmes (`systems.rs`) ne font que lire les ressources et appliquer les [`Change`] rendus.

use bevy::prelude::Resource;
use cf_analysis::Section;
use cf_render::{DISTRICT_HISTORY, DistrictOrder, DistrictState};
use cf_show::district::{Decision, DistrictId, Governor, GovernorSnapshot, TRANSITION_MAX_S, TransitionKind};

use crate::world::{AHEAD, CHUNK_LENGTH_M};

/// Zone de dégradé la plus courte, en rangées : à l'arrêt, le mélange reste lisible.
pub const MIN_BLEND_ROWS: u32 = 4;
/// Zone de dégradé la plus longue, en rangées : la fenêtre de streaming devant la voiture.
pub const MAX_BLEND_ROWS: u32 = AHEAD;
/// Raideur du lissage de la vitesse mesurée (1/s), comme celle du trafic.
const SPEED_SMOOTHING: f32 = 6.0;
/// Au-delà, la mesure est un saut (reprise, téléportation) et non une vitesse.
const MAX_SPEED_MPS: f32 = 400.0;

/// Le gouverneur de quartiers (`cf-show`), seedé par la graine du monde.
#[derive(Resource, Debug, Clone)]
pub struct DistrictBrain(pub Governor);

/// Ce que le monde voit à cette image, pour le gouverneur et la transition.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy)]
pub struct DistrictInput {
    /// Temps audio (s) : l'horloge du gouverneur.
    pub now_s: f64,
    /// Durée de l'image (s) : l'horloge de la transition, qui avance même si l'audio cale.
    pub dt_s: f32,
    /// Section du morceau.
    pub section: Section,
    /// La section vient de changer.
    pub section_changed: bool,
    /// Intensité, 0..1.
    pub intensity: f32,
    /// Basses, 0..1.
    pub bass: f32,
    /// Aigus, 0..1.
    pub high: f32,
    /// Tempo connu.
    pub bpm: Option<f32>,
    /// Un drop se prépare (`drop_plan().is_some()`).
    pub drop_imminent: bool,
    /// Le drop frappe à cette image.
    pub drop_now: bool,
    /// Un show est chargé.
    pub show_present: bool,
    /// Événements du trafic en cours (`None` sans trafic).
    pub active_events: Option<usize>,
    /// Position absolue de la voiture le long de la route (z, m), si elle existe.
    pub car_z: Option<f64>,
}

/// Une transition à lancer : le système pousse le segment dans le plan et demande l'étalonnage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    /// Quartier visé.
    pub to: DistrictId,
    /// Transition demandée (le dégradé est joué si elle n'existe pas encore).
    pub transition: TransitionKind,
    /// Longueur de la zone de dégradé, en rangées.
    pub blend_rows: u32,
}

/// La transition en cours.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Running {
    elapsed_s: f32,
    /// Première rangée après la zone de dégradé, si le segment a été posé dans le plan.
    blend_end: Option<i64>,
}

/// L'état de la colle d'une image à l'autre.
#[derive(Resource, Debug, Clone, Default)]
pub struct DistrictDriver {
    speed_mps: f32,
    last_z: Option<f64>,
    last_events: Option<usize>,
    running: Option<Running>,
}

impl DistrictDriver {
    /// Vitesse lissée de la voiture, en m/s.
    #[must_use]
    pub fn speed_mps(&self) -> f32 {
        self.speed_mps
    }

    /// Longueur du dégradé : la distance parcourue en [`TRANSITION_MAX_S`] à la vitesse courante (spec, règle 7),
    /// bornée à [`MIN_BLEND_ROWS`]..=[`MAX_BLEND_ROWS`].
    #[must_use]
    pub fn blend_rows(&self) -> u32 {
        let rows = (f64::from(self.speed_mps) * TRANSITION_MAX_S / f64::from(CHUNK_LENGTH_M)).ceil();
        if rows.is_finite() {
            (rows.max(0.0) as u32).clamp(MIN_BLEND_ROWS, MAX_BLEND_ROWS)
        } else {
            MIN_BLEND_ROWS
        }
    }

    /// Applique un ordre de l'interface. Un forçage contourne les règles (et remplace une transition en cours) : il
    /// rend la transition à lancer.
    pub fn apply_order(
        &mut self,
        governor: &mut Governor,
        order: DistrictOrder,
        now_s: f64,
        state: &mut DistrictState,
    ) -> Option<Change> {
        match order {
            DistrictOrder::SetFrozen(frozen) => {
                governor.set_frozen(frozen);
                state.frozen = frozen;
                None
            }
            DistrictOrder::Force { to, transition } => {
                governor.force(to, transition, now_s);
                copy_history(governor, state);
                Some(Change {
                    to,
                    transition,
                    blend_rows: self.blend_rows(),
                })
            }
        }
    }

    /// Une transition démarre : `from_index` est le début du segment posé dans le plan (`None` s'il n'a pas pu l'être,
    /// sans monde ni voiture : seul l'étalonnage change alors).
    pub fn begin(&mut self, change: &Change, from_index: Option<i64>, state: &mut DistrictState) {
        self.running = Some(Running {
            elapsed_s: 0.0,
            blend_end: from_index.map(|from| from.saturating_add(i64::from(change.blend_rows))),
        });
        state.target = Some(change.to);
        state.transition = change.transition;
        state.fallback = !change.transition.implemented();
        state.progress = 0.0;
    }

    /// Une image : mesure la vitesse, détecte la fin d'un événement, avance la transition, consulte le gouverneur.
    /// Rend la transition à lancer si le gouverneur décide un changement. Sans allocation hors changement.
    pub fn tick(
        &mut self,
        governor: &mut Governor,
        input: &DistrictInput,
        state: &mut DistrictState,
    ) -> Option<Change> {
        self.measure_speed(input);
        // Un événement de moins qu'à l'image précédente : il vient de finir (une seule image).
        let event_ended = matches!((self.last_events, input.active_events), (Some(before), Some(now)) if now < before);
        self.last_events = input.active_events;

        self.advance(governor, input, state);

        let decision = governor.tick(&GovernorSnapshot {
            now_s: input.now_s,
            section: input.section,
            section_changed: input.section_changed,
            intensity: input.intensity,
            bass: input.bass,
            high: input.high,
            bpm: input.bpm,
            drop_imminent: input.drop_imminent,
            drop_now: input.drop_now,
            event_ended,
            show_present: input.show_present,
        });
        copy_history(governor, state);
        match decision {
            Decision::Stay { reason } => {
                state.last_reason = reason;
                None
            }
            Decision::Change { to, transition } => Some(Change {
                to,
                transition,
                blend_rows: self.blend_rows(),
            }),
        }
    }

    /// Vitesse mesurée sur la position de la voiture (positive vers −z), lissée.
    fn measure_speed(&mut self, input: &DistrictInput) {
        if let (Some(last), Some(z)) = (self.last_z, input.car_z)
            && input.dt_s > 0.0
        {
            let v = ((last - z) / f64::from(input.dt_s)) as f32;
            if v.is_finite() && (0.0..MAX_SPEED_MPS).contains(&v) {
                self.speed_mps += (v - self.speed_mps) * (1.0 - (-input.dt_s * SPEED_SMOOTHING).exp());
            }
        }
        self.last_z = input.car_z;
    }

    /// Avance la transition : elle finit après [`TRANSITION_MAX_S`] ou quand la voiture a passé la zone de dégradé
    /// (le premier des deux). Le quartier affiché devient alors la cible.
    fn advance(&mut self, governor: &mut Governor, input: &DistrictInput, state: &mut DistrictState) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        running.elapsed_s += input.dt_s.max(0.0);
        let by_time = running.elapsed_s / TRANSITION_MAX_S as f32;
        let car_row = input.car_z.map(|z| (-z / f64::from(CHUNK_LENGTH_M)).floor() as i64);
        let passed = matches!((car_row, running.blend_end), (Some(row), Some(end)) if row >= end);
        state.progress = by_time.clamp(0.0, 1.0);
        if by_time >= 1.0 || passed {
            self.running = None;
            governor.transition_done(input.now_s);
            state.current = state.target.take().unwrap_or(state.current);
            state.progress = 1.0;
        }
    }
}

/// Recopie l'historique du gouverneur (au plus [`DISTRICT_HISTORY`] quartiers) sans allouer.
fn copy_history(governor: &Governor, state: &mut DistrictState) {
    state.history = [None; DISTRICT_HISTORY];
    let history = governor.history();
    let skip = history.len().saturating_sub(DISTRICT_HISTORY);
    for (slot, id) in state.history.iter_mut().zip(history.iter().skip(skip)) {
        *slot = Some(*id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_degrade_suit_la_vitesse_et_reste_borne() {
        let mut driver = DistrictDriver::default();
        assert_eq!(driver.blend_rows(), MIN_BLEND_ROWS);
        driver.speed_mps = 60.0;
        // 60 m/s × 10 s = 600 m = 23,1 rangées de 26 m.
        assert_eq!(driver.blend_rows(), 24);
        driver.speed_mps = 399.0;
        assert_eq!(driver.blend_rows(), MAX_BLEND_ROWS);
    }
}
