//! Le catalogue des cascades (`StuntKind`, `StuntTier`, `StuntAlign` de `stunts/types.ts`) et la validation des
//! identifiants que `cf-show` ne connaît que comme des chaînes (`cf_show::model::StuntKind`).

use cf_show::model::{ShowGates, StuntKind as ShowStuntKind, StuntTier as ShowStuntTier};

/// Nombre de cascades du catalogue.
pub const STUNT_COUNT: usize = 12;

/// Une cascade de Sillage. Les slugs ([`StuntKind::as_str`]) sont ceux des fichiers de look (`gates.stunt_kinds`) et du
/// drop préparé (`DropPlan::stunt`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StuntKind {
    /// Hors route, sur le bas-côté.
    OffRoad,
    /// Slalom entre les véhicules.
    Slalom,
    /// Frôlement d'un véhicule.
    NearMiss,
    /// Looping.
    Loop,
    /// Face à face.
    HeadOn,
    /// Vol sur le dos.
    Inverted,
    /// Roulage sur une façade.
    WallRide,
    /// Passage dans une structure.
    Threading,
    /// Saut d'enseigne en enseigne.
    SignHop,
    /// Sur les toits.
    Rooftop,
    /// Grand saut.
    BigJump,
    /// L'Ascension du niveau 5 (mode spectacle) : le final qui monte au-dessus de la ville.
    Ascension,
}

impl StuntKind {
    /// Toutes les cascades, dans l'ordre du répertoire de Sillage (`SCRIPTS`), du plus courant au plus rare : l'ordre
    /// compte, le tirage pondéré le parcourt.
    pub const ALL: [Self; STUNT_COUNT] = [
        Self::OffRoad,
        Self::Slalom,
        Self::NearMiss,
        Self::Loop,
        Self::HeadOn,
        Self::Inverted,
        Self::WallRide,
        Self::Threading,
        Self::SignHop,
        Self::Rooftop,
        Self::BigJump,
        Self::Ascension,
    ];

    /// Rang dans [`Self::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::OffRoad => 0,
            Self::Slalom => 1,
            Self::NearMiss => 2,
            Self::Loop => 3,
            Self::HeadOn => 4,
            Self::Inverted => 5,
            Self::WallRide => 6,
            Self::Threading => 7,
            Self::SignHop => 8,
            Self::Rooftop => 9,
            Self::BigJump => 10,
            Self::Ascension => 11,
        }
    }

    /// Le slug écrit dans les looks et les drops (`"off-road"`, `"wall-ride"`…).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OffRoad => "off-road",
            Self::Slalom => "slalom",
            Self::NearMiss => "near-miss",
            Self::Loop => "loop",
            Self::HeadOn => "head-on",
            Self::Inverted => "inverted",
            Self::WallRide => "wall-ride",
            Self::Threading => "threading",
            Self::SignHop => "sign-hop",
            Self::Rooftop => "rooftop",
            Self::BigJump => "big-jump",
            Self::Ascension => "ascension",
        }
    }

    /// L'identifiant de fraîcheur (`stunt:<slug>`), celui que le chef du spectacle connaît. Statique : le tirage le
    /// demande à chaque image d'attente, sans rien formater.
    #[must_use]
    pub const fn freshness_id(self) -> &'static str {
        match self {
            Self::OffRoad => "stunt:off-road",
            Self::Slalom => "stunt:slalom",
            Self::NearMiss => "stunt:near-miss",
            Self::Loop => "stunt:loop",
            Self::HeadOn => "stunt:head-on",
            Self::Inverted => "stunt:inverted",
            Self::WallRide => "stunt:wall-ride",
            Self::Threading => "stunt:threading",
            Self::SignHop => "stunt:sign-hop",
            Self::Rooftop => "stunt:rooftop",
            Self::BigJump => "stunt:big-jump",
            Self::Ascension => "stunt:ascension",
        }
    }

    /// La cascade de ce slug, si elle existe au catalogue.
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == slug)
    }
}

impl TryFrom<&ShowStuntKind> for StuntKind {
    type Error = UnknownStunt;

    fn try_from(kind: &ShowStuntKind) -> Result<Self, Self::Error> {
        Self::from_slug(kind.as_str()).ok_or_else(|| UnknownStunt(kind.as_str().to_owned()))
    }
}

