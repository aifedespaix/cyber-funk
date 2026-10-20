//! Musique synthétique : un tempo et un morceau découpé en parties (intro, montée, drop, break), répété sans fin.
//!
//! Elle ne produit pas de son, seulement les [`AudioFeatures`] que l'analyse donnerait : kicks réguliers, compte,
//! section, intensité qui monte pendant une montée (de quoi faire réagir `BuildDetector`) et un drop réel sur le
//! premier temps de chaque partie « drop ».

use cf_analysis::{AudioFeatures, Section, TempoSource};

/// Nature d'une partie du morceau.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartKind {
    /// Introduction ou fin : énergie basse.
    Calm,
    /// Montée : l'intensité grimpe, le drop approche.
    Build,
    /// Le drop et son corps : énergie au sommet. Un drop réel frappe sur son premier kick.
    Drop,
    /// Break : énergie basse, sans drop.
    Break,
}

/// Une partie du morceau.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    /// Nature.
    pub kind: PartKind,
    /// Durée en mesures (4 temps).
    pub bars: u32,
}

/// Intensité d'un calme ou d'un break, et de départ d'une montée.
const CALM_INTENSITY: f32 = 0.25;
/// Intensité de fin de montée (sous le plafond de `BuildDetector`, 0,9) et du corps d'un drop.
const PEAK_INTENSITY: f32 = 0.85;
/// Décroissance (s) de l'impulsion après un kick.
const PULSE_DECAY_S: f64 = 0.12;

/// Ce que la musique donne pour une image.
#[derive(Debug, Clone, Copy)]
pub struct MusicFrame {
    /// Descripteurs de l'analyse.
    pub audio: AudioFeatures,
    /// Le drop réel de la musique, une seule image.
    pub real_drop: bool,
}

/// Le morceau en cours : tempo, structure, position.
#[derive(Debug, Clone)]
pub struct SyntheticMusic {
    parts: Vec<Part>,
    bpm: f32,
    live: bool,
    /// Kicks depuis le début du morceau en cours (le compte de l'analyse, lui, ne retombe jamais).
    track_kicks: u32,
    since_kick_s: f64,
    audio: AudioFeatures,
    manual_drop: bool,
}

impl SyntheticMusic {
    /// Morceau de club : intro 16 mesures, montée 8, drop 32, break 16, montée 8, drop 32, fin 8 (soit 120 mesures,
    /// environ 3 min 50 à 125 BPM), répété. `start_live` : une source livre du signal dès la première image.
    #[must_use]
    pub fn club(bpm: f32, start_live: bool) -> Self {
        use PartKind::{Break, Build, Calm, Drop};
        let parts = [
            (Calm, 16),
            (Build, 8),
            (Drop, 32),
            (Break, 16),
            (Build, 8),
            (Drop, 32),
            (Calm, 8),
        ]
        .map(|(kind, bars)| Part { kind, bars })
        .to_vec();
        Self::new(parts, bpm, start_live)
    }

    /// Morceau de structure libre (`parts` vide : un seul calme de 16 mesures, pour ne jamais diviser par zéro).
    #[must_use]
    pub fn new(mut parts: Vec<Part>, bpm: f32, start_live: bool) -> Self {
        parts.retain(|p| p.bars > 0);
        if parts.is_empty() {
            parts.push(Part {
                kind: PartKind::Calm,
                bars: 16,
            });
        }
        let mut music = Self {
            parts,
            bpm: bpm.clamp(40.0, 240.0),
            live: start_live,
            track_kicks: 0,
            since_kick_s: 0.0,
            audio: AudioFeatures {
                intensity: CALM_INTENSITY,
                tempo_source: TempoSource::Audio,
                ..AudioFeatures::NEUTRAL
            },
            manual_drop: false,
        };
        music.audio.section = Section::Calm;
        music
    }

