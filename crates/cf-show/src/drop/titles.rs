//! Cartons-titres des drops (portage de `show/drop-titles.ts`), lus dans un fichier RON.
//!
//! Format (`assets/titles/drop-titles.ron`) :
//!
//! ```ron
//! DropTitles(
//!     version: 1,
//!     titles: [
//!         DropTitle(id: "drop", fr: "Drop", en: "Drop", min: 2),
//!         DropTitle(id: "noreturn", fr: "Pas de retour", en: "No way back", min: 5),
//!     ],
//! )
//! ```

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::ShowLevel;
use crate::load::{LoadError, check_version, deserialize};
use crate::model::FORMAT_VERSION;
use crate::model::ids::check_slug;
use crate::validate::Issue;

/// Longueur maximale d'un titre, en caractères : il se lit d'un coup d'œil sur l'écran du public.
pub const MAX_TITLE_LEN: usize = 32;

/// Langue d'affichage des cartons-titres. Sillage lit celle de la page ; ici c'est un réglage du show, français par
/// défaut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TitleLanguage {
    /// Français.
    #[default]
    Fr,
    /// Anglais.
    En,
}

/// Un carton-titre de drop, tel qu'écrit dans le fichier : une paire FR/EN. `min` est le niveau à partir duquel il
/// sert (les plus emphatiques attendent les niveaux hauts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DropTitle {
    /// Slug stable, unique dans le fichier ; sert de clé de fraîcheur (`title:<id>`).
    pub id: String,
    /// Texte français (affiché en capitales).
    pub fr: String,
    /// Texte anglais (affiché en capitales).
    pub en: String,
    /// Niveau minimal, 0 à 5 ([`super::MAX_LEVEL`]).
    pub min: ShowLevel,
}

/// Le fichier tel qu'il est lu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename = "DropTitles")]
struct DropTitlesFile {
    version: u32,
    titles: Vec<DropTitle>,
}

/// Seul le champ `version` est lu en premier ; le nom de structure est vérifié au passage par RON.
#[derive(Deserialize)]
#[serde(rename = "DropTitles")]
struct Header {
    version: Option<u32>,
}

/// Un titre prêt à servir : textes déjà en capitales (calculés une fois au chargement, pas à chaque drop) et clé de
/// fraîcheur déjà écrite.
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub(crate) key: String,
    pub(crate) min: ShowLevel,
    fr: Arc<str>,
    en: Arc<str>,
}

impl Entry {
    /// Le texte dans la langue demandée (copie du pointeur partagé : pas d'allocation).
    pub(crate) fn text(&self, language: TitleLanguage) -> Arc<str> {
        match language {
            TitleLanguage::Fr => Arc::clone(&self.fr),
            TitleLanguage::En => Arc::clone(&self.en),
        }
    }
}

/// Les cartons-titres d'un show, validés. Vide = jamais de titre.
#[derive(Debug, Clone, Default)]
pub struct DropTitles {
    entries: Vec<Entry>,
}

impl DropTitles {
    /// Aucun titre : les drops ne portent jamais de carton.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Lit, vérifie et valide un fichier de titres.
    ///
    /// # Errors
    ///
    /// Fichier illisible, RON invalide, version non prise en charge, ou contenu invalide (identifiant mal formé ou
    /// doublé, texte vide ou trop long, niveau hors 0 à 5).
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        let text = std::fs::read_to_string(path).map_err(|source| LoadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_ron(&text, &path.display().to_string())
    }

    /// Comme [`DropTitles::load`] sur un texte RON ; `origin` nomme la source dans les messages d'erreur.
    ///
    /// # Errors
    ///
    /// Voir [`DropTitles::load`].
    pub fn from_ron(text: &str, origin: &str) -> Result<Self, LoadError> {
        let header: Header = deserialize(text, origin)?;
        let found = check_version(header.version, origin)?;
        if found != FORMAT_VERSION {
            // Aucune ancienne version pour l'instant : chaque migration ajoutera son bras ici.
            return Err(LoadError::UnknownVersion {
                origin: origin.to_owned(),
                found,
            });
        }
        let file: DropTitlesFile = deserialize(text, origin)?;
        Self::new(file.titles, origin)
    }

    /// Valide des titres déjà en mémoire.
    ///
    /// # Errors
    ///
    /// [`LoadError::Invalid`] avec tous les problèmes trouvés.
    pub fn new(titles: Vec<DropTitle>, origin: &str) -> Result<Self, LoadError> {
        let issues = validate(&titles);
        if !issues.is_empty() {
            return Err(LoadError::Invalid {
                origin: origin.to_owned(),
                issues,
            });
        }
        let entries = titles
            .into_iter()
            .map(|t| Entry {
                key: format!("title:{}", t.id),
                min: t.min,
                fr: t.fr.to_uppercase().into(),
                en: t.en.to_uppercase().into(),
            })
            .collect();
        Ok(Self { entries })
    }

