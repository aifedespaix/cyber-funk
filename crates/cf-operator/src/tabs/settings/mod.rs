//! Onglet Réglages (Préparation) : la soirée (profils, import/export, DJs, fiche du DJ) au centre, l'audio, le MIDI,
//! les contrôles et la mise à jour dans leurs panneaux (repris tels quels).
//!
//! ```text
//! barre : Réglages ● modifié · [Passer en Live] · profil ▾ · Nouveau · Dupliquer · Supprimer
//!         Fichier [chemin] Importer Exporter · Annuler/Rétablir
//! ┌ DJs de la soirée ─┐┌ Édition du DJ ───────────────┐( Écran de sortie : tâche suivante, Panel::right )
//! │ 1. … (glisser,    ││ formulaires du créneau, cues │
//! │    Alt+↑/↓)       ││──────────────────────────────│
//! │ + Ajouter …       ││ ● non enregistré  Annuler  Enregistrer [Ctrl+S]
//! └───────────────────┘└──────────────────────────────┘
//! ```
//!
//! Les changements qui feraient perdre ou contourner des modifications non enregistrées (changer de DJ, de profil,
//! importer, quitter) demandent d'abord confirmation. En Live, cet onglet n'est pas dessiné (la vue Live le remplace)
//! et toute confirmation en attente est abandonnée : aucun dialogue d'ici n'est atteignable devant le public.

pub(crate) mod audio_panel;
mod confirm;
pub(crate) mod controls_panel;
mod dj_form;
mod dj_list;
pub(crate) mod learn_panel;
pub(crate) mod profiles;
pub(crate) mod restore;
#[cfg(test)]
mod tests;
mod transfer;
pub(crate) mod update_panel;

use std::path::PathBuf;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::egui;

use crate::prepare::panel::{self, Op};
use crate::prepare::widgets::Touch;
use crate::prepare::{PrepSession, Status};
#[cfg(test)]
use confirm::{Choice, resolve};
pub(crate) use confirm::{Pending, guard};
use dj_form::DjFormView;
use dj_list::ListFocus;

/// Une demande de la liste ou de la fiche, traitée après le dessin (la session est alors à jour).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Request {
    SelectDj(usize),
    Save,
    Revert,
}

/// État de l'onglet (jamais sauvegardé) : dossier et liste des profils, saisie d'un nom, confirmation en attente.
#[derive(Resource, Debug, Default)]
pub(crate) struct SettingsTab {
    /// Dossier des profils, `None` : pas de profils (pas de dossier de configuration).
    pub profiles_dir: Option<PathBuf>,
    /// Profils lisibles du dossier, relus après chaque geste sur les profils (jamais à chaque image).
    pub profiles: Vec<String>,
    /// Champ « nom du profil » (Nouveau, Dupliquer).
    pub name_input: String,
    pub pending: Option<Pending>,
    /// Geste de l'opérateur sur les profils à recopier dans les réglages (`restore::remember_evening`).
    pub remember: Option<Remembered>,
    focus: ListFocus,
}

/// Ce qu'un geste de l'opérateur sur les profils fait retenir pour le prochain lancement. Seuls ces gestes écrivent
/// `OperatorPrefs::{active_profile, last_event_path}` : un repli au démarrage ou un `--show` n'y touche jamais.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Remembered {
    /// Ce profil est désormais l'actif (enregistré, ouvert ou créé) ; son fichier devient le dernier ouvert.
    Profile { name: String, path: PathBuf },
    /// Le profil actif vient d'être supprimé.
    NoProfile,
}

impl Remembered {
    /// Applique le geste aux préférences.
    pub(crate) fn apply(&self, prefs: &mut cf_show::persist::OperatorPrefs) {
        match self {
            Self::Profile { name, path } => {
                prefs.active_profile = Some(name.clone());
                prefs.last_event_path = Some(path.display().to_string());
            }
            Self::NoProfile => prefs.active_profile = None,
        }
    }
}

impl SettingsTab {
    pub(crate) fn new(profiles_dir: Option<PathBuf>) -> Self {
        let mut tab = Self {
            profiles_dir,
            ..Self::default()
        };
        tab.refresh_profiles();
        tab
    }

    /// Relit la liste des profils sur le disque.
    pub(crate) fn refresh_profiles(&mut self) {
        self.profiles = self
            .profiles_dir
            .as_deref()
            .map(profiles::list_profiles)
            .unwrap_or_default();
    }
}

/// La session de préparation et l'état de l'onglet, posés ensemble au démarrage.
#[derive(SystemParam)]
pub(crate) struct PrepParams<'w> {
    pub session: Option<ResMut<'w, PrepSession>>,
    pub tab: Option<ResMut<'w, SettingsTab>>,
}

impl PrepParams<'_> {
    /// Abandonne la confirmation en attente (passage en Live). Ne touche la ressource que s'il y en a une.
    pub(crate) fn drop_pending(&mut self) {
        if let Some(tab) = self.tab.as_mut().filter(|t| t.pending.is_some()) {
            tab.pending = None;
        }
    }
}