    /// Change le tempo (BPM) sans toucher à la structure.
    pub fn set_bpm(&mut self, bpm: f32) {
        self.bpm = bpm.clamp(40.0, 240.0);
    }

    /// Une source livre du signal, ou non (perte du micro, pause).
    pub fn set_live(&mut self, live: bool) {
        self.live = live;
    }

    /// Nouveau morceau : la structure repart du début (le compte des kicks de l'analyse, lui, continue).
    pub fn new_track(&mut self) {
        self.track_kicks = 0;
    }

    /// Un drop réel sur la prochaine image (le DJ frappe hors de la structure prévue).
    pub fn strike(&mut self) {
        self.manual_drop = true;
    }

    /// Durée d'une partie en kicks.
    fn part_kicks(part: Part) -> u32 {
        part.bars * 4
    }

    /// La partie en cours et la position dedans, en kicks.
    fn locate(&self) -> (Part, u32) {
        let cycle: u32 = self.parts.iter().map(|p| Self::part_kicks(*p)).sum();
        let mut left = self.track_kicks % cycle.max(1);
        for part in &self.parts {
            let len = Self::part_kicks(*part);
            if left < len {
                return (*part, left);
            }
            left -= len;
        }
        (self.parts[0], 0)
    }

    /// Avance d'une image de `dt` secondes.
    pub fn frame(&mut self, dt: f32) -> MusicFrame {
        let dt_s = f64::from(dt.max(0.0));
        let mut real_drop = std::mem::take(&mut self.manual_drop);
        self.audio.step += 1;
        self.audio.kick = false;
        self.audio.section_changed = false;
        self.audio.live = self.live;
        if !self.live {
            // Pas de signal : pas de kick, pas de tempo, le temps audio n'avance pas, l'énergie retombe.
            let a = &mut self.audio;
            a.bpm = None;
            a.pulse = 0.0;
            a.bass = 0.0;
            a.loud = 0.0;
            a.confidence = 0.0;
            a.intensity += (CALM_INTENSITY - a.intensity) * (dt.min(1.0) * 0.5);
            return MusicFrame { audio: *a, real_drop };
        }
        let period = 60.0 / f64::from(self.bpm);
        self.audio.time_s += dt_s;
        self.since_kick_s += dt_s;
        if self.since_kick_s >= period {
            self.since_kick_s -= period;
            self.audio.count += 1;
            self.audio.kick = true;
            self.track_kicks += 1;
        }
        let (part, at) = self.locate();
        // Le drop frappe sur le premier kick de sa partie.
        real_drop |= self.audio.kick && at == 0 && part.kind == PartKind::Drop;
        let progress = at as f32 / Self::part_kicks(part) as f32;
        let (section, intensity) = match part.kind {
            PartKind::Calm | PartKind::Break => (Section::Calm, CALM_INTENSITY),
            PartKind::Build => (
                Section::Groove,
                CALM_INTENSITY + (PEAK_INTENSITY - CALM_INTENSITY) * progress,
            ),
            PartKind::Drop => (Section::Peak, PEAK_INTENSITY),
        };
        let a = &mut self.audio;
        if section != a.section {
            a.section = section;
            a.section_changed = true;
        }
        a.intensity = intensity;
        a.bpm = Some(self.bpm);
        a.confidence = 0.9;
        a.beat = f64::from(a.count) + self.since_kick_s / period;
        a.pulse = (-self.since_kick_s / PULSE_DECAY_S).exp() as f32;
        a.bass = a.pulse * intensity;
        a.mid = intensity * 0.6;
        a.high = intensity * 0.4;
        a.loud = (0.5 * a.bass + 0.35 * a.mid + 0.15 * a.high).clamp(0.0, 1.0);
        a.bar = a.count / 4;
        a.phrase = a.count / 16;
        a.phase = (f64::from(a.count % 4) / 4.0 + self.since_kick_s / period / 4.0) as f32;
        MusicFrame { audio: *a, real_drop }
    }
}
