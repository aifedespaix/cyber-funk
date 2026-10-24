//! Repères musicaux (`cinema/cues.ts`) : temps, mesure, phrase, drop, section, montée, break.

use crate::types::{Cue, CueKind, Section};
use crate::world::CinemaWorld;

/// Sans kick depuis ce délai (s), on fabrique des temps : micro faible, morceau sans basse, début de piste.
const SILENCE_BEFORE_SYNTH: f64 = 2.0;
const SYNTH_BEAT: f64 = 0.8;
/// Une mesure = 4 temps, une phrase = 16 temps, comme avec de vrais kicks.
pub const BAR_BEATS: u32 = 4;
/// Voir [`BAR_BEATS`].
pub const PHRASE_BEATS: u32 = 16;
/// Montée : l'énergie dépasse sa moyenne lente d'au moins ce seuil, depuis ce nombre de secondes.
const BUILD_GAP: f32 = 0.1;
const BUILD_SECONDS: f32 = 3.0;
const BUILD_REARM: f32 = 25.0;
const REFERENCE_SECONDS: f32 = 6.0;
/// Break : l'énergie passe sous ce seuil (ou chute franchement).
const BREAK_LEVEL: f32 = 0.3;
const BREAK_DROP: f32 = 0.18;
const BREAK_ARMED_ABOVE: f32 = 0.5;

/// Au plus un repère de chaque genre par image (sept genres).
pub const MAX_CUES: usize = 7;

/// Les repères d'une image, dans un tableau de taille fixe (aucune allocation).
#[derive(Debug, Clone, Copy)]
pub struct CueList {
    items: [Cue; MAX_CUES],
    len: usize,
}

impl Default for CueList {
    fn default() -> Self {
        Self {
            items: [Cue {
                kind: CueKind::Beat,
                section: Section::Groove,
                intensity: 0.0,
            }; MAX_CUES],
            len: 0,
        }
    }
}

impl CueList {
    /// Les repères de l'image.
    #[must_use]
    pub fn as_slice(&self) -> &[Cue] {
        &self.items[..self.len]
    }

    /// Un repère de ce genre est-il présent ?
    #[must_use]
    pub fn has(&self, kind: CueKind) -> bool {
        self.as_slice().iter().any(|c| c.kind == kind)
    }

    fn clear(&mut self) {
        self.len = 0;
    }

    fn push(&mut self, kind: CueKind, world: &CinemaWorld) {
        if let Some(slot) = self.items.get_mut(self.len) {
            *slot = Cue {
                kind,
                section: world.section,
                intensity: world.intensity,
            };
            self.len += 1;
        }
    }
}

/// Traduit l'état du monde en événements musicaux, une fois par image. Build et break ne sortent qu'une fois par
/// épisode.
#[derive(Debug, Clone)]
pub struct CueTracker {
    cues: CueList,
    count: Option<u32>,
    last_kick: f64,
    synth_at: f64,
    synth_count: u32,
    reference: f32,
    rising: f32,
    since_build: f32,
    build_armed: bool,
    break_armed: bool,
}

impl Default for CueTracker {
    fn default() -> Self {
        Self {
            cues: CueList::default(),
            count: None,
            last_kick: f64::NEG_INFINITY,
            synth_at: 0.0,
            synth_count: 0,
            reference: 0.4,
            rising: 0.0,
            since_build: BUILD_REARM,
            build_armed: true,
            break_armed: false,
        }
    }
}

impl CueTracker {
    /// Repart de zéro (démarrage du réalisateur).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Les repères de cette image.
    pub fn update(&mut self, world: &CinemaWorld) -> CueList {
        self.cues.clear();
        if !world.live {
            return self.cues;
        }
        let count = world.beat.count;
        let previous = *self.count.get_or_insert(count);
        if count != previous {
            self.count = Some(count);
            self.last_kick = world.time;
            self.synth_at = 0.0;
            self.cues.push(CueKind::Beat, world);
            if count / BAR_BEATS > previous / BAR_BEATS {
                self.cues.push(CueKind::Bar, world);
            }
            if count / PHRASE_BEATS > previous / PHRASE_BEATS {
                self.cues.push(CueKind::Phrase, world);
            }
        } else if world.time - self.last_kick > SILENCE_BEFORE_SYNTH {
            self.synthesize(world);
        }

        if world.drop {
            self.cues.push(CueKind::Drop, world);
        }
        if world.section_changed.is_some() {
            self.cues.push(CueKind::Section, world);
        }
        self.watch_energy(world);
        self.cues
    }

