//! Le quartier courant, tel que l'interface le lit, et les ordres qu'elle peut donner.
//!
//! Les deux types vivent ici, et non dans `cf-world-neon` qui les écrit et les lit : la fenêtre opérateur
//! (`cf-operator`) ne dépend que de `cf-render`, jamais d'un monde. Aucune logique : le monde (`cf-world-neon`,
//! module `districts`) est le seul écrivain de [`DistrictState`] et le seul lecteur de [`DistrictOrder`].

use bevy::prelude::*;
use cf_show::district::{DistrictId, DistrictKind, StayReason, TransitionKind};

/// Nombre de quartiers gardés dans [`DistrictState::history`].
pub const DISTRICT_HISTORY: usize = 5;

/// L'état du moteur de quartiers, copie lisible pour l'interface (panneau F4). Écrit chaque image par le monde.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct DistrictState {
    /// Quartier affiché : l'ancien tant que la transition n'est pas finie.
    pub current: DistrictId,
    /// Quartier visé par la transition en cours, `None` hors transition.
    pub target: Option<DistrictId>,
    /// Transition demandée (la dernière jouée hors transition).
    pub transition: TransitionKind,
    /// Avancement de la transition, 0..=1 (1 hors transition).
    pub progress: f32,
    /// Dernière raison donnée par le gouverneur pour ne rien changer.
    pub last_reason: StayReason,
    /// Automatique gelé (les forçages passent quand même).
    pub frozen: bool,
    /// Les derniers quartiers, du plus ancien au plus récent (cible comprise), `None` pour les places vides.
    pub history: [Option<DistrictId>; DISTRICT_HISTORY],
    /// La transition demandée n'existe pas encore : elle est jouée en dégradé (« repli : dégradé »).
    pub fallback: bool,
    /// Durée des prochaines transitions (s, 1..=10) : longueur du dégradé (distance parcourue pendant ce temps) et
    /// fondu de l'étalonnage. Réglée par [`DistrictOrder::SetFadeSeconds`].
    pub fade_s: f32,
}

impl Default for DistrictState {
    fn default() -> Self {
        let neon = DistrictId {
            kind: DistrictKind::Neon,
            variant: 0,
        };
        let mut history = [None; DISTRICT_HISTORY];
        history[0] = Some(neon);
        Self {
            current: neon,
            target: None,
            transition: TransitionKind::Gradient,
            progress: 1.0,
            last_reason: StayReason::NoShow,
            frozen: false,
            history,
            fallback: false,
            fade_s: 10.0,
        }
    }
}

/// Ordre de l'interface (ou d'un déclencheur `district:<type>`) au moteur de quartiers.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub enum DistrictOrder {
    /// Lancer tout de suite la transition vers `to` par `transition`, sans consulter les règles (outil de debug) : la
    /// ville change au bord de la fenêtre de streaming, la couleur quand la voiture y arrive. Remplace une
    /// transition en cours ; accepté pendant un drop. Le limiteur de flashs reste en place.
    Force {
        /// Quartier visé.
        to: DistrictId,
        /// Transition demandée (les non implémentées retombent sur le dégradé).
        transition: TransitionKind,
    },
    /// Gèle (`true`) ou dégèle l'automatique.
    SetFrozen(bool),
    /// Durée des prochaines transitions, en secondes, bornée à 1..=10 (curseur de F4). Une valeur non finie est ignorée.
    SetFadeSeconds(f32),
}