/// Réponses du dialogue de sortie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QuitChoice {
    /// Enregistrer la soirée (profil actif), puis quitter.
    SaveAndQuit,
    /// Quitter en perdant les modifications non enregistrées.
    QuitWithoutSaving,
    /// Quitter (rien à enregistrer).
    Quit,
    Cancel,
}

/// Ce que propose le dialogue de sortie : avec une soirée modifiée, d'enregistrer d'abord.
pub(crate) fn quit_choices(dirty: bool) -> &'static [(QuitChoice, &'static str)] {
    if dirty {
        &[
            (QuitChoice::SaveAndQuit, "Enregistrer et quitter"),
            (QuitChoice::QuitWithoutSaving, "Quitter sans enregistrer"),
            (QuitChoice::Cancel, "Annuler"),
        ]
    } else {
        &[(QuitChoice::Quit, "Quitter"), (QuitChoice::Cancel, "Annuler")]
    }
}

/// Dessine l'onglet en Préparation. `mode_button` ajoute « Passer en Live » à la barre. Rend le rectangle de la
/// carte des DJ (tests de mise en page).
pub(crate) fn draw(
    ui: &mut egui::Ui,
    session: &mut PrepSession,
    tab: &mut SettingsTab,
    library_ids: &[String],
    mode_button: impl FnOnce(&mut egui::Ui),
) -> egui::Rect {
    if panel::shortcuts(ui.ctx(), session) && tab.pending.is_none() {
        profiles::save_evening(session, tab);
    }
    ui.horizontal_wrapped(|ui| {
        ui.heading("Réglages");
        if session.is_dirty() {
            ui.colored_label(egui::Color32::YELLOW, "● modifié");
        }
        mode_button(ui);
    });
    // Règle 8 : rien ne se charge pendant un show ; le show qui joue est celui lu à l'ouverture.
    ui.weak("Les changements sont joués au prochain lancement (profil actif, ou --show).");
    profiles::bar(ui, session, tab);
    ui.horizontal_wrapped(|ui| {
        transfer::bar(ui, session, tab);
        ui.separator();
        panel::history_buttons(ui, session);
    });
    panel::messages(ui, session);
    ui.separator();

    let mut draft = session.event().clone();
    let mut touch = Touch::default();
    let mut selection = session.selection;
    let mut ops = Vec::new();
    let mut requests = Vec::new();
    let looks = session.look_ids();
    let flagged = panel::flagged_paths(session);
    let modified = if selection.event {
        session.settings_modified()
    } else {
        session.slot_modified(selection.slot)
    };

    let list = egui::Panel::left("dj_list")
        .default_size(260.0)
        .resizable(true)
        .show(ui, |ui| {
            dj_list::draw(
                ui,
                &draft,
                &mut selection,
                &flagged,
                &mut tab.focus,
                &mut ops,
                &mut requests,
            );
        })
        .response
        .rect;
    // La carte « Écran de sortie » viendra ici, en `Panel::right`, avant la fiche centrale.
    egui::CentralPanel::default().show(ui, |ui| {
        DjFormView {
            event: &mut draft,
            selection: &mut selection,
            looks: &looks,
            library_ids,
            flagged: &flagged,
            modified,
            touch: &mut touch,
            ops: &mut ops,
            requests: &mut requests,
        }
        .show(ui);
    });

    apply_ops(&mut draft, &mut selection, &ops, &mut touch);
    session.selection = selection;
    session.commit(draft, touch.key());
    for request in requests {
        handle(session, tab, request);
    }
    confirm::confirmation(ui.ctx(), session, tab);
    list
}

/// Applique les modifications structurelles. Un déplacement garde la sélection sur le même DJ (déplacer une autre
/// ligne ne doit pas changer la fiche affichée sans confirmation).
fn apply_ops(
    draft: &mut cf_show::model::Event,
    selection: &mut crate::prepare::Selection,
    ops: &[Op],
    touch: &mut Touch,
) {
    for &op in ops {
        let selected = draft.slots.get(selection.slot).map(|s| s.id.clone());
        let next = panel::apply(draft, *selection, op);
        *selection = match (op, selected) {
            (Op::MoveSlot(..), Some(id)) if !selection.event => crate::prepare::Selection {
                slot: draft.slots.iter().position(|s| s.id == id).unwrap_or(next.slot),
                ..*selection
            },
            _ => next,
        };
        touch.structural();
    }
}

fn handle(session: &mut PrepSession, tab: &mut SettingsTab, request: Request) {
    match request {
        Request::SelectDj(target) => {
            dj_list::select_dj(session, tab, target);
        }
        Request::Save => {
            profiles::save_evening(session, tab);
        }
        Request::Revert => {
            if session.selection.event {
                session.revert_settings();
            } else {
                session.revert_slot(session.selection.slot);
            }
            session.status = Status::Info("Modifications de la fiche annulées".to_owned());
        }
    }
}
