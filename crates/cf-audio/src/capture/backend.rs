//! Le contrat d'un backend de capture : énumérer les entrées, ouvrir un flux qui remplit un [`InputFeed`].
//!
//! Deux implémentations : [`super::CpalBackend`] (le vrai matériel) et [`super::SyntheticBackend`] (signal généré,
//! branchements et débranchements simulés), pour tester toute la chaîne sans carte son.

use std::fmt;

use super::{CaptureError, DeviceSelector, InputDeviceInfo, InputFormat};
use crate::feed::InputFeed;

/// Un hôte audio capable d'ouvrir des flux d'entrée.
///
/// Le superviseur de capture possède le backend sur son propre thread : `open` et la destruction du flux y ont lieu,
/// donc le flux lui-même n'a pas besoin d'être `Send` (`cpal::Stream` ne l'est pas sur toutes les plateformes).
pub trait InputBackend: Send + 'static {
    /// Garde le flux ouvert ; le lâcher arrête la capture et ferme le côté écriture du flux.
    type Stream;

    /// Les périphériques d'entrée présents en ce moment.
    ///
    /// # Errors
    ///
    /// Erreur de l'hôte audio (hôte absent, permissions…).
    fn input_devices(&self) -> Result<Vec<InputDeviceInfo>, CaptureError>;

    /// Ouvre et démarre un flux de capture sur le périphérique choisi, avec un ring de `ring_seconds` secondes.
    ///
    /// # Errors
    ///
    /// Périphérique introuvable ou ambigu, aucun format utilisable, refus de l'hôte audio.
    fn open(&self, selector: &DeviceSelector, ring_seconds: f32) -> Result<OpenedInput<Self::Stream>, CaptureError>;
}

/// Un flux de capture ouvert.
pub struct OpenedInput<S> {
    /// Garde du flux (le lâcher l'arrête).
    pub stream: S,
    /// Côté lecture, à confier au thread d'analyse.
    pub feed: InputFeed,
    /// Nom du périphérique effectivement ouvert.
    pub device: String,
    /// Format effectivement ouvert.
    pub format: InputFormat,
}

impl<S> fmt::Debug for OpenedInput<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenedInput")
            .field("feed", &self.feed)
            .field("device", &self.device)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

/// Destination des flux ouverts : en pratique, le thread d'analyse (`cf-analysis`), qui range chaque flux dans
/// l'emplacement de sa source. Une fermeture `FnMut(InputFeed)` convient aussi.
pub trait FeedSink: Send + 'static {
    /// Confie un flux fraîchement ouvert (première ouverture ou reconnexion).
    fn attach(&mut self, feed: InputFeed);
}

impl<F: FnMut(InputFeed) + Send + 'static> FeedSink for F {
    fn attach(&mut self, feed: InputFeed) {
        self(feed);
    }
}
