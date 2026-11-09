//! Le quartier courant, tel que l'interface le lit, et les ordres qu'elle peut donner.
//!
//! Les deux types vivent ici, et non dans `cf-world-neon` qui les écrit et les lit : la fenêtre opérateur
//! (`cf-operator`) ne dépend que de `cf-render`, jamais d'un monde. Aucune logique : le monde (`cf-world-neon`,
//! module `districts`) est le seul écrivain de [`DistrictState`] et le seul lecteur de [`DistrictOrder`].

use bevy::prelude::*;
use cf_show::district::{DistrictId, DistrictKind, StayReason, TransitionKind};

/// Nombre de quartiers gardés dans [`DistrictState::history`].
pub const DISTRICT_HISTORY: usize = 5;

/// Où en est la transition en cours (ce que l'opérateur doit comprendre quand « rien ne change »).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DistrictPhase {
    /// Pas de transition : le quartier affiché est le quartier réel.
    #[default]
    Idle,
    /// Le nouveau quartier est posé loin devant ; la voiture doit encore y rouler (la couleur ne bouge pas).
    Approach,
    /// La voiture est arrivée mais la musique retient le changement (pic, drop).
    Held,
    /// La voiture traverse le dégradé : la ville et la couleur changent maintenant.
    Crossing,
}

impl DistrictPhase {
    /// Libellé court, pour l'interface.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Idle => "stable",
            Self::Approach => "approche",
            Self::Held => "retenu par la musique",
            Self::Crossing => "traversée",
        }
    }
}

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
    /// Où en est la transition en cours.
    pub phase: DistrictPhase,
    /// Distance (m) que la voiture doit encore rouler avant d'atteindre le nouveau quartier ; 0 hors approche.
    pub approach_m: f32,
    /// Vitesse mesurée de la voiture (m/s) : à 0, l'approche n'avance pas.
    pub car_speed_mps: f32,
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
            phase: DistrictPhase::Idle,
            approach_m: 0.0,
            car_speed_mps: 0.0,
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
