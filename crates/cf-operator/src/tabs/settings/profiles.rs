//! Les profils de soirée : un profil est un fichier d'événement RON, `<config>/cyber-funk/profiles/<nom>.ron`. Ses DJ
//! sont les créneaux (`DjSlot`) de l'événement ; aucune seconde représentation n'existe.
//!
//! `Ctrl+S` enregistre dans le profil actif ; sans profil actif, il crée « Soirée » (ou « Soirée 2 »… si le nom est
//! pris) et le rend actif. L'écriture est celle d'`EventDocument` : copie `.bak` de l'ancien fichier puis
//! `write_atomic`.

use std::path::{Path, PathBuf};

use bevy_egui::egui;

use super::{Pending, SettingsTab};
use crate::prepare::{PrepSession, Status};

/// Nom du premier profil créé par un enregistrement sans profil actif.
const FIRST_PROFILE: &str = "Soirée";
/// Extension des fichiers de profil.
const EXTENSION: &str = "ron";
/// Caractères interdits dans un nom de fichier sous Windows (les séparateurs y compris).
const FORBIDDEN: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Le fichier du profil `name` dans `dir`. `None` pour un nom vide (une fois les espaces retirés), contenant un
/// séparateur de chemin, `..`, un caractère interdit sous Windows ou un caractère de contrôle.
pub(crate) fn profile_path(dir: &Path, name: &str) -> Option<PathBuf> {
    let name = name.trim();
    let refused = name.is_empty()
        || name.contains("..")
        || name
            .chars()
            .any(|c| FORBIDDEN.contains(&c) || c.is_control() || std::path::is_separator(c));
    (!refused).then(|| dir.join(format!("{name}.{EXTENSION}")))
}

/// Le nom du profil d'un fichier situé dans `dir`, s'il en est un.
pub(crate) fn profile_name_of(dir: &Path, path: &Path) -> Option<String> {
    let name = path.file_stem()?.to_str()?;
    (profile_path(dir, name)?.as_path() == path).then(|| name.to_owned())
}

/// Les profils de `dir`, triés par ordre alphabétique (sans tenir compte de la casse). Les fichiers illisibles ou
/// qui ne sont pas des événements sont ignorés ; un dossier absent donne une liste vide.
pub(crate) fn list_profiles(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == EXTENSION) && path.is_file())
        .filter_map(|path| {
            let name = profile_name_of(dir, &path)?;
            let text = std::fs::read_to_string(&path).ok()?;
            cf_show::load::parse_event(&text, &name).ok()?;
            Some(name)
        })
        .collect();
    names.sort_by_cached_key(|name| (name.to_lowercase(), name.clone()));
    names
}

/// Un nom de profil libre : `base`, sinon `base 2`, `base 3`… (un fichier présent, même illisible, n'est jamais
/// écrasé par un nouveau profil).
fn free_name(dir: &Path, base: &str) -> Option<String> {
    std::iter::once(base.to_owned())
        .chain((2..1000).map(|n| format!("{base} {n}")))
        .find(|name| profile_path(dir, name).is_some_and(|path| !path.exists()))
}

/// Refuse d'écrire un événement invalide : il ne se rouvrirait pas au prochain lancement. Les problèmes restent
/// affichés sous la barre, comme d'habitude.
fn refuse_invalid(session: &mut PrepSession) -> bool {
    if session.issues().is_empty() {
        return false;
    }
    session.status = Status::Error(format!(
        "Enregistrement refusé : {} problème(s) à corriger dans l'événement.",
        session.issues().len()
    ));
    true
}

/// `Ctrl+S` / « Enregistrer » : dans le profil actif, ou un nouveau profil « Soirée » rendu actif. Sans dossier de
/// profils, le fichier associé ou le chemin saisi (comportement d'avant les profils). Rend `true` si c'est enregistré.
pub(crate) fn save_evening(session: &mut PrepSession, tab: &mut SettingsTab) -> bool {
    if refuse_invalid(session) {
        return false;
    }
    let Some(dir) = tab.profiles_dir.clone() else {
        return save_without_profiles(session);
    };
    let Some(name) = session
        .active_profile()
        .map(str::to_owned)
        .or_else(|| free_name(&dir, FIRST_PROFILE))
    else {
        session.status = Status::Error("Aucun nom de profil libre.".to_owned());
        return false;
    };

    save_to_profile(session, tab, &dir, &name)
}

/// Enregistre l'événement dans le profil `name` et le rend actif.
fn save_to_profile(session: &mut PrepSession, tab: &mut SettingsTab, dir: &Path, name: &str) -> bool {
    let Some(path) = profile_path(dir, name) else {
        session.status = Status::Error(format!("Nom de profil invalide : « {name} »."));
        return false;
    };
    if let Err(e) = std::fs::create_dir_all(dir) {
        session.status = Status::Error(format!("dossier des profils impossible à créer : {e}"));
        return false;
    }
    if !session.save_as(&path) {
        return false;
    }
    session.set_active_profile(Some(name.trim().to_owned()));
    session.status = Status::Info(format!("Profil « {} » enregistré", name.trim()));
    tab.refresh_profiles();
    true
}

