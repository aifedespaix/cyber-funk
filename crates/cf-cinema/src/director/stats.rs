//! Le bilan du réalisateur.

use crate::editor::PickMode;
use crate::types::TransitionKind;

/// Ce que le réalisateur a fait depuis son démarrage : de quoi vérifier le rythme et la sûreté (tests, rapport).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DirectorStats {
    /// Plans lancés (coupes et transitions confondues).
    pub shots: u32,
    /// Instant (horloge du monde) du dernier plan lancé.
    pub last_shot_at: f64,
    /// Mode du dernier tirage.
    pub last_mode: Option<PickMode>,
    /// Entrée du dernier plan.
    pub last_kind: Option<TransitionKind>,
    /// Le dernier plan est parti sur un temps musical.
    pub last_on_beat: bool,
    /// Temps comptés depuis la phrase au moment du dernier plan.
    pub last_beat_no: u32,
    /// Images où la pose touchait un immeuble et a été dégagée.
    pub wall_saves: u32,
    /// Images où la pose a été remplacée par le dernier bon cadrage (NaN, mur indégageable).
    pub fallbacks: u32,
    /// Coupes franches admises (transitions `Cut` et sauts de pose des plans) : plafonnées à
    /// [`crate::choreo::MAX_CUTS_PER_SECOND`] par seconde glissante.
    pub cuts: u32,
    /// Coupes refusées par le plafond : transition adoucie en fondu, ou saut de plan retardé (pose précédente tenue).
    pub cuts_held: u32,
}
