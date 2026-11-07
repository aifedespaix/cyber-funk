//! Reprise de la soirée au relancement : quel fichier d'événement ouvrir, et retenir le profil actif et le dernier
//! fichier dans les réglages (`OperatorPrefs`).
//!
//! ```text
//! priorité au démarrage : --show  ▶  profil actif (s'il existe et se lit)  ▶  last_event_path (idem)  ▶  neuf
//! remember_evening : geste sur un profil (SettingsTab::remember) ─▶ Settings.value.operator (sur vrai changement)
//! ```
//!
//! Un fichier manquant ou illisible passe au candidat suivant avec un avertissement dans le journal : il n'empêche
//! jamais de démarrer.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use cf_render::Settings;
use cf_show::persist::OperatorPrefs;

use super::profiles::{profile_name_of, profile_path};
use super::{Remembered, SettingsTab};
use crate::prepare::{PrepConfig, PrepSession};

/// Les fichiers à essayer au démarrage, dans l'ordre (sans toucher au disque). Un `--show` explicite est seul : on
/// n'ouvre jamais en douce autre chose que ce qui a été demandé.
fn startup_candidates(prefs: &OperatorPrefs, profiles: Option<&Path>, cli_show: Option<&Path>) -> Vec<PathBuf> {
    if let Some(cli) = cli_show {
        return vec![cli.to_path_buf()];
    }
    let mut candidates = Vec::with_capacity(2);
    if let (Some(dir), Some(name)) = (profiles, prefs.active_profile.as_deref()) {
        if let Some(path) = profile_path(dir, name) {
            candidates.push(path);
        } else {
            tracing::warn!(name, "profil actif au nom invalide : ignoré");
        }
    }
    if let Some(last) = prefs.last_event_path.as_deref().filter(|p| !p.trim().is_empty()) {
        let last = PathBuf::from(last);
        if !candidates.contains(&last) {
            candidates.push(last);
        }
    }
    candidates
}

/// L'événement à ouvrir au démarrage : `--show`, puis le profil actif, puis `last_event_path`, chacun seulement s'il
/// existe et se lit (RON d'une version connue) ; `None` : événement neuf. Le `--show` est pris tel quel (le binaire
/// l'a déjà vérifié, et un show demandé explicitement ne se remplace pas en douce).
#[must_use]
pub fn startup_event_path(prefs: &OperatorPrefs, profiles: &Path, cli_show: Option<&Path>) -> Option<PathBuf> {
    pick(prefs, Some(profiles), cli_show)
}

/// [`startup_event_path`], le dossier des profils pouvant manquer (pas de dossier de configuration).
fn pick(prefs: &OperatorPrefs, profiles: Option<&Path>, cli_show: Option<&Path>) -> Option<PathBuf> {
    if let Some(cli) = cli_show {
        return Some(cli.to_path_buf());
    }
    startup_candidates(prefs, profiles, None)
        .into_iter()
        .find(|path| readable_event(path))
}

/// Le fichier existe-t-il et se lit-il comme un événement ? Sinon, avertissement dans le journal.
pub(crate) fn readable_event(path: &Path) -> bool {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            tracing::warn!(path = %path.display(), "soirée non reprise, fichier illisible : {e}");
            return false;
        }
    };
    match cf_show::load::parse_event(&text, &path.display().to_string()) {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!(path = %path.display(), "soirée non reprise, fichier invalide : {e}");
            false
        }
    }
}

/// Crée la session de préparation au démarrage (lecture de fichiers : permise, le show n'est pas encore en cours) et
/// l'état de l'onglet Réglages. Un fichier qui se lit mais que la validation refuse (look introuvable…) laisse un
/// événement neuf, avec la raison affichée dans la barre d'état.
pub(crate) fn init_session(config: Res<PrepConfig>, settings: Option<Res<Settings>>, mut commands: Commands) {
    let prefs = settings.map(|s| s.value.operator.clone()).unwrap_or_default();
    let profiles = config.profiles_dir.as_deref();
    let cli = config.show_path.as_deref();
    let chosen = match profiles {
        Some(dir) => startup_event_path(&prefs, dir, cli),
        None => pick(&prefs, None, cli),
    };

    let Some(mut session) = PrepSession::start_from(&config, chosen.as_slice()) else {
        tracing::error!("préparation indisponible : impossible de créer un événement neuf");
        return;
    };
    let profile = profiles
        .zip(session.doc_path())
        .and_then(|(dir, path)| profile_name_of(dir, path));
    session.set_active_profile(profile);
    commands.insert_resource(SettingsTab::new(config.profiles_dir.clone()));
    commands.insert_resource(session);
}

/// Recopie dans les réglages ce que l'opérateur vient de faire sur les profils (`SettingsTab::remember`). Rien
/// d'autre n'y écrit : un repli au démarrage ou un lancement `--show` ne fait jamais oublier le profil retenu.
/// `Settings` n'est touché (donc réécrit sur disque) que sur un vrai changement.
pub(crate) fn remember_evening(tab: Option<ResMut<SettingsTab>>, settings: Option<ResMut<Settings>>) {
    let (Some(mut tab), Some(mut settings)) = (tab, settings) else {
        return;
    };
    if tab.remember.is_none() {
        return;
    }
    if let Some(remembered) = tab.remember.take() {
        apply_remembered(&mut settings, &remembered);
    }
}

/// Applique un geste retenu aux préférences, seulement s'il change quelque chose.
pub(crate) fn apply_remembered(settings: &mut Settings, remembered: &Remembered) {
    let mut prefs = settings.value.operator.clone();
    remembered.apply(&mut prefs);
    if prefs != settings.value.operator {
        settings.value.operator = prefs;
    }
}

/// À la fermeture confirmée : les réglages sont écrits tout de suite, sans attendre le délai de calme de l'écriture
/// automatique (sinon un « Enregistrer et quitter » perdrait le profil actif). Une erreur est seulement journalisée.
pub(crate) fn flush_on_quit(settings: &mut Settings, tab: Option<&mut SettingsTab>) {
    if let Some(remembered) = tab.and_then(|t| t.remember.take()) {
        apply_remembered(settings, &remembered);
    }
    if let Err(error) = settings.value.save(settings.path()) {
        tracing::warn!(%error, "réglages : écriture à la fermeture impossible");
    }
}
