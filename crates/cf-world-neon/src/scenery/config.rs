//! Réglages de la ville : ce qui change son contenu sans changer ses tirages (qualité, zones dégagées).

use super::districts::DistrictPlan;
use super::graffiti_plan::GraffitiLibrary;

/// Couronnes lointaines conservées selon le niveau de qualité (0 = tout, 3 = le strict minimum).
pub const FAR_BY_QUALITY: [usize; 4] = [3, 3, 2, 1];

/// Zone sans immeuble : de `z0` à `z1` (z monde, `z0 < z1`), des deux côtés (`side` = 0) ou d'un seul (−1 gauche, 1
/// droite). La ville s'écarte devant une enseigne ou un événement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clearing {
    /// Début, en m (le plus négatif).
    pub z0: f64,
    /// Fin, en m.
    pub z1: f64,
    /// −1 : côté gauche, 1 : côté droit, 0 : les deux.
    pub side: i8,
}

impl Clearing {
    /// La zone, élargie de 8 m de chaque côté, couvre-t-elle le point `z` du côté `side` ?
    pub(super) fn covers(&self, z: f64, side: i8) -> bool {
        z > self.z0 - 8.0 && z < self.z1 + 8.0 && (self.side == 0 || self.side == side)
    }
}

/// Réglages de la ville : ce qui change son contenu sans changer ses tirages.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneryConfig {
    far_layers: usize,
    clearings: Vec<Clearing>,
    /// Zone dégagée pour l'enseigne du DJ : indépendante de celles du trafic, qui remplacent leur liste à chaque
    /// événement.
    sign_clearing: Option<Clearing>,
    /// Plan de quartiers de la route, et son interrupteur (faux : la ville historique).
    pub(super) districts: DistrictPlan,
    pub(super) districts_active: bool,
    /// Mots et placements des graffitis ; vide par défaut (la ville historique, sans graffiti ni projecteur).
    graffiti: GraffitiLibrary,
    /// Multiples de densité des graffitis muraux et des projecteurs automatiques (réglage F4) : 1 = comme livré, 0 = aucun.
    graffiti_scale: (f32, f32),
    /// Quartier vide (bac à sable du Debug) : plus aucun immeuble, enseigne ni graffiti, la route seule.
    empty: bool,
}

impl Default for SceneryConfig {
    fn default() -> Self {
        Self {
            far_layers: FAR_BY_QUALITY[0],
            clearings: Vec::new(),
            sign_clearing: None,
            districts: DistrictPlan::default(),
            districts_active: false,
            graffiti: GraffitiLibrary::default(),
            graffiti_scale: (1.0, 1.0),
            empty: false,
        }
    }
}

impl SceneryConfig {
    /// Couronnes lointaines gardées pour le niveau de qualité `level` (0 = tout). Renvoie vrai si la ville change :
    /// l'appelant re-tire alors les tronçons vivants (`ChunkStream::replan_all`).
    pub fn set_quality(&mut self, level: usize) -> bool {
        let far = FAR_BY_QUALITY[level.min(FAR_BY_QUALITY.len() - 1)];
        std::mem::replace(&mut self.far_layers, far) != far
    }

    /// Couronnes lointaines affichées.
    #[must_use]
    pub fn far_layers(&self) -> usize {
        self.far_layers
    }

    /// Remplace les zones dégagées. Renvoie vrai si la liste change (à re-tirer comme [`Self::set_quality`]).
    pub fn set_clearings(&mut self, clearings: &[Clearing]) -> bool {
        if self.clearings == clearings {
            return false;
        }
        self.clearings.clear();
        self.clearings.extend_from_slice(clearings);
        true
    }

    /// Zone dégagée de l'enseigne du DJ (ou `None`). Renvoie vrai si elle change.
    pub fn set_sign_clearing(&mut self, clearing: Option<Clearing>) -> bool {
        std::mem::replace(&mut self.sign_clearing, clearing) != clearing
    }

    /// Remplace la bibliothèque de graffitis. Renvoie vrai si elle change (à re-tirer comme [`Self::set_quality`]).
    pub fn set_graffiti(&mut self, lib: GraffitiLibrary) -> bool {
        if self.graffiti == lib {
            return false;
        }
        self.graffiti = lib;
        true
    }

    /// Vide (ou rend) la ville. Renvoie vrai si cela change (à re-tirer comme [`Self::set_quality`]).
    pub fn set_empty(&mut self, empty: bool) -> bool {
        std::mem::replace(&mut self.empty, empty) != empty
    }

    /// La ville est vidée (bac à sable).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.empty
    }

    /// Règle les densités de graffitis muraux et de projecteurs automatiques (multiples de la densité livrée, 0 : aucun).
    /// Renvoie vrai si elles changent (à re-tirer comme [`Self::set_quality`]).
    pub fn set_graffiti_scale(&mut self, wall: f32, holo: f32) -> bool {
        std::mem::replace(&mut self.graffiti_scale, (wall, holo)) != (wall, holo)
    }

    /// Le quartier `look` avec les densités de graffitis réglées à la main. À densité 1, il est rendu tel quel.
    pub(super) fn tuned(&self, look: &super::districts::DistrictLook) -> super::districts::DistrictLook {
        let scale = |one_in: u32, density: f32| {
            if one_in == 0 || density <= 0.0 {
                0
            } else {
                ((one_in as f32 / density).round() as u32).max(1)
            }
        };
        super::districts::DistrictLook {
            wall_one_in: scale(look.wall_one_in, self.graffiti_scale.0),
            holo_one_in: scale(look.holo_one_in, self.graffiti_scale.1),
            ..*look
        }
    }

    /// La bibliothèque de graffitis.
    #[must_use]
    pub fn graffiti(&self) -> &GraffitiLibrary {
        &self.graffiti
    }

    pub(super) fn cleared(&self, z: f64, side: i8) -> bool {
        self.sign_clearing.is_some_and(|c| c.covers(z, side)) || self.clearings.iter().any(|c| c.covers(z, side))
    }
}
