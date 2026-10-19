//! Les sources audio que le thread d'analyse sait écouter, et entre lesquelles l'opérateur bascule à chaud.

use std::fmt;

/// Sorte de source audio.
///
/// Le thread d'analyse garde un emplacement par sorte : un flux de capture rouvert après un débranchement remplace
/// le précédent dans son emplacement sans toucher au lecteur de démo, et l'opérateur choisit laquelle est analysée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceKind {
    /// Micro ou entrée ligne d'une carte son (capture `cpal`, module [`crate::capture`]).
    Capture,
    /// Lecteur des pistes de démo (J1-02) : le signal joué est recopié vers l'analyse.
    Demo,
}

impl SourceKind {
    /// Nombre de sortes de sources (taille des tableaux indexés par [`SourceKind::index`]).
    pub const COUNT: usize = 2;

    /// Toutes les sortes, dans l'ordre de [`SourceKind::index`].
    pub const ALL: [Self; Self::COUNT] = [Self::Capture, Self::Demo];

    /// Rang dans un tableau de [`SourceKind::COUNT`] éléments.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Capture => 0,
            Self::Demo => 1,
        }
    }

    /// Nom court pour les journaux et l'interface opérateur.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Capture => "capture",
            Self::Demo => "démo",
        }
    }
}

impl fmt::Display for SourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_matches_all_order() {
        for (i, kind) in SourceKind::ALL.iter().enumerate() {
            assert_eq!(kind.index(), i);
        }
    }
}
