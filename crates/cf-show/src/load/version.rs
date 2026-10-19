//! Lecture et écriture RON, numéro de version et migrations.
//!
//! Chaque fichier commence par `version: N`. On lit d'abord ce seul champ (en ignorant le reste), puis on désérialise
//! avec le type de cette version et on migre, version par version, jusqu'au format courant.
//!
//! Ajouter une version (changement incompatible du format) :
//! 1. copier les types actuels concernés dans un module figé `load::v<N>` (ils ne bougeront plus) ;
//! 2. incrémenter [`FORMAT_VERSION`] et modifier le modèle ;
//! 3. écrire `fn migrate_v<N>(old: v<N>::Event) -> v<N+1>::Event` et ajouter le bras `N => …` dans [`parse_event`]
//!    et [`parse_look`] (chaîner les migrations : v1 → v2 → … → courant) ;
//! 4. garder un fichier d'exemple de l'ancienne version dans les tests.
//!
//! Le format est né en version 1 : aucune migration n'existe encore.

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::LoadError;
use crate::model::{Event, FORMAT_VERSION, Look};

/// Extensions RON actives à la lecture comme à l'écriture : `implicit_some` évite d'écrire `Some(…)` autour des
/// champs facultatifs (`logo: "logos/krmn.png"` plutôt que `logo: Some("logos/krmn.png")`).
fn options() -> ron::Options {
    ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
}

/// Désérialise `text` en gardant le chemin du champ fautif et la position en cas d'erreur.
pub(crate) fn deserialize<T: DeserializeOwned>(text: &str, origin: &str) -> Result<T, LoadError> {
    let options = options();
    let mut de =
        ron::Deserializer::from_str_with_options(text, &options).map_err(|error| syntax_error(origin, None, &error))?;
    let value = match serde_path_to_error::deserialize(&mut de) {
        Ok(value) => value,
        Err(error) => {
            let path = error.path().to_string();
            let field = (path != ".").then_some(path);
            let spanned = de.span_error(error.into_inner());
            return Err(syntax_error(origin, field, &spanned));
        }
    };
    de.end()
        .map_err(|error| syntax_error(origin, None, &de.span_error(error)))?;
    Ok(value)
}

fn syntax_error(origin: &str, field: Option<String>, error: &ron::error::SpannedError) -> LoadError {
    LoadError::Syntax {
        origin: origin.to_owned(),
        field,
        line: error.span.start.line,
        column: error.span.start.col,
        message: error.code.to_string(),
    }
}

/// Écrit une valeur du modèle en RON lisible (indentation, une entrée par ligne).
pub(crate) fn serialize<T: Serialize>(value: &T) -> Result<String, LoadError> {
    let pretty = ron::ser::PrettyConfig::new()
        .struct_names(true)
        .indentor("    ".to_owned());
    Ok(options().to_string_pretty(value, pretty)?)
}

/// Seul le champ `version` est lu ; le nom de structure (`Event`, `Look`) est vérifié au passage par RON.
#[derive(serde::Deserialize)]
#[serde(rename = "Event")]
struct EventHeader {
    version: Option<u32>,
}

#[derive(serde::Deserialize)]
#[serde(rename = "Look")]
struct LookHeader {
    version: Option<u32>,
}

/// Situe une version lue par rapport au format courant.
fn check_version(found: Option<u32>, origin: &str) -> Result<u32, LoadError> {
    match found {
        None => Err(LoadError::MissingVersion {
            origin: origin.to_owned(),
        }),
        Some(found) if found > FORMAT_VERSION => Err(LoadError::TooNew {
            origin: origin.to_owned(),
            found,
        }),
        Some(0) => Err(LoadError::UnknownVersion {
            origin: origin.to_owned(),
            found: 0,
        }),
        Some(found) => Ok(found),
    }
}

/// Lit un événement de n'importe quelle version connue et le migre au format courant (sans validation).
///
/// # Errors
///
/// Version absente, inconnue ou trop récente ; RON invalide pour le format de cette version.
pub fn parse_event(text: &str, origin: &str) -> Result<Event, LoadError> {
    let header: EventHeader = deserialize(text, origin)?;
    match check_version(header.version, origin)? {
        FORMAT_VERSION => deserialize(text, origin),
        // Aucune ancienne version pour l'instant : chaque migration ajoutera son bras ici (`1 => …`).
        found => Err(LoadError::UnknownVersion {
            origin: origin.to_owned(),
            found,
        }),
    }
}

/// Lit un look de n'importe quelle version connue et le migre au format courant (sans validation).
///
/// # Errors
///
/// Version absente, inconnue ou trop récente ; RON invalide pour le format de cette version.
pub fn parse_look(text: &str, origin: &str) -> Result<Look, LoadError> {
    let header: LookHeader = deserialize(text, origin)?;
    match check_version(header.version, origin)? {
        FORMAT_VERSION => deserialize(text, origin),
        found => Err(LoadError::UnknownVersion {
            origin: origin.to_owned(),
            found,
        }),
    }
}
