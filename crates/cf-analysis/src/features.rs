//! `AudioFeatures` : ce que l'analyse publie à chaque pas, lu par tout le reste de l'application.
//!
//! Contrat du skill `realtime-audio` : reprend `BeatState` (`beat.ts`) et `SectionTracker` (`sections.ts`) de
//! Sillage, plus l'arbitrage du tempo et l'horloge audio. J1-03 pose la structure ; les champs musicaux restent à leur
//! valeur neutre jusqu'à leur calcul (kicks et tempo : J1-04 ; sections, intensité : J1-05 ; sources de tempo
//! externes, phase, mesure, phrase : J3-03).

/// Section du morceau (`Section` de `sections.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Section {
    /// Intro, break : énergie basse.
    Calm,
    /// Corps du morceau. Valeur de départ dans Sillage.
    #[default]
    Groove,
    /// Refrain, drop : énergie au sommet.
    Peak,
}

/// D'où vient le tempo publié (arbitrage, voir `docs/ARCHITECTURE.md` §1.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TempoSource {
    /// Horloge MIDI (24 impulsions par noire).
    Clock,
    /// Ableton Link.
    Link,
    /// Détection des kicks sur l'entrée audio (portage de `beat.ts`). Valeur par défaut, avec une confiance nulle
    /// tant qu'aucun tempo n'est détecté.
    #[default]
    Audio,
    /// Tap tempo de l'opérateur.
    Tap,
}

/// État de l'analyse audio à la fin d'un pas, publié par triple buffer (le lecteur prend toujours le plus récent).
///
/// `Copy` et sans pointeur : la publication est une simple copie, sans allocation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioFeatures {
    /// Nombre de pas analysés depuis le démarrage du thread d'analyse, celui-ci compris. Continu à travers les
    /// changements de source.
    pub step: u64,
    /// Temps audio (s) à la fin de ce pas : `step × 512 / 48 000`. Compté en échantillons analysés, jamais lu sur
    /// l'horloge murale ; il n'avance pas quand aucune source ne livre.
    pub time_s: f64,
    /// Une source livre du signal (`live` de Sillage : « musique ou micro actif »). Faux pendant une perte de
    /// périphérique, une pause du lecteur, ou sans source choisie : le monde ralentit alors en douceur.
    pub live: bool,
    /// Vrai pendant un seul pas, au moment d'un kick.
    pub kick: bool,
    /// Énergie des basses, lissée, 0..1.
    pub bass: f32,
    /// Énergie des médiums (voix, synthés), lissée, 0..1.
    pub mid: f32,
    /// Énergie des aigus (charleston, brillance), lissée, 0..1.
    pub high: f32,
    /// Volume global pondéré (basses 50 %, médiums 35 %, aigus 15 %), 0..1.
    pub loud: f32,
    /// Impulsion décroissante après chaque kick, 1 → 0.
    pub pulse: f32,
    /// Nombre de kicks depuis le dernier `reset`.
    pub count: u32,
    /// Compteur continu : `count` plus une montée douce 0 → 1 pendant la décroissance de `pulse`.
    pub beat: f64,
    /// Tempo détecté (battements par minute), `None` tant qu'il n'est pas établi.
    pub bpm: Option<f32>,
    /// Section courante du morceau.
    pub section: Section,
    /// Énergie lissée du morceau, 0 (calme) → 1 (sommet).
    pub intensity: f32,
    /// Source du tempo publié.
    pub tempo_source: TempoSource,
    /// Confiance dans le tempo, 0..1.
    pub confidence: f32,
    /// Position dans la mesure, 0..1.
    pub phase: f32,
    /// Numéro de mesure.
    pub bar: u32,
    /// Numéro de phrase (groupe de mesures).
    pub phrase: u32,
}

impl AudioFeatures {
    /// État neutre : aucun signal, énergies nulles, section et intensité de départ de Sillage (`groove`, 0,4).
    pub const NEUTRAL: Self = Self {
        step: 0,
        time_s: 0.0,
        live: false,
        kick: false,
        bass: 0.0,
        mid: 0.0,
        high: 0.0,
        loud: 0.0,
        pulse: 0.0,
        count: 0,
        beat: 0.0,
        bpm: None,
        section: Section::Groove,
        intensity: 0.4,
        tempo_source: TempoSource::Audio,
        confidence: 0.0,
        phase: 0.0,
        bar: 0,
        phrase: 0,
    };

    /// Met les énergies instantanées au repos (signal interrompu) sans toucher au tempo ni à la section : ce sont
    /// des mémoires que l'analyseur décide lui-même d'oublier.
    pub fn rest_energy(&mut self) {
        self.kick = false;
        self.bass = 0.0;
        self.mid = 0.0;
        self.high = 0.0;
        self.loud = 0.0;
        self.pulse = 0.0;
    }
}

impl Default for AudioFeatures {
    fn default() -> Self {
        Self::NEUTRAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_matches_sillage_initial_state() {
        let f = AudioFeatures::default();
        assert!(!f.live && !f.kick);
        assert_eq!(f.count, 0);
        assert_eq!(f.bpm, None);
        assert_eq!(f.section, Section::Groove);
        assert!((f.intensity - 0.4).abs() < f32::EPSILON);
        assert_eq!(f.tempo_source, TempoSource::Audio);
    }

    #[test]
    fn rest_energy_keeps_tempo_memory() {
        let mut f = AudioFeatures {
            kick: true,
            bass: 0.8,
            mid: 0.5,
            high: 0.3,
            loud: 0.6,
            pulse: 1.0,
            bpm: Some(128.0),
            section: Section::Peak,
            ..AudioFeatures::NEUTRAL
        };
        f.rest_energy();
        assert!(!f.kick);
        assert!(f.bass.abs() < f32::EPSILON && f.pulse.abs() < f32::EPSILON);
        assert_eq!(f.bpm, Some(128.0));
        assert_eq!(f.section, Section::Peak);
    }
}
