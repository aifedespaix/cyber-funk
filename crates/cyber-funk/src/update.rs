//! Mise à jour automatique (J9-06) : vérifie au démarrage si une release plus récente existe sur GitHub, puis
//! l'installe à la demande de l'opérateur.
//!
//! Garde-fous (CLAUDE.md règles 3 et 8) : le réseau ne passe jamais par le thread principal ni par le thread audio,
//! la vérification a un délai maximal et échoue en silence, et l'installation n'est jamais lancée en vue Live.
//! Les mises à jour sont signées (clé publique dans `packaging/updater.pub`) : un fichier non signé est refusé.

use std::path::Path;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use bevy::prelude::*;
use cargo_packager_updater::semver::Version;
use cargo_packager_updater::{Config, Update, UpdaterBuilder};
use cf_operator::{InstallUpdate, OperatorMode, UpdateStatus};

/// Manifeste publié avec chaque release par `.github/workflows/release.yml`.
const MANIFEST_URL: &str = "https://github.com/aifedespaix/cyber-funk/releases/latest/download/latest.json";
/// Clé publique de signature (la clé privée vit dans les secrets GitHub).
const PUBLIC_KEY: &str = include_str!("../../../packaging/updater.pub");
/// Au-delà, on renonce : le réseau ne doit jamais retarder ni gêner l'application.
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
/// Le téléchargement d'un paquet de ~50 Mo peut être long sur une connexion lente.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

/// Vérifie les mises à jour au démarrage, seulement en release (en développement la version `0.1.0` serait toujours
/// « périmée »), sauf `CF_UPDATE_CHECK=1`. `CF_NO_UPDATE=1` la coupe partout.
pub struct UpdatePlugin;

impl Plugin for UpdatePlugin {
    fn build(&self, app: &mut App) {
        let (sender, events) = mpsc::channel();
        app.insert_resource(Worker {
            events: Mutex::new(events),
            sender,
            found: None,
        })
        .add_systems(Startup, start_check)
        .add_systems(Update, (poll_events, start_install));
    }
}

/// Ce que les threads réseau rapportent au thread principal.
enum Event {
    Found(Box<Update>),
    NoUpdate,
    CheckFailed(String),
    Installed(String),
    InstallFailed(String),
}

#[derive(Resource)]
struct Worker {
    events: Mutex<Receiver<Event>>,
    sender: Sender<Event>,
    found: Option<Update>,
}

fn enabled() -> bool {
    let on = |name: &str| std::env::var_os(name).is_some_and(|v| v == "1");
    !on("CF_NO_UPDATE") && (!cfg!(debug_assertions) || on("CF_UPDATE_CHECK"))
}

fn config() -> anyhow::Result<Config> {
    Ok(Config {
        endpoints: vec![MANIFEST_URL.parse()?],
        pubkey: PUBLIC_KEY.trim().to_owned(),
        ..Config::default()
    })
}

fn check(current: &Version) -> anyhow::Result<Option<Update>> {
    let updater = UpdaterBuilder::new(current.clone(), config()?)
        .timeout(CHECK_TIMEOUT)
        .build()?;
    Ok(updater.check()?)
}

fn start_check(worker: Res<Worker>) {
    if !enabled() {
        tracing::debug!("vérification des mises à jour désactivée");
        return;
    }
    let Ok(current) = env!("CARGO_PKG_VERSION").parse::<Version>() else {
        return;
    };
    let sender = worker.sender.clone();
    // Thread dédié : la requête HTTP est bloquante.
    let spawned = std::thread::Builder::new()
        .name("cf-update-check".into())
        .spawn(move || {
            let event = match check(&current) {
                Ok(Some(update)) => Event::Found(Box::new(update)),
                Ok(None) => Event::NoUpdate,
                Err(err) => Event::CheckFailed(format!("{err:#}")),
            };
            // Le récepteur n'existe plus si l'application s'arrête : sans importance.
            let _ = sender.send(event);
        });
    if let Err(err) = spawned {
        tracing::warn!(%err, "thread de vérification des mises à jour non démarré");
    }
}

