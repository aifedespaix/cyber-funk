//! Le badge de mise à jour, dans la barre d'onglets : une pastille colorée qui dit qu'une version existe, avec le
//! bouton pour l'installer.
//!
//! Ce crate ne fait aucun accès réseau : le binaire interroge GitHub et écrit [`UpdateStatus`] ; l'opérateur affiche
//! l'état et émet [`InstallUpdate`] quand l'utilisateur accepte. Pendant un show, l'installation est refusée : elle
//! quitte l'application.

use bevy::prelude::*;
use bevy_egui::egui;

/// Où en est la recherche ou l'installation d'une mise à jour.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub enum UpdateStatus {
    /// Pas de vérification faite (ou rien à signaler) : aucun badge.
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

/// Orange des mises à jour proposées.
const AVAILABLE: egui::Color32 = egui::Color32::from_rgb(235, 150, 30);
/// Vert d'une mise à jour installée.
const INSTALLED: egui::Color32 = egui::Color32::from_rgb(40, 150, 90);
/// Rouge d'un échec.
const FAILED: egui::Color32 = egui::Color32::from_rgb(190, 55, 55);

/// Ce que le badge affiche.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BadgeSpec {
    pub text: String,
    pub color: egui::Color32,
    /// Le badge porte le bouton « Installer ».
    pub installable: bool,
    pub detail: Option<String>,
}

/// Le badge de cet état ; `None` quand il n'y a rien à signaler.
pub(crate) fn badge_spec(status: &UpdateStatus) -> Option<BadgeSpec> {
    let spec = |text: String, color, installable, detail| BadgeSpec {
        text,
        color,
        installable,
        detail,
    };
    match status {
        UpdateStatus::Idle => None,
        UpdateStatus::Available { version, notes } => Some(spec(
            format!("Mise à jour {version} disponible"),
            AVAILABLE,
            true,
            notes.clone().filter(|n| !n.trim().is_empty()),
        )),
        UpdateStatus::AvailableManual { version } => Some(spec(
            format!("Mise à jour {version} disponible"),
            AVAILABLE,
            false,
            Some("Téléchargez-la sur la page des releases GitHub de cyber-funk.".to_owned()),
        )),
        UpdateStatus::Installing { version } => Some(spec(
            format!("Installation de {version}…"),
            AVAILABLE,
            false,
            Some("Ne fermez pas l'application.".to_owned()),
        )),
        UpdateStatus::Installed { version } => {
            Some(spec(format!("{version} installée : relancez"), INSTALLED, false, None))
        }
        UpdateStatus::Failed(reason) => Some(spec(
            "Mise à jour impossible".to_owned(),
            FAILED,
            false,
            Some(reason.clone()),
        )),
    }
}

/// Dessine le badge à droite de la barre d'onglets ; rend `true` si l'opérateur clique sur « Installer ».
/// `show_running` : pas d'installation en plein show.
pub(crate) fn draw(ui: &mut egui::Ui, status: &UpdateStatus, show_running: bool) -> bool {
    let Some(spec) = badge_spec(status) else {
        return false;
    };
    let mut install = false;
    let response = egui::Frame::new()
        .fill(spec.color)
        .corner_radius(14.0)
        .inner_margin(egui::Margin::symmetric(12, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if matches!(status, UpdateStatus::Installing { .. }) {
                    ui.spinner();
                }
                ui.label(egui::RichText::new(&spec.text).strong().color(egui::Color32::WHITE));
                if spec.installable {
                    let button = egui::Button::new("Installer").small();
                    let response = ui.add_enabled(!show_running, button);
                    let hint = if show_running {
                        "Pas pendant le show : arrêtez-le d'abord."
                    } else {
                        "Télécharge la mise à jour, puis ferme l'application."
                    };
                    install = response.on_hover_text(hint).clicked();
                }
            });
        })
        .response;
    if let Some(detail) = spec.detail {
        response.on_hover_text(detail);
    }
    install
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pas_de_badge_sans_mise_a_jour() {
        assert_eq!(badge_spec(&UpdateStatus::Idle), None);
    }

    #[test]
    fn une_version_disponible_propose_l_installation() {
        let spec = badge_spec(&UpdateStatus::Available {
            version: "0.4.2".into(),
            notes: Some("Correctifs".into()),
        })
        .expect("badge");
        assert_eq!(spec.text, "Mise à jour 0.4.2 disponible");
        assert_eq!(spec.color, AVAILABLE);
        assert!(spec.installable);
        assert_eq!(spec.detail.as_deref(), Some("Correctifs"));
    }

    #[test]
    fn des_notes_vides_ne_font_pas_d_infobulle() {
        let spec = badge_spec(&UpdateStatus::Available {
            version: "1".into(),
            notes: Some("  ".into()),
        })
        .expect("badge");
        assert_eq!(spec.detail, None);
    }

    #[test]
    fn seule_la_mise_a_jour_automatique_a_un_bouton() {
        let manual = badge_spec(&UpdateStatus::AvailableManual { version: "1".into() }).expect("badge");
        assert!(!manual.installable);
        for status in [
            UpdateStatus::Installing { version: "1".into() },
            UpdateStatus::Installed { version: "1".into() },
            UpdateStatus::Failed("réseau".into()),
        ] {
            assert!(!badge_spec(&status).expect("badge").installable, "{status:?}");
        }
    }
}
