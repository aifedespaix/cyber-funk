//! Ce que le pont audio publie côté Bevy : l'état d'analyse, les messages datés, les commandes de l'opérateur et le
//! décalage visuel.

use std::ops::Deref;
use std::sync::Arc;

use bevy::prelude::*;
use cf_analysis::{AnalysisStatsSnapshot, Section};
use cf_audio::capture::{CaptureState, DeviceSelector};
use cf_audio::player::PlayerState;
use cf_audio::playlist::Track;

use super::latency::MAX_LATENCY_MS;

/// `AudioFeatures` de `cf-analysis`, publié comme ressource Bevy (le type de `cf-analysis` ne peut pas l'être : la
/// crate ne dépend pas de Bevy). Se déréférence vers lui : `features.bass`, `features.section`…
///
/// Écrite par **un seul** système (le pont, en `CfSet::Input`). C'est l'état **déjà décalé** du retard visuel, de
/// sorte que tout ce que l'image montre reste d'accord avec les messages [`Kick`] et [`SectionChanged`].
///
/// `kick` et `section_changed` valent vrai pour l'image où le message correspondant est émis, pas pour un « pas
/// d'analyse » de 10,7 ms qu'une image à 60 i/s peut manquer : ils ne sont donc jamais perdus.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct AudioFeatures(pub cf_analysis::AudioFeatures);

impl Deref for AudioFeatures {
    type Target = cf_analysis::AudioFeatures;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Un kick, au moment où il doit se voir (décalage visuel appliqué).
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct Kick {
    /// Temps audio de l'attaque (s), l'horloge de `AudioFeatures::time_s`.
    pub audio_time_s: f64,
    /// Pas d'analyse pendant lequel le kick a été détecté.
    pub step: u64,
}

/// La section du morceau vient de changer, au moment où cela doit se voir.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionChanged {
    /// Nouvelle section.
    pub section: Section,
}

/// Décalage visuel du projecteur, ressource réglable à chaud (le panneau opérateur l'écrit).
///
/// Il retarde ce que l'image montre pour le recaler sur un son qui arrive plus tard (retard de sonorisation, de
/// traitement). Borné à `0..=500` ms : on ne peut pas montrer un kick avant de l'avoir détecté.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct VisualLatency {
    ms: f32,
}

impl VisualLatency {
    /// Décalage de `ms` millisecondes, borné à `0..=MAX_LATENCY_MS` (une valeur non finie donne 0).
    #[must_use]
    pub fn new(ms: f32) -> Self {
        Self { ms: sanitize_ms(ms) }
    }

    /// Millisecondes.
    #[must_use]
    pub fn ms(self) -> f32 {
        self.ms
    }

    /// Change le décalage (borné comme [`VisualLatency::new`]).
    pub fn set_ms(&mut self, ms: f32) {
        self.ms = sanitize_ms(ms);
    }

    /// Secondes.
    #[must_use]
    pub fn seconds(self) -> f64 {
        f64::from(self.ms) / 1000.0
    }
}

impl Default for VisualLatency {
    fn default() -> Self {
        Self::new(0.0)
    }
}

fn sanitize_ms(ms: f32) -> f32 {
    if ms.is_finite() { ms.clamp(0.0, MAX_LATENCY_MS) } else { 0.0 }
}

/// Source audio choisie par l'opérateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AudioSource {
    /// Micro : capture avec gain automatique (le micro d'un portable capte la musique à -30 dB ou moins).
    Microphone,
    /// Entrée ligne d'une carte son (table de mixage) : capture sans gain automatique, le niveau est celui d'un fichier.
    Line,
    /// Lecteur des pistes de démo.
    #[default]
    Demo,
}

impl AudioSource {
    /// Les trois choix, dans l'ordre d'affichage.
    pub const ALL: [Self; 3] = [Self::Microphone, Self::Line, Self::Demo];

    /// Nom court pour l'interface et les journaux.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Microphone => "Micro",
            Self::Line => "Ligne",
            Self::Demo => "Démo",
        }
    }

    /// Source de capture (micro ou ligne), par opposition au lecteur de démo.
    #[must_use]
    pub const fn is_capture(self) -> bool {
        !matches!(self, Self::Demo)
    }
}

/// Ordres de l'opérateur au pont audio. Lus en `CfSet::Input` ; aucun ne bloque l'image.
#[derive(Message, Debug, Clone, PartialEq)]
pub enum AudioCommand {
    /// Écouter une autre source, à chaud.
    SelectSource(AudioSource),
    /// Périphérique d'entrée du micro ou de la ligne (les deux peuvent être des cartes différentes).
    SelectDevice {
        /// `Microphone` ou `Line` (ignoré pour `Demo`).
        source: AudioSource,
        /// Périphérique à ouvrir.
        device: DeviceSelector,
    },
    /// Relire la liste des périphériques d'entrée.
    RefreshDevices,
    /// Lecture du lecteur de démo.
    DemoPlay,
    /// Pause du lecteur de démo.
    DemoPause,
    /// Bascule lecture / pause.
    DemoToggle,
    /// Piste suivante (selon le mode de lecture).
    DemoNext,
    /// Piste précédente (ou début de la piste après 3 s).
    DemoPrev,
    /// Jouer la piste d'indice donné.
    DemoSelect(usize),
    /// Volume du lecteur, 0..1 (n'affecte pas l'analyse).
    DemoVolume(f32),
}

/// Instantané de l'état du pont pour le panneau opérateur, rafraîchi une dizaine de fois par seconde (il copie des
/// chaînes : pas question de le refaire à chaque image).
#[derive(Resource, Debug, Clone, Default)]
pub struct AudioStatus {
    /// Source choisie.
    pub source: AudioSource,
    /// Le thread d'analyse tourne.
    pub analysis_running: bool,
    /// État de la capture ; `None` tant qu'aucune source de capture n'a été demandée (le micro n'est ouvert qu'à la
    /// première demande) ou sans entrée audio.
    pub capture: Option<CaptureState>,
    /// État du lecteur de démo ; `None` s'il n'a pas pu démarrer (voir `notes`).
    pub demo: Option<PlayerState>,
    /// Pistes de démo.
    pub tracks: Arc<[Track]>,
    /// Compteurs de l'analyse (latence callback → publication, pertes).
    pub analysis: AnalysisStatsSnapshot,
    /// Périphériques d'entrée connus à la dernière lecture de la liste.
    pub devices: Vec<String>,
    /// Périphérique du micro.
    pub mic_device: DeviceSelector,
    /// Périphérique de la ligne.
    pub line_device: DeviceSelector,
    /// Problèmes de démarrage et avertissements, à lire par l'opérateur (pas de carte son, aucune piste…).
    pub notes: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_decalage_est_borne() {
        assert!((VisualLatency::new(-5.0).ms()).abs() < f32::EPSILON);
        assert!((VisualLatency::new(9_999.0).ms() - MAX_LATENCY_MS).abs() < f32::EPSILON);
        assert!((VisualLatency::new(f32::NAN).ms()).abs() < f32::EPSILON);
        assert!((VisualLatency::new(40.0).seconds() - 0.04).abs() < 1e-9);
    }

    #[test]
    fn la_ressource_se_deref_vers_les_features() {
        let features = AudioFeatures(cf_analysis::AudioFeatures {
            bass: 0.5,
            ..cf_analysis::AudioFeatures::NEUTRAL
        });
        assert!((features.bass - 0.5).abs() < f32::EPSILON);
    }
}