    /// Temps fabriqués sur l'horloge : le réalisateur garde un rythme de coupe même sans kick fiable.
    fn synthesize(&mut self, world: &CinemaWorld) {
        if self.synth_at == 0.0 {
            self.synth_at = world.time + SYNTH_BEAT;
            return;
        }
        if world.time < self.synth_at {
            return;
        }
        self.synth_at += SYNTH_BEAT;
        if self.synth_at < world.time {
            self.synth_at = world.time + SYNTH_BEAT;
        }
        self.synth_count = self.synth_count.wrapping_add(1);
        self.cues.push(CueKind::Beat, world);
        if self.synth_count.is_multiple_of(BAR_BEATS) {
            self.cues.push(CueKind::Bar, world);
        }
        if self.synth_count.is_multiple_of(PHRASE_BEATS) {
            self.cues.push(CueKind::Phrase, world);
        }
    }

    fn watch_energy(&mut self, world: &CinemaWorld) {
        let level = world.intensity;
        let dt = world.dt;
        self.reference += (level - self.reference) * (1.0 - (-dt / REFERENCE_SECONDS).exp());
        let gap = level - self.reference;
        self.since_build += dt;

        if self.build_armed && gap > BUILD_GAP && level < 0.9 {
            self.rising += dt;
        } else {
            self.rising = (self.rising - dt * 2.0).max(0.0);
        }
        if self.build_armed && self.rising > BUILD_SECONDS {
            self.cues.push(CueKind::Build, world);
            self.build_armed = false;
            self.rising = 0.0;
            self.since_build = 0.0;
        } else if !self.build_armed && self.since_build > BUILD_REARM && gap < 0.03 {
            self.build_armed = true;
        }

        if level > BREAK_ARMED_ABOVE {
            self.break_armed = true;
        }
        if self.break_armed && (level < BREAK_LEVEL || (self.reference - level > BREAK_DROP && level < 0.5)) {
            self.cues.push(CueKind::Break, world);
            self.break_armed = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(time: f64, count: u32) -> CinemaWorld {
        CinemaWorld {
            time,
            dt: 0.02,
            live: true,
            beat: crate::types::BeatView {
                count,
                ..Default::default()
            },
            ..CinemaWorld::default()
        }
    }

    #[test]
    fn temps_mesures_et_phrases() {
        let mut tracker = CueTracker::default();
        assert!(tracker.update(&world(0.0, 15)).as_slice().is_empty());
        let cues = tracker.update(&world(0.5, 16));
        assert!(cues.has(CueKind::Beat) && cues.has(CueKind::Bar) && cues.has(CueKind::Phrase));
        let cues = tracker.update(&world(1.0, 17));
        assert!(cues.has(CueKind::Beat) && !cues.has(CueKind::Bar));
        let cues = tracker.update(&world(1.5, 20));
        assert!(cues.has(CueKind::Bar) && !cues.has(CueKind::Phrase));
    }

    #[test]
    fn sans_kick_on_fabrique_des_temps() {
        let mut tracker = CueTracker::default();
        tracker.update(&world(0.0, 3));
        tracker.update(&world(0.1, 4));
        let mut beats = 0;
        let mut t = 0.1;
        while t < 12.0 {
            t += 0.02;
            if tracker.update(&world(t, 4)).has(CueKind::Beat) {
                beats += 1;
            }
        }
        // Silence à partir de 2,1 s, premier temps fabriqué 0,8 s plus tard, puis un toutes les 0,8 s.
        assert!((10..=13).contains(&beats), "{beats}");
    }

    #[test]
    fn hors_musique_aucun_repere() {
        let mut tracker = CueTracker::default();
        let mut w = world(0.0, 1);
        w.live = false;
        w.drop = true;
        assert!(tracker.update(&w).as_slice().is_empty());
    }

    #[test]
    fn montee_puis_break_une_fois_chacun() {
        let mut tracker = CueTracker::default();
        let (mut builds, mut breaks) = (0, 0);
        let mut t = 0.0;
        for i in 0..4000 {
            t += 0.02;
            let mut w = world(t, 0);
            // 0,4 stable, montée vers 0,85 de 10 à 20 s, chute à 0,2 à 50 s.
            w.intensity = if i < 500 {
                0.4
            } else if i < 1000 {
                0.4 + 0.45 * (i - 500) as f32 / 500.0
            } else if i < 2500 {
                0.85
            } else {
                0.2
            };
            let cues = tracker.update(&w);
            builds += usize::from(cues.has(CueKind::Build));
            breaks += usize::from(cues.has(CueKind::Break));
        }
        assert_eq!((builds, breaks), (1, 1));
    }
}
