//! L'affinité des événements avec les quartiers : dans quels types de quartier un événement a sa place.
//!
//! Sans entrée, un événement va **partout**. Une entrée le limite à certains types ; le tirage spontané du trafic ne le
//! choisit plus ailleurs, et l'onglet Debug l'affiche grisé hors de ses quartiers (l'opérateur peut le forcer quand même
//! pour juger du rendu avant de demander qu'on l'autorise).
//!
//! La table vient d'un fichier RON (`assets/districts/affinity.ron`), lu une fois à l'ouverture : un agent n'a qu'à
//! l'éditer pour autoriser un événement dans un quartier. Une entrée nomme un événement par son identifiant de catalogue
//! (`"landmark:arcade"`) ou par sa famille, la partie avant le `:` (`"landmark"`).

use std::collections::BTreeMap;

use bevy::prelude::*;
use cf_show::district::{DistrictKind, DistrictSet};
use serde::Deserialize;

/// Le contenu du fichier : identifiant ou famille → slugs des quartiers.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AffinityFile {
    events: BTreeMap<String, Vec<String>>,
}

/// La table d'affinité. Le défaut est vide : tout partout.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct DistrictAffinity {
    entries: BTreeMap<String, DistrictSet>,
}

impl DistrictAffinity {
    /// Lit la table. Ne panique jamais : une erreur de syntaxe donne une table vide, un slug inconnu écarte l'entrée, et
    /// chaque problème est rendu en clair pour le journal de démarrage.
    #[must_use]
    pub fn from_ron(text: &str) -> (Self, Vec<String>) {
        let mut problems = Vec::new();
        let file: AffinityFile = match ron::from_str(text) {
            Ok(file) => file,
            Err(error) => {
                problems.push(format!("affinity.ron illisible, aucune restriction : {error}"));
                return (Self::default(), problems);
            }
        };
        let mut entries = BTreeMap::new();
        for (id, slugs) in file.events {
            let mut kinds = Vec::with_capacity(slugs.len());
            let mut known = true;
            for slug in &slugs {
                if let Some(kind) = DistrictKind::ALL.into_iter().find(|kind| kind.slug() == slug) {
                    kinds.push(kind);
                } else {
                    problems.push(format!(
                        "affinity.ron : « {id} » nomme le quartier inconnu « {slug} », entrée écartée"
                    ));
                    known = false;
                }
            }
            if known {
                entries.insert(id, DistrictSet::of(&kinds));
            }
        }
        (Self { entries }, problems)
    }

    /// Les quartiers de cet événement : son entrée, sinon celle de sa famille, sinon tous.
    #[must_use]
    pub fn districts_of(&self, event_id: &str) -> DistrictSet {
        self.entries
            .get(event_id)
            .or_else(|| event_id.split(':').next().and_then(|family| self.entries.get(family)))
            .copied()
            .unwrap_or_default()
    }

    /// Cet événement a sa place dans ce type de quartier.
    #[must_use]
    pub fn allows(&self, event_id: &str, kind: DistrictKind) -> bool {
        self.districts_of(event_id).contains(kind)
    }

    /// Nombre d'événements restreints.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Aucune restriction.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"(events: {"landmark": ["neon"], "landmark:arcade": ["neon", "suburb"], "storm": []})"#;

    #[test]
    fn no_entry_means_everywhere() {
        let table = DistrictAffinity::default();
        for kind in DistrictKind::ALL {
            assert!(table.allows("chase", kind));
        }
    }

    #[test]
    fn an_exact_entry_wins_over_its_family() {
        let (table, problems) = DistrictAffinity::from_ron(SAMPLE);
        assert!(problems.is_empty(), "{problems:?}");
        assert!(table.allows("landmark:arcade", DistrictKind::Suburb), "entrée exacte");
        assert!(!table.allows("landmark:other", DistrictKind::Suburb), "famille");
        assert!(table.allows("landmark:other", DistrictKind::Neon));
    }

    #[test]
    fn an_empty_list_means_no_district() {
        let (table, _) = DistrictAffinity::from_ron(SAMPLE);
        assert!(DistrictKind::ALL.into_iter().all(|kind| !table.allows("storm", kind)));
    }

    #[test]
    fn a_syntax_error_gives_an_empty_table_and_a_message() {
        let (table, problems) = DistrictAffinity::from_ron("(events: {");
        assert!(table.is_empty());
        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn an_unknown_district_drops_only_that_entry() {
        let (table, problems) = DistrictAffinity::from_ron(r#"(events: {"tower": ["moon"], "adship": ["neon"]})"#);
        assert_eq!(problems.len(), 1);
        assert!(
            table.allows("tower", DistrictKind::Suburb),
            "l'entrée écartée ne restreint rien"
        );
        assert!(!table.allows("adship", DistrictKind::Suburb));
    }
}
