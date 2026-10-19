//! Validation du contenu : plages numériques, longueurs de texte, doublons, références de looks.
//!
//! La lecture RON vérifie déjà la forme (champs, types, syntaxe des identifiants et des chemins de macro) ; ici on
//! vérifie le sens. On ne s'arrête pas au premier problème : l'opérateur voit tout ce qu'il doit corriger, chaque
//! problème avec le chemin de son champ (`slots[1].cues[3].look`).

mod event;
mod look;

use std::collections::BTreeSet;
use std::fmt;
use std::ops::RangeInclusive;

pub use event::validate_event;
pub use look::validate_look;

use crate::model::ParamPath;

/// Un problème trouvé dans un fichier : où (chemin du champ) et quoi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// Chemin du champ fautif, à la manière de `serde_path_to_error` : `slots[0].cues[2].advance.seconds`.
    pub path: String,
    /// Ce qui ne va pas, en clair.
    pub message: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} : {}", self.path, self.message)
    }
}

/// Accumulateur de problèmes, avec les vérifications communes.
#[derive(Debug, Default)]
pub(crate) struct Report {
    issues: Vec<Issue>,
}

/// `prefix.field`, ou `field` à la racine.
pub(crate) fn join(prefix: &str, field: &str) -> String {
    if prefix.is_empty() {
        field.to_owned()
    } else {
        format!("{prefix}.{field}")
    }
}

/// `prefix[index]`.
pub(crate) fn index(prefix: &str, i: usize) -> String {
    format!("{prefix}[{i}]")
}

impl Report {
    pub(crate) fn into_issues(self) -> Vec<Issue> {
        self.issues
    }

    pub(crate) fn push(&mut self, path: String, message: impl Into<String>) {
        self.issues.push(Issue {
            path,
            message: message.into(),
        });
    }

    /// Nombre fini dans la plage (bornes incluses).
    pub(crate) fn f32_in(&mut self, path: String, value: f32, range: RangeInclusive<f32>) {
        if !value.is_finite() {
            self.push(path, format!("{value} n'est pas un nombre fini"));
        } else if !range.contains(&value) {
            self.push(
                path,
                format!("{value} hors de la plage permise [{}, {}]", range.start(), range.end()),
            );
        }
    }

    /// Valeur d'un paramètre pilotable, dans la plage de [`ParamPath::range`].
    pub(crate) fn param(&mut self, path: String, value: f32, param: ParamPath) {
        self.f32_in(path, value, param.range());
    }

    /// Entier dans la plage (bornes incluses).
    pub(crate) fn int_in<T: PartialOrd + fmt::Display>(&mut self, path: String, value: T, range: RangeInclusive<T>) {
        if !range.contains(&value) {
            self.push(
                path,
                format!("{value} hors de la plage permise [{}, {}]", range.start(), range.end()),
            );
        }
    }

    /// Texte de `min` à `max` caractères ; s'il est obligatoire (`min > 0`), pas fait que d'espaces.
    pub(crate) fn text(&mut self, path: String, text: &str, min: usize, max: usize) {
        let len = text.chars().count();
        if min > 0 && text.trim().is_empty() {
            self.push(path, "ne doit pas être vide");
        } else if len < min || len > max {
            self.push(path, format!("{len} caractères, il en faut de {min} à {max}"));
        }
    }

    /// Nombre d'éléments d'une liste.
    pub(crate) fn count(&mut self, path: String, len: usize, min: usize, max: usize) {
        if len < min || len > max {
            self.push(path, format!("{len} élément(s), il en faut de {min} à {max}"));
        }
    }

    /// Signale chaque élément déjà vu plus haut dans la liste, au chemin `prefix[i]`.
    pub(crate) fn unique<'a, T: Ord + fmt::Display + 'a>(&mut self, prefix: &str, items: impl Iterator<Item = &'a T>) {
        let mut seen = BTreeSet::new();
        for (i, item) in items.enumerate() {
            if !seen.insert(item) {
                self.push(index(prefix, i), format!("« {item} » en double"));
            }
        }
    }
}