/// Un identifiant de cascade qui n'est pas au catalogue.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "cascade inconnue : « {0} » (catalogue : off-road, slalom, near-miss, loop, head-on, inverted, wall-ride, threading, sign-hop, rooftop, big-jump, ascension)"
)]
pub struct UnknownStunt(pub String);

/// Les cascades de la liste qui ne sont pas au catalogue (liste vide : tout est valide). Pour valider un look ou un
/// drop à l'ouverture d'un show, pas par image.
#[must_use]
pub fn unknown_kinds<'a>(kinds: impl IntoIterator<Item = &'a ShowStuntKind>) -> Vec<UnknownStunt> {
    kinds
        .into_iter()
        .filter_map(|kind| StuntKind::try_from(kind).err())
        .collect()
}

/// Valide les portes d'un look : cascades de la liste blanche et rangs permis. `Err` : tous les identifiants inconnus.
///
/// # Errors
///
/// Renvoie la liste de ce qui n'est ni une cascade ni un rang du catalogue.
pub fn validate_gates(gates: &ShowGates) -> Result<(), Vec<String>> {
    let mut bad: Vec<String> = unknown_kinds(&gates.stunt_kinds).into_iter().map(|e| e.0).collect();
    bad.extend(
        gates
            .stunt_tiers
            .iter()
            .filter(|tier| StuntTier::from_slug(tier.as_str()).is_none())
            .map(|tier| format!("rang {}", tier.as_str())),
    );
    if bad.is_empty() { Ok(()) } else { Err(bad) }
}

/// Rareté : les grosses cascades sont tirées plus rarement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StuntTier {
    /// Courante.
    Common,
    /// Peu courante.
    Uncommon,
    /// Rare.
    Rare,
}

impl StuntTier {
    /// Le slug du rang dans les looks.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Common => "common",
            Self::Uncommon => "uncommon",
            Self::Rare => "rare",
        }
    }

    /// Le rang de ce slug.
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        [Self::Common, Self::Uncommon, Self::Rare]
            .into_iter()
            .find(|tier| tier.as_str() == slug)
    }

    /// Les rangs permis par un look contiennent-ils celui-ci ?
    #[must_use]
    pub fn allowed_by(self, tiers: &[ShowStuntTier]) -> bool {
        tiers.iter().any(|t| t.as_str() == self.as_str())
    }

    /// Facteur du poids de tirage (`TIER_FACTOR`).
    #[must_use]
    pub const fn factor(self) -> f64 {
        match self {
            Self::Common => 1.0,
            Self::Uncommon => 0.7,
            Self::Rare => 0.4,
        }
    }
}

/// Départ : sur une mesure, sur une phrase (16 temps) ou sur un drop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StuntAlign {
    /// Sur une mesure.
    Bar,
    /// Sur une phrase.
    Phrase,
    /// Sur un drop.
    Drop,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_catalogue_est_coherent() {
        for (i, kind) in StuntKind::ALL.into_iter().enumerate() {
            assert_eq!(kind.index(), i);
            assert_eq!(StuntKind::from_slug(kind.as_str()), Some(kind));
            assert_eq!(kind.freshness_id(), format!("stunt:{}", kind.as_str()));
        }
        assert!(StuntKind::from_slug("moonwalk").is_none());
    }

    #[test]
    fn cf_show_ne_connait_que_des_cascades_du_catalogue() {
        // La liste blanche du look « ouvert » et l'Ascension du chef des drops : tout doit exister ici.
        let gates = cf_show::gates::open_gates();
        assert!(validate_gates(&gates).is_ok());
        assert_eq!(
            gates.stunt_kinds.len(),
            STUNT_COUNT - 1,
            "l'Ascension a sa propre porte"
        );
        let ascension = ShowStuntKind::new("ascension").unwrap();
        assert_eq!(StuntKind::try_from(&ascension), Ok(StuntKind::Ascension));
    }

    #[test]
    fn une_cascade_inconnue_est_signalee() {
        let bad = ShowStuntKind::new("moonwalk").unwrap();
        let errors = unknown_kinds([&bad]);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].to_string().contains("moonwalk"));
    }
}
