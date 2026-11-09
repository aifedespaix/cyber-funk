//! Un ensemble de types de quartier : où un événement a sa place.
//!
//! Petit masque de bits, `Copy`, sans allocation : l'affinité d'un événement avec les quartiers se lit à chaque image
//! par le tirage du trafic.

use super::DistrictKind;

/// Un sous-ensemble des trois types de quartier. Le défaut est « partout ».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DistrictSet(u8);

impl Default for DistrictSet {
    fn default() -> Self {
        Self::ALL
    }
}

impl DistrictSet {
    /// Tous les quartiers.
    pub const ALL: Self = Self(0b111);
    /// Aucun quartier (un événement qu'aucun quartier n'accueille).
    pub const NONE: Self = Self(0);

    /// Les quartiers donnés.
    #[must_use]
    pub fn of(kinds: &[DistrictKind]) -> Self {
        Self(kinds.iter().fold(0, |mask, kind| mask | 1 << kind.index()))
    }

    /// Ce quartier en fait partie.
    #[must_use]
    pub fn contains(self, kind: DistrictKind) -> bool {
        self.0 & (1 << kind.index()) != 0
    }

    /// Tous les quartiers en font partie (aucune restriction).
    #[must_use]
    pub fn is_all(self) -> bool {
        self == Self::ALL
    }

    /// Les types présents, dans l'ordre de [`DistrictKind::ALL`].
    pub fn kinds(self) -> impl Iterator<Item = DistrictKind> {
        DistrictKind::ALL.into_iter().filter(move |kind| self.contains(*kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_everywhere() {
        let all = DistrictSet::default();
        assert!(all.is_all());
        for kind in DistrictKind::ALL {
            assert!(all.contains(kind));
        }
    }

    #[test]
    fn a_set_keeps_only_its_kinds_in_order() {
        let set = DistrictSet::of(&[DistrictKind::Residential, DistrictKind::Neon]);
        assert!(set.contains(DistrictKind::Neon));
        assert!(!set.contains(DistrictKind::Suburb));
        assert!(set.contains(DistrictKind::Residential));
        assert!(!set.is_all());
        let kinds: Vec<_> = set.kinds().collect();
        assert_eq!(kinds, [DistrictKind::Neon, DistrictKind::Residential]);
    }

    #[test]
    fn none_contains_nothing() {
        assert!(DistrictKind::ALL.into_iter().all(|k| !DistrictSet::NONE.contains(k)));
    }
}