fn save_without_profiles(session: &mut PrepSession) -> bool {
    if session.doc_path().is_some() {
        session.save();
        return matches!(session.status, Status::Info(_));
    }
    let typed = session.path_input.trim().to_owned();
    if typed.is_empty() {
        session.status = Status::Error("Indique d'abord un chemin de fichier (Exporter).".to_owned());
        return false;
    }
    session.save_as(&PathBuf::from(typed))
}

/// Ouvre le profil `name` et le rend actif (l'appelant a déjà confirmé si la session était modifiée).
pub(crate) fn open_profile(session: &mut PrepSession, tab: &SettingsTab, name: &str) -> bool {
    let Some(path) = tab.profiles_dir.as_deref().and_then(|dir| profile_path(dir, name)) else {
        return false;
    };
    if !session.open(&path) {
        return false;
    }
    session.set_active_profile(Some(name.to_owned()));
    true
}

/// Crée le profil `name` à partir d'un événement neuf et le rend actif.
pub(crate) fn new_profile(session: &mut PrepSession, tab: &mut SettingsTab, name: &str) -> bool {
    let Some(dir) = tab.profiles_dir.clone() else {
        return false;
    };
    if !session.restart_blank() {
        return false;
    }
    save_to_profile(session, tab, &dir, name)
}

/// Supprime le fichier du profil `name` (après confirmation). L'édition en cours reste ; si c'était le profil actif,
/// il n'y a plus de profil actif (le prochain `Ctrl+S` en recrée un).
pub(crate) fn delete_profile(session: &mut PrepSession, tab: &mut SettingsTab, name: &str) {
    let Some(path) = tab.profiles_dir.as_deref().and_then(|dir| profile_path(dir, name)) else {
        return;
    };
    match std::fs::remove_file(&path) {
        Ok(()) => {
            if session.active_profile() == Some(name) {
                session.set_active_profile(None);
            }
            session.status = Status::Info(format!("Profil « {name} » supprimé"));
        }
        Err(e) => session.status = Status::Error(format!("suppression impossible : {e}")),
    }
    tab.refresh_profiles();
}

/// Le nom saisi, s'il est utilisable pour un nouveau profil (valide et pas déjà pris) ; sinon le message d'erreur.
fn typed_new_name(tab: &SettingsTab) -> Result<String, String> {
    let name = tab.name_input.trim();
    let dir = tab.profiles_dir.as_deref().ok_or("Profils indisponibles.")?;
    let path = profile_path(dir, name).ok_or_else(|| format!("Nom de profil invalide : « {name} »."))?;
    if path.exists() {
        return Err(format!("Le profil « {name} » existe déjà."));
    }
    Ok(name.to_owned())
}

/// La barre des profils : sélecteur, nom, Nouveau, Dupliquer, Supprimer.
pub(crate) fn bar(ui: &mut egui::Ui, session: &mut PrepSession, tab: &mut SettingsTab) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Profil de soirée");
        if tab.profiles_dir.is_none() {
            ui.weak("indisponible (pas de dossier de configuration)");
            return;
        }
        let active = session.active_profile().map(str::to_owned);
        let mut picked = None;
        egui::ComboBox::from_id_salt("evening_profile")
            .selected_text(active.as_deref().unwrap_or("— aucun —"))
            .show_ui(ui, |ui| {
                for name in &tab.profiles {
                    if ui.selectable_label(active.as_deref() == Some(name), name).clicked() {
                        picked = Some(name.clone());
                    }
                }
                if tab.profiles.is_empty() {
                    ui.weak("Aucun profil enregistré.");
                }
            });
        if let Some(name) = picked.filter(|name| active.as_deref() != Some(name)) {
            super::guard(session, tab, Pending::OpenProfile(name));
        }
        ui.add(
            egui::TextEdit::singleline(&mut tab.name_input)
                .desired_width(140.0)
                .hint_text("nom du profil"),
        );
        let typed = !tab.name_input.trim().is_empty();
        if ui.add_enabled(typed, egui::Button::new("Nouveau")).clicked() {
            match typed_new_name(tab) {
                Ok(name) => super::guard(session, tab, Pending::NewProfile(name)),
                Err(message) => session.status = Status::Error(message),
            }
        }
        if ui
            .add_enabled(typed, egui::Button::new("Dupliquer"))
            .on_hover_text("Enregistre l'édition en cours sous ce nouveau nom, qui devient le profil actif.")
            .clicked()
        {
            match typed_new_name(tab) {
                Ok(name) => {
                    if !refuse_invalid(session)
                        && let Some(dir) = tab.profiles_dir.clone()
                    {
                        save_to_profile(session, tab, &dir, &name);
                    }
                }
                Err(message) => session.status = Status::Error(message),
            }
        }
        if ui
            .add_enabled(active.is_some(), egui::Button::new("Supprimer"))
            .clicked()
            && let Some(name) = active
        {
            tab.pending = Some(Pending::DeleteProfile(name));
        }
    });
}
