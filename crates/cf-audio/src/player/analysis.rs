//! Ce que le rendu temps réel écrit hors de la sortie : le flux d'analyse et le compteur d'erreurs du périphérique.
//! Mêmes règles que [`super::render`] : ni allocation, ni verrou, ni journal.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::shared::Shared;
use crate::feed::FeedWriter;

/// Trames regroupées avant chaque envoi vers le flux d'analyse : un envoi par bloc plutôt qu'un par trame.
pub(super) const ANALYSIS_CHUNK_FRAMES: usize = 512;

/// Envoi vers le flux d'analyse (le [`FeedWriter`] de J1-03, source `Demo`). Champ à part du [`Renderer`] pour
/// pouvoir l'utiliser pendant qu'un bloc du ring audio est emprunté.
pub(super) struct AnalysisWriter {
    pub writer: FeedWriter,
    /// Copie, pour la durée d'un appel, de [`Shared::tap_enabled`].
    pub enabled: bool,
    /// Trames stéréo, après fondu et avant volume, en attente d'envoi ; alloué une fois à la construction.
    pub pending: Box<[f32]>,
    pub len: usize,
}

impl std::fmt::Debug for AnalysisWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnalysisWriter")
            .field("enabled", &self.enabled)
            .finish_non_exhaustive()
    }
}

impl AnalysisWriter {
    /// Ajoute une trame stéréo sans volume ; envoie le paquet quand il est plein.
    #[inline]
    pub fn push(&mut self, left: f32, right: f32) {
        if !self.enabled {
            return;
        }
        self.pending[self.len] = left;
        self.pending[self.len + 1] = right;
        self.len += 2;
        if self.len == self.pending.len() {
            self.flush();
        }
    }

    /// Envoie les trames en attente : conversion mono et copie dans le ring d'analyse, sans allocation ; ring plein =
    /// échantillons jetés et comptés par le flux ([`crate::feed::FeedStats::dropped`]).
    pub fn flush(&mut self) {
        if self.len > 0 {
            self.writer.push_interleaved(&self.pending[..self.len], 2);
            self.len = 0;
        }
    }
}

/// Permet au pilote de sortie de signaler une erreur du périphérique (xrun, perte…) depuis son propre callback
/// d'erreur, sans toucher au [`Renderer`]. Simple incrément atomique.
#[derive(Debug, Clone)]
pub struct DeviceErrorReporter(pub(super) Arc<Shared>);

impl DeviceErrorReporter {
    /// Compte une erreur du périphérique de sortie.
    pub fn report(&self) {
        self.0.device_errors.fetch_add(1, Ordering::Relaxed);
    }
}
