//! Réglages de la ville : ce qui change son contenu sans changer ses tirages (qualité, zones dégagées).

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
}

impl Default for SceneryConfig {
    fn default() -> Self {
        Self {
            far_layers: FAR_BY_QUALITY[0],
            clearings: Vec::new(),
            sign_clearing: None,
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

    pub(super) fn cleared(&self, z: f64, side: i8) -> bool {
        self.sign_clearing.is_some_and(|c| c.covers(z, side)) || self.clearings.iter().any(|c| c.covers(z, side))
    }
}