    /// Nombre de titres.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Vrai s'il n'y a aucun titre.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clés de fraîcheur (`title:<id>`) de tous les titres.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.key.as_str())
    }

    /// Le texte (en capitales) du titre d'identifiant `id` dans la langue demandée, ou `None` s'il n'existe pas.
    /// Copie du pointeur partagé : aucune allocation (sert à l'action `Title` de l'opérateur).
    #[must_use]
    pub fn text(&self, id: &str, language: TitleLanguage) -> Option<Arc<str>> {
        self.entries
            .iter()
            .find(|e| e.key.strip_prefix("title:") == Some(id))
            .map(|e| e.text(language))
    }

    pub(crate) fn entries(&self) -> &[Entry] {
        &self.entries
    }
}

fn validate(titles: &[DropTitle]) -> Vec<Issue> {
    let mut issues = Vec::new();
    let mut seen = BTreeSet::new();
    let mut push = |path: String, message: String| issues.push(Issue { path, message });
    for (i, t) in titles.iter().enumerate() {
        if let Err(reason) = check_slug(&t.id) {
            push(
                format!("titles[{i}].id"),
                format!("identifiant « {} » refusé : {reason}", t.id),
            );
        } else if !seen.insert(t.id.as_str()) {
            push(format!("titles[{i}].id"), format!("identifiant « {} » en double", t.id));
        }
        for (field, text) in [("fr", &t.fr), ("en", &t.en)] {
            let len = text.trim().chars().count();
            if len == 0 {
                push(format!("titles[{i}].{field}"), "texte vide".to_owned());
            } else if len > MAX_TITLE_LEN {
                push(
                    format!("titles[{i}].{field}"),
                    format!("{len} caractères, {MAX_TITLE_LEN} au plus"),
                );
            }
        }
        if t.min > super::MAX_LEVEL {
            push(
                format!("titles[{i}].min"),
                format!("niveau {} hors 0 à {}", t.min, super::MAX_LEVEL),
            );
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"DropTitles(version: 1, titles: [
        DropTitle(id: "drop", fr: "Drop", en: "Drop", min: 2),
        DropTitle(id: "letgo", fr: "Lâchez tout", en: "Let go", min: 2),
    ])"#;

    #[test]
    fn reads_pairs_and_uppercases_once() {
        let titles = DropTitles::from_ron(GOOD, "test").unwrap();
        assert_eq!(titles.len(), 2);
        assert_eq!(&*titles.entries()[1].text(TitleLanguage::Fr), "LÂCHEZ TOUT");
        assert_eq!(&*titles.entries()[1].text(TitleLanguage::En), "LET GO");
        assert_eq!(titles.keys().collect::<Vec<_>>(), ["title:drop", "title:letgo"]);
    }

    #[test]
    fn rejects_bad_content_with_field_paths() {
        let text = r#"DropTitles(version: 1, titles: [
            DropTitle(id: "ok", fr: "A", en: "B", min: 2),
            DropTitle(id: "ok", fr: "", en: "B", min: 9),
            DropTitle(id: "Pas Slug", fr: "A", en: "B", min: 0),
        ])"#;
        let error = DropTitles::from_ron(text, "test").unwrap_err();
        let LoadError::Invalid { issues, .. } = error else {
            unreachable!("autre erreur : {error}");
        };
        let paths: Vec<&str> = issues.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(paths, ["titles[1].id", "titles[1].fr", "titles[1].min", "titles[2].id"]);
    }

    #[test]
    fn rejects_syntax_version_and_unknown_fields() {
        assert!(matches!(
            DropTitles::from_ron("DropTitles(titles: [])", "t"),
            Err(LoadError::MissingVersion { .. })
        ));
        assert!(matches!(
            DropTitles::from_ron("DropTitles(version: 9, titles: [])", "t"),
            Err(LoadError::TooNew { found: 9, .. })
        ));
        let extra = r#"DropTitles(version: 1, titles: [DropTitle(id: "a", fr: "A", en: "A", min: 2, rare: true)])"#;
        let error = DropTitles::from_ron(extra, "t").unwrap_err();
        assert_eq!(error.field(), Some("titles[0].rare"), "{error}");
        assert!(DropTitles::from_ron("DropTitles(version: 1, titles: [", "t").is_err());
    }

    #[test]
    fn too_long_text_is_refused() {
        let long = "x".repeat(MAX_TITLE_LEN + 1);
        let titles = vec![DropTitle {
            id: "long".to_owned(),
            fr: long,
            en: "ok".to_owned(),
            min: 2,
        }];
        assert!(DropTitles::new(titles, "t").is_err());
    }
}
