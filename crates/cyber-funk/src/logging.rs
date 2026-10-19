//! Journaux : `tracing` vers la console, et vers un fichier tournant en release.
//!
//! On installe notre propre abonné et on désactive le `LogPlugin` de Bevy : c'est le seul moyen d'ajouter un fichier
//! dont le dossier dépend de la configuration (le `custom_layer` de Bevy est un pointeur de fonction, sans état).

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::{EnvFilter, Layer as _, fmt};

use crate::cli::LogLevel;

/// Nom du dossier de l'application dans le dossier de données de l'utilisateur.
const APP_DIR: &str = "cyber-funk";
/// Préfixe des fichiers journaux : `cyber-funk.2026-10-19.log`.
const FILE_PREFIX: &str = "cyber-funk";
/// Nombre de jours de journaux conservés : de quoi relire une semaine de soirées, sans remplir le disque.
const MAX_LOG_FILES: usize = 14;
/// Niveau par défaut quand ni `--log` ni `RUST_LOG` ne sont donnés.
const DEFAULT_LEVEL: &str = "info";
/// Bibliothèques très bavardes, bridées quel que soit le niveau demandé (comme le fait Bevy par défaut).
/// `RUST_LOG` permet de les rouvrir au besoin.
const NOISY_CRATES: &str = "wgpu=error,naga=warn";

/// Où et comment journaliser.
#[derive(Debug, Default)]
pub struct LogConfig {
    /// Niveau demandé par `--log`.
    pub level: Option<LogLevel>,
    /// Dossier du fichier journal ; `None` pour ne journaliser que sur la console.
    pub dir: Option<PathBuf>,
}

/// Garde du fichier journal : à conserver jusqu'à la fin de `main`, sinon les dernières lignes sont perdues.
#[must_use = "la garde doit vivre jusqu'à la fin du programme pour vider le fichier journal"]
#[derive(Debug)]
pub struct LogGuard {
    /// Jamais lue : seule sa destruction compte (elle vide le tampon d'écriture).
    _worker: Option<WorkerGuard>,
}

/// Dossier des journaux en release : `<dossier de données>/cyber-funk/logs`.
///
/// `None` quand le système n'a pas de dossier de données (rare) : on journalise alors sur la console seulement.
#[must_use]
pub fn default_log_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join(APP_DIR).join("logs"))
}

/// Construit le filtre. Priorité : `--log`, puis `RUST_LOG`, puis le niveau par défaut.
///
/// # Errors
/// Si `RUST_LOG` n'est pas une directive de filtre valide.
pub fn build_filter(level: Option<LogLevel>, rust_log: Option<&str>) -> anyhow::Result<EnvFilter> {
    if let Some(level) = level {
        let directives = format!("{},{NOISY_CRATES}", level.as_filter_str());
        return EnvFilter::try_new(directives).context("construction du filtre de --log");
    }
    match rust_log.filter(|value| !value.trim().is_empty()) {
        Some(value) => EnvFilter::try_new(value).with_context(|| format!("RUST_LOG invalide : {value:?}")),
        None => EnvFilter::try_new(format!("{DEFAULT_LEVEL},{NOISY_CRATES}")).context("filtre par défaut"),
    }
}

/// Ouvre le fichier journal tournant (un fichier par jour) dans `dir`, écrit sur un thread dédié.
///
/// L'écriture ne bloque jamais le rendu : si le disque ne suit pas, des lignes sont abandonnées plutôt que de figer
/// l'image.
///
/// # Errors
/// Si le dossier ne peut pas être créé ou le fichier ouvert.
pub fn open_file_writer(dir: &Path) -> anyhow::Result<(NonBlocking, WorkerGuard)> {
    std::fs::create_dir_all(dir).with_context(|| format!("création du dossier {}", dir.display()))?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(FILE_PREFIX)
        .filename_suffix("log")
        .max_log_files(MAX_LOG_FILES)
        .build(dir)
        .with_context(|| format!("ouverture du journal dans {}", dir.display()))?;
    Ok(tracing_appender::non_blocking(appender))
}

/// Installe l'abonné global `tracing` (et le pont depuis la crate `log`, utilisée par `wgpu`).
///
/// # Errors
/// Si `RUST_LOG` est invalide ou si un abonné est déjà installé. Un fichier journal impossible à ouvrir n'est pas
/// une erreur : on continue sur la console et on le signale.
pub fn init(config: &LogConfig) -> anyhow::Result<LogGuard> {
    let filter = build_filter(config.level, std::env::var("RUST_LOG").ok().as_deref())?;

    // Un journal de fichier manquant ne doit pas empêcher de jouer : on garde l'erreur pour la journaliser une
    // fois l'abonné installé.
    let file = config.dir.as_deref().map(|dir| (dir, open_file_writer(dir)));
    let (file_layer, guard, file_error) = match file {
        Some((_, Ok((writer, guard)))) => (
            Some(fmt::layer().with_ansi(false).with_writer(writer).boxed()),
            Some(guard),
            None,
        ),
        Some((dir, Err(error))) => (None, None, Some((dir, error))),
        None => (None, None, None),
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(file_layer)
        .try_init()
        .context("installation de l'abonné tracing")?;

    install_panic_hook();
    if let Some((dir, error)) = file_error {
        tracing::warn!(dir = %dir.display(), "journal fichier indisponible, console seule : {error:#}");
    } else if let Some(dir) = &config.dir {
        tracing::info!(dir = %dir.display(), "journal fichier actif");
    }
    Ok(LogGuard { _worker: guard })
}

/// Consigne les paniques dans le journal avant le message habituel : en release la console est souvent invisible.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("panique : {info}");
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_niveau_de_la_ligne_de_commande_l_emporte_sur_rust_log() {
        let filter = build_filter(Some(LogLevel::Debug), Some("error")).unwrap().to_string();
        assert!(filter.split(',').any(|directive| directive == "debug"), "{filter}");
        assert!(!filter.split(',').any(|directive| directive == "error"), "RUST_LOG ignoré : {filter}");
    }

    #[test]
    fn rust_log_l_emporte_sur_le_defaut() {
        let filter = build_filter(None, Some("cf_render=trace")).unwrap().to_string();
        assert_eq!(filter, "cf_render=trace");
    }

    #[test]
    fn sans_rien_on_journalise_en_info_et_on_bride_wgpu() {
        for empty in [None, Some(""), Some("  ")] {
            let filter = build_filter(None, empty).unwrap().to_string();
            assert!(filter.contains("info") && filter.contains("wgpu=error"), "{filter}");
        }
    }

    #[test]
    fn un_rust_log_invalide_est_une_erreur_explicite() {
        let error = build_filter(None, Some("info,=="));
        assert!(error.is_err());
    }

    #[test]
    fn le_fichier_journal_recoit_les_lignes() {
        let dir = tempfile::tempdir().unwrap();
        let (writer, guard) = open_file_writer(&dir.path().join("logs")).unwrap();
        let subscriber = tracing_subscriber::registry().with(fmt::layer().with_ansi(false).with_writer(writer));
        tracing::subscriber::with_default(subscriber, || tracing::info!("bonjour le journal"));
        drop(guard); // vide le tampon du thread d'écriture

        let files: Vec<_> = std::fs::read_dir(dir.path().join("logs")).unwrap().collect();
        assert_eq!(files.len(), 1, "un fichier par jour");
        let path = files[0].as_ref().unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("cyber-funk.") && name.ends_with(".log"), "{name}");
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("bonjour le journal"), "{content}");
        assert!(!content.contains('\u{1b}'), "pas de séquences ANSI dans le fichier");
    }
}