fn poll_events(mut worker: ResMut<Worker>, mut status: ResMut<UpdateStatus>) {
    let events: Vec<Event> = match worker.events.lock() {
        Ok(events) => events.try_iter().collect(),
        Err(_) => return,
    };
    for event in events {
        match event {
            Event::Found(update) => {
                let version = update.version.clone();
                tracing::info!(%version, "mise à jour disponible");
                *status = if can_self_update() {
                    UpdateStatus::Available {
                        version,
                        notes: update.body.clone(),
                    }
                } else {
                    UpdateStatus::AvailableManual { version }
                };
                worker.found = Some(*update);
            }
            Event::NoUpdate => tracing::info!("cyber-funk est à jour"),
            // Pas de bandeau : ne pas avoir de réseau n'est pas un problème à signaler à l'opérateur.
            Event::CheckFailed(err) => tracing::warn!(%err, "vérification des mises à jour impossible"),
            Event::Installed(version) => *status = UpdateStatus::Installed { version },
            Event::InstallFailed(err) => {
                tracing::error!(%err, "installation de la mise à jour échouée");
                *status = UpdateStatus::Failed(err);
            }
        }
    }
}

fn start_install(
    mut requests: MessageReader<InstallUpdate>,
    mode: Option<Res<OperatorMode>>,
    mut worker: ResMut<Worker>,
    mut status: ResMut<UpdateStatus>,
) {
    if requests.read().count() == 0 {
        return;
    }
    // Jamais pendant un show : l'installateur ferme l'application (Windows) ou remplace son binaire.
    if mode.is_some_and(|m| *m == OperatorMode::Live) {
        return;
    }
    let UpdateStatus::Available { version, .. } = status.clone() else {
        return;
    };
    let Some(mut update) = worker.found.take() else {
        return;
    };
    update.timeout = Some(DOWNLOAD_TIMEOUT);
    *status = UpdateStatus::Installing {
        version: version.clone(),
    };
    let sender = worker.sender.clone();
    let spawned = std::thread::Builder::new()
        .name("cf-update-install".into())
        .spawn(move || {
            // Sous Windows, `install` lance l'installateur puis termine le processus : l'événement ne part pas.
            let event = match update.download().and_then(|bytes| update.install(bytes)) {
                Ok(()) => Event::Installed(version),
                Err(err) => Event::InstallFailed(err.to_string()),
            };
            let _ = sender.send(event);
        });
    if let Err(err) = spawned {
        *status = UpdateStatus::Failed(err.to_string());
    }
}

/// Cette installation peut-elle se remplacer elle-même ? Le paquet publié pour Linux n'est applicable qu'à un
/// `AppImage`, celui de macOS qu'à un `.app` ; lancé depuis les sources ou l'archive portable, on prévient seulement.
fn can_self_update() -> bool {
    let exe = std::env::current_exe().ok();
    self_updatable(exe.as_deref(), std::env::var_os("APPIMAGE").is_some())
}

fn self_updatable(exe: Option<&Path>, appimage: bool) -> bool {
    if cfg!(windows) {
        return true;
    }
    if cfg!(target_os = "macos") {
        return exe.is_some_and(|p| p.to_string_lossy().contains(".app/Contents/MacOS/"));
    }
    appimage
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_cle_publique_du_depot_est_valide() {
        // `build` décode la clé : une clé corrompue ou d'un autre format échouerait ici, pas chez l'utilisateur.
        let current = Version::new(0, 1, 0);
        assert!(UpdaterBuilder::new(current, config().unwrap()).build().is_ok());
    }

    #[test]
    fn le_manifeste_est_une_url_https() {
        assert_eq!(config().unwrap().endpoints[0].scheme(), "https");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn sous_linux_seul_un_appimage_se_remplace() {
        assert!(self_updatable(None, true));
        assert!(!self_updatable(None, false));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn sous_macos_seul_un_app_se_remplace() {
        let app = Path::new("/Applications/cyber-funk.app/Contents/MacOS/cyber-funk");
        assert!(self_updatable(Some(app), false));
        assert!(!self_updatable(
            Some(Path::new("/tmp/target/release/cyber-funk")),
            false
        ));
    }
}
