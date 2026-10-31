//! Réglages du logiciel, sauvegardés tout seuls et relus au relancement.
//!
//! [`Settings`] porte les [`AppSettings`] de `cf-show` (type pur, lecture tolérante). Les panneaux et les plugins
//! qui retiennent un réglage l'écrivent dans `Settings::value` ; [`systems::save_when_changed`] l'écrit sur disque
//! une seconde après le dernier changement, sur le pool d'E/S (jamais sur le fil d'image, skill `live-safety`).

mod debounce;
mod systems;

use std::path::PathBuf;

use bevy::prelude::*;
use cf_show::persist::AppSettings;

pub use debounce::{Debounce, QUIET_S};

/// Les réglages en cours et le fichier où ils vivent.
#[derive(Resource, Debug)]
pub struct Settings {
    /// Les valeurs. Les écrire (`ResMut`) déclenche l'écriture automatique.
    pub value: AppSettings,
    path: PathBuf,
}

impl Settings {
    /// Réglages relus de `path` (ou ceux par défaut).
    #[must_use]
    pub fn new(value: AppSettings, path: PathBuf) -> Self {
        Self { value, path }
    }

    /// Le fichier de réglages.
    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

/// Installe [`Settings`] et son écriture automatique.
#[derive(Debug, Clone)]
pub struct SettingsPlugin {
    /// Fichier de réglages.
    pub path: PathBuf,
    /// Valeurs déjà relues par l'appelant (il en a besoin avant la construction des plugins).
    pub initial: AppSettings,
}

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Settings::new(self.initial.clone(), self.path.clone()))
            .init_resource::<systems::WriteState>()
            .add_systems(Last, systems::save_when_changed);
    }
}

#[cfg(test)]
mod tests;
