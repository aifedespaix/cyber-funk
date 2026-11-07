//! Les profils de soirée : un profil est un fichier d'événement RON, `<config>/cyber-funk/profiles/<nom>.ron`. Ses DJ
//! sont les créneaux (`DjSlot`) de l'événement ; aucune seconde représentation n'existe.
//!
//! `Ctrl+S` enregistre là d'où vient la soirée : le profil actif ; sinon le fichier ouvert (`--show`, Importer), qui
//! est réécrit sans toucher aux profils ; sinon (soirée neuve, sans fichier) un nouveau profil « Soirée » (ou
//! « Soirée 2 »… si le nom est pris), rendu actif. « Dupliquer » enregistre sous un profil nommé depuis n'importe
//! quelle soirée. L'écriture est celle d'`EventDocument` : copie `.bak` de l'ancien fichier puis `write_atomic`.
//!
//! Seuls ces gestes sur les profils font retenir le profil actif pour le prochain lancement (`Remembered`).

use std::path::{Path, PathBuf};

use bevy_egui::egui;

use super::{Pending, Remembered, SettingsTab};
use crate::prepare::{PrepSession, Status};

/// Nom du premier profil créé par un enregistrement sans profil actif.
const FIRST_PROFILE: &str = "Soirée";
/// Extension des fichiers de profil.
const EXTENSION: &str = "ron";
/// Caractères interdits dans un nom de fichier sous Windows (les séparateurs y compris).
const FORBIDDEN: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Le fichier du profil `name` dans `dir`. `None` pour un nom vide (une fois les espaces retirés), contenant un
/// séparateur de chemin, `..`, un caractère interdit sous Windows, un caractère de contrôle, un nom réservé de
/// Windows (`CON`, `NUL`, `COM1`…, avec ou sans extension) ou un nom qui finit par un point.
pub(crate) fn profile_path(dir: &Path, name: &str) -> Option<PathBuf> {
    let name = name.trim();
    let refused = name.is_empty()
        || name.contains("..")
        || name.ends_with('.')
        || reserved_on_windows(name)
        || name
            .chars()
            .any(|c| FORBIDDEN.contains(&c) || c.is_control() || std::path::is_separator(c));
    (!refused).then(|| dir.join(format!("{name}.{EXTENSION}")))
}

/// Les noms de périphériques de Windows, refusés même suivis d'une extension (`nul.txt` ouvre le périphérique).
fn reserved_on_windows(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end().to_ascii_uppercase();
    let numbered = |prefix: &str| {
        stem.strip_prefix(prefix)
            .is_some_and(|n| n.len() == 1 && n != "0" && n.bytes().all(|b| b.is_ascii_digit()))
    };
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") || numbered("COM") || numbered("LPT")
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

/// `Ctrl+S` / « Enregistrer » : dans le profil actif ; sinon dans le fichier ouvert (sans toucher aux profils) ;
/// sinon dans un nouveau profil « Soirée » rendu actif. Sans dossier de profils, le chemin saisi en dernier recours.
/// Rend `true` si c'est enregistré.
pub(crate) fn save_evening(session: &mut PrepSession, tab: &mut SettingsTab) -> bool {
    if refuse_invalid(session) {
        return false;
    }
    let dir = tab.profiles_dir.clone();
    if let (Some(dir), Some(name)) = (&dir, session.active_profile().map(str::to_owned)) {
        return save_to_profile(session, tab, dir, &name);
    }
    if session.doc_path().is_some() {
        session.save();
        return matches!(session.status, Status::Info(_));
    }
    let Some(dir) = dir else {
        return save_to_typed_path(session);
    };
    let Some(name) = free_name(&dir, FIRST_PROFILE) else {
        session.status = Status::Error("Aucun nom de profil libre.".to_owned());
        return false;
    };
    save_to_profile(session, tab, &dir, &name)
}

/// Enregistre l'événement dans le profil `name` et le rend actif (et retenu pour le prochain lancement).
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
    let name = name.trim().to_owned();
    session.set_active_profile(Some(name.clone()));
    session.status = Status::Info(format!("Profil « {name} » enregistré"));
    tab.remember = Some(Remembered::Profile { name, path });
    tab.refresh_profiles();
    true
}

/// Sans dossier de profils ni fichier associé : le chemin saisi dans la barre (comportement d'avant les profils).
fn save_to_typed_path(session: &mut PrepSession) -> bool {
    let typed = session.path_input.trim().to_owned();
    if typed.is_empty() {
        session.status = Status::Error("Indique d'abord un chemin de fichier (Exporter).".to_owned());
        return false;
    }
    session.save_as(&PathBuf::from(typed))
}

/// Ouvre le profil `name` et le rend actif (l'appelant a déjà confirmé si la session était modifiée).
pub(crate) fn open_profile(session: &mut PrepSession, tab: &mut SettingsTab, name: &str) -> bool {
    let Some(path) = tab.profiles_dir.as_deref().and_then(|dir| profile_path(dir, name)) else {
        return false;
    };
    if !session.open(&path) {
        return false;
    }
    session.set_active_profile(Some(name.to_owned()));
    tab.remember = Some(Remembered::Profile {
        name: name.to_owned(),
        path,
    });
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

/// Supprime le fichier du profil `name` (après confirmation). L'édition en cours reste à l'écran ; si c'était le
/// profil actif, elle n'a plus de fichier (le prochain `Ctrl+S` crée un nouveau profil, jamais l'ancien fichier).
pub(crate) fn delete_profile(session: &mut PrepSession, tab: &mut SettingsTab, name: &str) {
    let Some(path) = tab.profiles_dir.as_deref().and_then(|dir| profile_path(dir, name)) else {
        return;
    };
    match std::fs::remove_file(&path) {
        Ok(()) => {
            if session.active_profile() == Some(name) {
                session.set_active_profile(None);
                session.detach_file();
                tab.remember = Some(Remembered::NoProfile);
                session.status = Status::Info(format!(
                    "Profil « {name} » supprimé : la soirée affichée n'est plus enregistrée sur le disque (Ctrl+S \
                     crée un nouveau profil)."
                ));
            } else {
                session.status = Status::Info(format!("Profil « {name} » supprimé"));
            }
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
