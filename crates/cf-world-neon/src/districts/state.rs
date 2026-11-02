//! La colle du moteur de quartiers, sans Bevy dans la logique : l'instantané du gouverneur, la vitesse de la voiture, la
//! fin d'événement détectée d'une image à l'autre, l'avancement de la transition et la copie lisible
//! ([`DistrictState`]). Les systèmes (`systems.rs`) ne font que lire les ressources et appliquer les [`Change`] rendus.

use bevy::prelude::Resource;
use cf_analysis::Section;
use cf_render::palette::MAX_WAIT;
use cf_render::{DISTRICT_HISTORY, DistrictGradeState, DistrictOrder, DistrictState};
use cf_show::district::{
    Decision, DistrictId, Governor, GovernorSnapshot, StayReason, TRANSITION_MAX_S, TransitionKind,
};

use crate::world::{AHEAD, CHUNK_LENGTH_M};

/// Zone de dégradé la plus courte, en rangées : à l'arrêt, le mélange reste lisible.
pub const MIN_BLEND_ROWS: u32 = 4;
/// Zone de dégradé la plus longue, en rangées : la fenêtre de streaming devant la voiture.
pub const MAX_BLEND_ROWS: u32 = AHEAD;
/// Raideur du lissage de la vitesse mesurée (1/s), comme celle du trafic.
const SPEED_SMOOTHING: f32 = 6.0;
/// Au-delà, la mesure est un saut (reprise, téléportation) et non une vitesse.
const MAX_SPEED_MPS: f32 = 400.0;
/// Durée de transition la plus courte réglable depuis F4 (s) ; la plus longue est [`TRANSITION_MAX_S`].
pub const MIN_FADE_S: f32 = 1.0;
/// Approche la plus longue (s) : passé ce délai, l'étalonnage part même si la voiture n'a pas atteint la zone (voiture
/// arrêtée), pour qu'elle ne bloque jamais le moteur.
pub const APPROACH_MAX_S: f32 = 60.0;
/// Retenue la plus longue (s) du départ de la couleur une fois la voiture dans la zone, quand la musique l'interdit
/// (Peak, drop imminent, après un drop). Passé ce délai la couleur part quand même : un Peak sans fin ne doit pas laisser
/// la ville nouvelle sous l'ancienne couleur indéfiniment.
pub const MUSIC_HOLD_MAX_S: f32 = 45.0;

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
    /// Forçage de debug : la couleur part à l'entrée de la zone même si la musique l'interdit.
    pub forced: bool,
}

