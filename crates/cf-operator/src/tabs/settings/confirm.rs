//! Les confirmations de l'onglet Réglages : quel geste attend, quelles réponses on propose, et le dialogue modal.
//!
//! Un geste qui ferait perdre ou contourner des modifications non enregistrées (changer de DJ, de profil, importer)
//! passe par [`guard`] ; supprimer un profil est toujours confirmé.

use std::path::PathBuf;

use bevy_egui::egui;

use super::{SettingsTab, dj_list, profiles};
use crate::prepare::{PrepSession, Status};

/// Un geste qui attend la confirmation de l'opérateur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Pending {
    /// Passer au DJ de ce rang alors que la fiche affichée est modifiée.
    SwitchDj(usize),
    /// Ouvrir ce profil alors que la session est modifiée.
    OpenProfile(String),
    /// Créer ce profil (événement neuf) alors que la session est modifiée.
    NewProfile(String),
    /// Importer ce fichier alors que la session est modifiée.
    Import(PathBuf),
    /// Supprimer ce profil (toujours confirmé).
    DeleteProfile(String),
}

/// Réponse de l'opérateur à une confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Choice {
    /// Enregistrer d'abord, puis faire le geste.
    SaveFirst,
    /// Revenir à la version enregistrée de la fiche, puis changer de DJ.
    Discard,
    /// Faire le geste sans enregistrer.
    Proceed,
    /// Ne rien faire.
    Stay,
}

/// Fait `pending` tout de suite si rien n'est modifié, sinon le met en attente de confirmation.
pub(crate) fn guard(session: &mut PrepSession, tab: &mut SettingsTab, pending: Pending) {
    if session.is_dirty() {
        tab.pending = Some(pending);
    } else {
        perform(session, tab, &pending);
    }
}

/// Le geste lui-même, une fois confirmé.
pub(crate) fn perform(session: &mut PrepSession, tab: &mut SettingsTab, pending: &Pending) {
    match pending {
        Pending::SwitchDj(target) => {
            session.selection = crate::prepare::Selection {
                slot: *target,
                cue: None,
                event: false,
            };
        }
        Pending::OpenProfile(name) => {
            profiles::open_profile(session, tab, name);
        }
        Pending::NewProfile(name) => {
            profiles::new_profile(session, tab, name);
        }
        // Un fichier importé devient la cible de l'enregistrement (Ctrl+S le réécrit) : plus de profil actif dans
        // la session, mais le profil retenu pour le prochain lancement ne change pas.
        Pending::Import(path) => {
            if session.open(path) {
                session.set_active_profile(None);
            }
        }

        Pending::DeleteProfile(name) => profiles::delete_profile(session, tab, name),
    }
}

/// Les réponses proposées pour une confirmation, avec leur libellé.
pub(crate) fn choices(pending: &Pending) -> &'static [(Choice, &'static str)] {
    match pending {
        Pending::SwitchDj(_) => &[
            (Choice::SaveFirst, "Enregistrer"),
            (Choice::Discard, "Abandonner les modifications"),
            (Choice::Proceed, "Changer sans enregistrer"),
            (Choice::Stay, "Rester"),
        ],
        Pending::DeleteProfile(_) => &[(Choice::Proceed, "Supprimer"), (Choice::Stay, "Annuler")],
        Pending::OpenProfile(_) | Pending::NewProfile(_) | Pending::Import(_) => &[
            (Choice::SaveFirst, "Enregistrer d'abord"),
            (Choice::Proceed, "Continuer sans enregistrer"),
            (Choice::Stay, "Rester"),
        ],
    }
}

/// Applique la réponse de l'opérateur à la confirmation en attente.
pub(crate) fn resolve(session: &mut PrepSession, tab: &mut SettingsTab, choice: Choice) {
    let Some(pending) = tab.pending.take() else {
        return;
    };
    match choice {
        Choice::Stay => {}
        Choice::Proceed => perform(session, tab, &pending),
        // Un enregistrement refusé (événement invalide) laisse la question ouverte : rien n'est perdu.
        Choice::SaveFirst => {
            if profiles::save_evening(session, tab) {
                perform(session, tab, &pending);
            } else {
                tab.pending = Some(pending);
            }
        }
        Choice::Discard => {
            let Pending::SwitchDj(target) = pending else {
                return;
            };
            // Un DJ neuf abandonné disparaît : on retrouve la cible par son identifiant.
            let target_id = session.event().slots.get(target).map(|s| s.id.clone());
            session.revert_slot(session.selection.slot);
            let target = target_id
                .and_then(|id| session.event().slots.iter().position(|s| s.id == id))
                .unwrap_or(0);
            perform(session, tab, &Pending::SwitchDj(target));
        }
    }
}

/// Le dialogue de confirmation en attente, modal (il bloque le reste de l'onglet). `Échap` vaut « Rester ».
pub(crate) fn confirmation(ctx: &egui::Context, session: &mut PrepSession, tab: &mut SettingsTab) {
    let Some(pending) = tab.pending.clone() else {
        return;
    };
    let text = match &pending {
        Pending::SwitchDj(_) => format!(
            "Le DJ « {} » a des modifications non enregistrées.",
            session
                .event()
                .slots
                .get(session.selection.slot)
                .map_or("?", dj_list::slot_label)
        ),
        Pending::OpenProfile(name) => format!("Modifications non enregistrées. Ouvrir le profil « {name} » ?"),
        Pending::NewProfile(name) => format!("Modifications non enregistrées. Créer le profil « {name} » ?"),
        Pending::Import(path) => format!("Modifications non enregistrées. Importer {} ?", path.display()),
        Pending::DeleteProfile(name) => format!("Supprimer le profil « {name} » ? Son fichier est effacé."),
    };
    let mut picked = None;
    let modal = egui::Modal::new(egui::Id::new("settings_confirm")).show(ctx, |ui| {
        ui.set_max_width(420.0);
        ui.label(text);
        if let Status::Error(message) = &session.status {
            ui.colored_label(egui::Color32::LIGHT_RED, message);
        }
        ui.horizontal_wrapped(|ui| {
            for (choice, label) in choices(&pending) {
                if ui.button(*label).clicked() {
                    picked = Some(*choice);
                }
            }
        });
    });
    if picked.is_none() && modal.should_close() {
        picked = Some(Choice::Stay);
    }
    if let Some(choice) = picked {
        resolve(session, tab, choice);
    }
}
