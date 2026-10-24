//! Le carnet de chargement : chaque morceau de préchargement (voiture, musique, logos…) y inscrit une étape, l'écran de
//! progression de l'opérateur la lit, et le passage à `Ready` attend que toutes soient closes.
//!
//! Une étape en échec ne bloque pas le show : un asset manquant est **signalé à l'ouverture** (journal et fenêtre
//! opérateur), pas au moment du cue (`live-safety`), mais l'opérateur garde la main (look de secours, autre piste).

use bevy::prelude::*;

/// Identifiant d'une étape du carnet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StepId(usize);

/// Où en est une étape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepStatus {
    /// En cours.
    Pending,
    /// Terminée.
    Done,
    /// Échouée ; le texte dit pourquoi (affiché tel quel à l'opérateur).
    Failed(String),
}

/// Une étape de chargement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadStep {
    /// Nom lisible (« Voiture (GLB) », « Musiques »…).
    pub label: String,
    /// Avancement.
    pub status: StepStatus,
}

/// Les étapes de chargement de l'ouverture du show. Ressource écrite par les plugins qui préchargent.
#[derive(Resource, Debug, Default, Clone)]
pub struct LoadTracker {
    steps: Vec<LoadStep>,
}

impl LoadTracker {
    /// Inscrit une étape en cours.
    pub fn register(&mut self, label: impl Into<String>) -> StepId {
        self.steps.push(LoadStep {
            label: label.into(),
            status: StepStatus::Pending,
        });
        StepId(self.steps.len() - 1)
    }

    /// Inscrit une étape déjà terminée (chargée avant le démarrage de l'application : show, looks…).
    pub fn register_done(&mut self, label: impl Into<String>) {
        let id = self.register(label);
        self.finish(id);
    }

    /// Clôt une étape avec succès. Sans effet sur un identifiant inconnu.
    pub fn finish(&mut self, id: StepId) {
        if let Some(step) = self.steps.get_mut(id.0) {
            step.status = StepStatus::Done;
        }
    }

    /// Clôt une étape en échec : le show démarrera quand même, l'erreur est journalisée et montrée à l'opérateur.
    pub fn fail(&mut self, id: StepId, reason: impl Into<String>) {
        let reason = reason.into();
        if let Some(step) = self.steps.get_mut(id.0) {
            tracing::error!(step = %step.label, %reason, "préchargement : étape en échec");
            step.status = StepStatus::Failed(reason);
        }
    }

    /// Toutes les étapes, dans l'ordre d'inscription.
    #[must_use]
    pub fn steps(&self) -> &[LoadStep] {
        &self.steps
    }

    /// Vrai quand plus aucune étape n'est en cours (vrai aussi sans étape).
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.steps.iter().all(|step| step.status != StepStatus::Pending)
    }

    /// Part des étapes closes, dans `[0, 1]` (1 sans étape).
    #[must_use]
    pub fn fraction(&self) -> f32 {
        if self.steps.is_empty() {
            return 1.0;
        }
        let closed = self.steps.iter().filter(|s| s.status != StepStatus::Pending).count();
        closed as f32 / self.steps.len() as f32
    }

    /// Les étapes en échec.
    pub fn failures(&self) -> impl Iterator<Item = &LoadStep> {
        self.steps
            .iter()
            .filter(|step| matches!(step.status, StepStatus::Failed(_)))
    }

    /// Clôt en échec toutes les étapes encore en cours (délai dépassé).
    pub(super) fn fail_pending(&mut self, reason: &str) {
        for index in 0..self.steps.len() {
            if self.steps[index].status == StepStatus::Pending {
                self.fail(StepId(index), reason);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_etape_en_echec_clot_quand_meme_le_chargement() {
        let mut tracker = LoadTracker::default();
        assert!(tracker.is_complete());
        let a = tracker.register("a");
        let b = tracker.register("b");
        assert!(!tracker.is_complete());
        assert!(tracker.fraction() < f32::EPSILON);
        tracker.finish(a);
        assert!((tracker.fraction() - 0.5).abs() < f32::EPSILON);
        tracker.fail(b, "absent");
        assert!(tracker.is_complete());
        assert_eq!(tracker.failures().count(), 1);
    }

    #[test]
    fn le_delai_depasse_ferme_les_etapes_en_cours() {
        let mut tracker = LoadTracker::default();
        let a = tracker.register("a");
        tracker.register("b");
        tracker.finish(a);
        tracker.fail_pending("délai dépassé");
        assert!(tracker.is_complete());
        assert_eq!(tracker.failures().count(), 1);
    }
}
