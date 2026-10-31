//! Bandeau de mise à jour de la fenêtre opérateur (J9-06).
//!
//! Ce crate ne fait aucun accès réseau : le binaire interroge GitHub et écrit [`UpdateStatus`] ; l'opérateur affiche
//! l'état et émet [`InstallUpdate`] quand l'utilisateur accepte. L'installation n'est jamais proposée en vue Live :
//! le bandeau n'est dessiné que dans la vue de préparation.

use bevy::prelude::*;
use bevy_egui::egui;

/// Où en est la recherche ou l'installation d'une mise à jour.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub enum UpdateStatus {
    /// Pas de vérification faite (ou rien à signaler) : aucun bandeau.
    #[default]
    Idle,
    /// Une version plus récente existe.
    Available {
        /// Numéro de la nouvelle version.
        version: String,
        /// Notes de version, si la release en fournit.
        notes: Option<String>,
    },
    /// Une version plus récente existe, mais cette installation ne peut pas se remplacer elle-même (archive portable,
    /// lancement depuis les sources) : l'opérateur la télécharge à la main.
    AvailableManual {
        /// Numéro de la nouvelle version.
        version: String,
    },
    /// Téléchargement et installation en cours.
    Installing {
        /// Numéro de la version en cours d'installation.
        version: String,
    },
    /// Installée : elle prend effet au prochain lancement.
    Installed {
        /// Numéro de la version installée.
        version: String,
    },
    /// L'installation a échoué ; le message dit pourquoi.
    Failed(String),
}

/// Demande de l'opérateur : télécharger et installer la mise à jour proposée.
#[derive(Message, Debug, Clone, Copy)]
pub struct InstallUpdate;

/// Dessine le bandeau ; renvoie `true` si l'opérateur clique sur « Installer ».
pub(crate) fn draw(ui: &mut egui::Ui, status: &UpdateStatus) -> bool {
    let mut install = false;
    match status {
        UpdateStatus::Idle => return false,
        UpdateStatus::Available { version, notes } => {
            ui.horizontal(|ui| {
                ui.strong(format!("Version {version} disponible."));
                install = ui.button("Installer et quitter").clicked();
            });
            if let Some(notes) = notes.as_deref().filter(|n| !n.trim().is_empty()) {
                egui::CollapsingHeader::new("Notes de version").show(ui, |ui| {
                    ui.label(notes);
                });
            }
        }
        UpdateStatus::AvailableManual { version } => {
            ui.strong(format!(
                "Version {version} disponible : téléchargez-la sur la page des releases GitHub de cyber-funk."
            ));
        }
        UpdateStatus::Installing { version } => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!(
                    "Installation de la version {version}… ne fermez pas l'application."
                ));
            });
        }
        UpdateStatus::Installed { version } => {
            ui.strong(format!(
                "Version {version} installée : relancez cyber-funk pour l'utiliser."
            ));
        }
        UpdateStatus::Failed(reason) => {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("Mise à jour impossible : {reason}"));
        }
    }
    ui.separator();
    install
}