/// La transition en cours.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Running {
    to: DistrictId,
    blend_rows: u32,
    /// Première rangée de la zone de dégradé, si le segment a été posé dans le plan.
    from: Option<i64>,
    /// Première rangée après la zone de dégradé.
    blend_end: Option<i64>,
    /// Temps passé à approcher la zone (s).
    approach_s: f32,
    /// Temps passé depuis le départ de la couleur (s), `None` tant que la voiture approche ou que la musique retient.
    zone_s: Option<f32>,
    /// Temps passé dans la zone à attendre que la musique permette le départ de la couleur (s).
    held_s: f32,
    /// Forçage : jamais retenu par la musique.
    forced: bool,
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

    /// Longueur du dégradé : la distance parcourue en `fade_s` secondes à la vitesse courante (spec, règle 7), bornée à
    /// [`MIN_BLEND_ROWS`]..=[`MAX_BLEND_ROWS`].
    #[must_use]
    pub fn blend_rows(&self, fade_s: f32) -> u32 {
        let rows = (f64::from(self.speed_mps) * f64::from(fade_s) / f64::from(CHUNK_LENGTH_M)).ceil();
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
            DistrictOrder::SetFadeSeconds(seconds) => {
                if seconds.is_finite() {
                    state.fade_s = seconds.clamp(MIN_FADE_S, TRANSITION_MAX_S as f32);
                }
                None
            }
            DistrictOrder::Force { to, transition } => {
                governor.force(to, transition, now_s);
                copy_history(governor, state);
                Some(Change {
                    to,
                    transition,
                    blend_rows: self.blend_rows(state.fade_s),
                    forced: true,
                })
            }
        }
    }

    /// Une transition démarre : `from_index` est le début du segment posé dans le plan (`None` s'il n'a pas pu l'être,
    /// sans monde ni voiture : seul l'étalonnage change alors, tout de suite).
    pub fn begin(&mut self, change: &Change, from_index: Option<i64>, state: &mut DistrictState) {
        self.running = Some(Running {
            to: change.to,
            blend_rows: change.blend_rows,
            from: from_index,
            blend_end: from_index.map(|from| from.saturating_add(i64::from(change.blend_rows))),
            approach_s: 0.0,
            zone_s: None,
            held_s: 0.0,
            forced: change.forced,
        });
        state.target = Some(change.to);
        state.transition = change.transition;
        state.fallback = !change.transition.implemented();
        state.progress = 0.0;
    }

    /// Une image : mesure la vitesse, détecte la fin d'un événement, avance la transition (et l'étalonnage, quand la
    /// voiture entre dans la zone), consulte le gouverneur. Rend la transition à lancer si le gouverneur décide un
    /// changement. Sans allocation hors changement.
    ///
    /// `event_ended` compare le nombre d'événements d'une image à l'autre : une fin qui tombe sur la même image qu'un
    /// départ ne se voit pas (la fenêtre s'ouvrira à la fin suivante, ou au temps maximum).
    pub fn tick(
        &mut self,
        governor: &mut Governor,
        input: &DistrictInput,
        grade: Option<&mut DistrictGradeState>,
        state: &mut DistrictState,
    ) -> Option<Change> {
        self.measure_speed(input);
        // Un événement de moins qu'à l'image précédente : il vient de finir (une seule image).
        let event_ended = matches!((self.last_events, input.active_events), (Some(before), Some(now)) if now < before);
        self.last_events = input.active_events;

        let snapshot = GovernorSnapshot {
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
        };
        let held = self.advance(governor, input, &snapshot, grade, state);

        let decision = governor.tick(&snapshot);
        copy_history(governor, state);
        match decision {
            Decision::Stay { reason } => {
                // Couleur retenue par la musique : F4 montre pourquoi plutôt qu'un simple « transition en cours ».
                state.last_reason = held.unwrap_or(reason);
                None
            }
            Decision::Change { to, transition } => Some(Change {
                to,
                transition,
                blend_rows: self.blend_rows(state.fade_s),
                forced: false,
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

    /// Avance la transition en deux temps.
    ///
    /// - **Approche** : le segment est posé loin devant (au-delà des rangées générées) ; la couleur ne bouge pas, le
    ///   gouverneur est maintenu en `Transitioning`, `progress` reste à 0. Au plus [`APPROACH_MAX_S`] : une voiture
    ///   arrêtée ne bloque pas le moteur.
    /// - **Retenue** : la voiture est dans la zone mais la musique interdit un changement visible
    ///   ([`Governor::music_blocks`] : Peak, drop imminent, après un drop) ; la couleur attend, au plus
    ///   [`MUSIC_HOLD_MAX_S`]. Un forçage n'est jamais retenu. Rend la raison de la retenue.
    /// - **Zone** : la voiture entre dans la zone de dégradé ; l'étalonnage part (fondu de la durée de traversée, au plus
    ///   `fade_s`) et le temps passé dans le quartier se compte de là. `progress` = la plus avancée de la traversée et du
    ///   temps. Fin quand la voiture a passé la zone et que l'étalonnage est posé, ou au plus `fade_s` + [`MAX_WAIT`]
    ///   après l'entrée (`set_target` attend un temps fort, au plus [`MAX_WAIT`] s).
    fn advance(
        &mut self,
        governor: &mut Governor,
        input: &DistrictInput,
        snapshot: &GovernorSnapshot,
        mut grade: Option<&mut DistrictGradeState>,
        state: &mut DistrictState,
    ) -> Option<StayReason> {
        let running = self.running.as_mut()?;
        let dt = input.dt_s.max(0.0);
        let car_row = input.car_z.map(|z| (-z / f64::from(CHUNK_LENGTH_M)).floor() as i64);
        let fade_s = state.fade_s.clamp(MIN_FADE_S, TRANSITION_MAX_S as f32);

        let zone_s = if let Some(zone_s) = running.zone_s.as_mut() {
            *zone_s += dt;
            *zone_s
        } else {
            running.approach_s += dt;
            let reached = match (running.from, car_row) {
                (Some(from), Some(row)) => row >= from,
                // Pas de segment posé (ni monde ni voiture) : seul l'étalonnage change, tout de suite.
                _ => true,
            };
            if !reached && running.approach_s < APPROACH_MAX_S {
                governor.hold_transition(input.now_s);
                state.progress = 0.0;
                return None;
            }
            // Dans la zone : jamais de changement visible en plein drop ni en Peak (sauf forçage, sauf retenue trop
            // longue).
            let blocked = (!running.forced && running.held_s < MUSIC_HOLD_MAX_S)
                .then(|| governor.music_blocks(snapshot))
                .flatten();
            if let Some(reason) = blocked {
                running.held_s += dt;
                governor.hold_transition(input.now_s);
                state.progress = 0.0;
                return Some(reason);
            }
            // Entrée dans la zone : la couleur suit la voiture, sur le temps de traversée de la zone.
            running.zone_s = Some(0.0);
            governor.restart_dwell(input.now_s);
            let crossing_s = running.blend_rows as f32 * CHUNK_LENGTH_M / self.speed_mps.max(1e-3);
            if let Some(grade) = grade.as_deref_mut() {
                grade.set_target(running.to, crossing_s.min(fade_s));
            }
            0.0
        };

        let by_rows = match (running.from, car_row) {
            (Some(from), Some(row)) => (row - from) as f32 / running.blend_rows.max(1) as f32,
            _ => 0.0,
        };
        state.progress = by_rows.max(zone_s / fade_s).clamp(0.0, 1.0);
        let passed = match (car_row, running.blend_end) {
            (Some(row), Some(end)) => row >= end,
            _ => true,
        };
        let graded = grade
            .as_deref()
            .map_or(zone_s >= fade_s, |g| !g.is_fading() && !g.is_pending());
        if (passed && graded) || zone_s >= fade_s + MAX_WAIT {
            self.running = None;
            governor.transition_done();
            state.current = state.target.take().unwrap_or(state.current);
            state.progress = 1.0;
        } else {
            governor.hold_transition(input.now_s);
        }
        None
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
        assert_eq!(driver.blend_rows(10.0), MIN_BLEND_ROWS);
        driver.speed_mps = 60.0;
        // 60 m/s × 10 s = 600 m = 23,1 rangées de 26 m.
        assert_eq!(driver.blend_rows(10.0), 24);
        // Durée réglée à 5 s : 300 m, 11,5 rangées.
        assert_eq!(driver.blend_rows(5.0), 12);
        driver.speed_mps = 399.0;
        assert_eq!(driver.blend_rows(10.0), MAX_BLEND_ROWS);
    }
}
