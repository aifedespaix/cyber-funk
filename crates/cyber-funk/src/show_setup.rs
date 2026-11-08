//! Chargement du show et des profils de contrôle demandés par la ligne de commande.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use cf_render::{ControllerLibrary, ControlsConfig, ControlsPlugin, MidiChoice, ShowPlugin};
use cf_show::drop::DropTitles;
use cf_show::load::{LoadedShow, LookLibrary};
use cf_show::persist::OperatorPrefs;

use crate::cli::Cli;

/// Graine de l'aléatoire du spectacle quand rien d'autre ne la fixe : un même show rejoué donne la même conduite.
const DEFAULT_SEED: u64 = 0xC1BE_4F00;

/// Le plugin du spectacle : le show de `--show`, ses looks et ses cartons-titres, ou rien sans `--show`.
///
/// # Errors
///
/// Le show, la bibliothèque de looks ou le fichier des titres sont illisibles ou invalides : on le dit au démarrage,
/// pas devant le public.
pub fn show_plugin(cli: &Cli) -> anyhow::Result<ShowPlugin> {
    let Some(path) = &cli.show else {
        tracing::info!("aucun show (--show) : les contrôles tournent, le spectacle attend");
        return Ok(ShowPlugin {
            show: None,
            titles: DropTitles::empty(),
            seed: DEFAULT_SEED,
            utc_offset_s: 0.0,
        });
    };
    // Un look corrompu ne doit pas empêcher d'ouvrir le show : il est écarté et dit au journal, les cues qui le
    // jouent retombent sur le look calme (skill `live-safety`).
    let (looks, rejected) = LookLibrary::load_dir_lenient(&cli.assets_dir.join("looks"))
        .with_context(|| format!("looks de {}", cli.assets_dir.display()))?;
    for error in &rejected {
        tracing::error!(%error, "look illisible écarté");
    }
    let event_text = std::fs::read_to_string(path).with_context(|| format!("show {}", path.display()))?;
    let mut event = cf_show::load::parse_event(&event_text, &path.display().to_string())
        .with_context(|| format!("show {}", path.display()))?;
    // Chaque DJ a ses couleurs : celles du fichier, sinon une palette de présélection jamais répétée d'un DJ au suivant.
    event.assign_dj_themes();
    let (show, substitutions) = LoadedShow::new_resilient(event, &looks, &path.display().to_string())
        .with_context(|| format!("show {}", path.display()))?;
    for s in &substitutions {
        tracing::error!(
            slot = s.slot,
            cue = s.cue,
            wanted = %s.wanted,
            "look introuvable : le cue joue le look calme"
        );
    }
    let titles_path = cli.assets_dir.join("titles/drop-titles.ron");
    let titles = if titles_path.is_file() {
        DropTitles::load(&titles_path).with_context(|| format!("titres {}", titles_path.display()))?
    } else {
        tracing::warn!(path = %titles_path.display(), "pas de fichier de titres : les drops sont sans carton");
        DropTitles::empty()
    };
    tracing::info!(show = %path.display(), looks = looks.len(), "show chargé");
    Ok(ShowPlugin {
        show: Some(show),
        titles,
        seed: DEFAULT_SEED,
        utc_offset_s: 0.0,
    })
}

/// Le show à jouer quand `--show` manque : la soirée retenue par l'opérateur (profil actif, sinon dernier fichier),
/// résolue comme l'éditeur la résout (`cf_operator::startup_event_path`). Rien avec `--show` (il garde la priorité),
/// en `--soak` ou `--no-operator` (comportement inchangé), ni sans réglages ou dossier de profils.
pub fn remembered_show(cli: &Cli, prefs: Option<&OperatorPrefs>, profiles: Option<&Path>) -> Option<PathBuf> {
    if cli.show.is_some() || cli.soak.is_some() || cli.no_operator {
        return None;
    }
    cf_operator::startup_event_path(prefs?, profiles?, None)
}

/// [`show_plugin`], en jouant `remembered` s'il y en a un (il devient le `--show` de la suite du démarrage). Un show
/// retenu qui ne se charge pas n'empêche jamais de démarrer : avertissement, puis « aucun show » comme avant. Tout se
/// lit ici, à l'ouverture (règle 8).
///
/// # Errors
///
/// Celles de [`show_plugin`] sans show retenu (un `--show` invalide reste une erreur de démarrage).
pub fn show_plugin_with_fallback(cli: &mut Cli, remembered: Option<PathBuf>) -> anyhow::Result<ShowPlugin> {
    let Some(path) = remembered else {
        return show_plugin(cli);
    };
    cli.show = Some(path);
    match show_plugin(cli) {
        Ok(plugin) => Ok(plugin),
        Err(error) => {
            tracing::warn!("soirée retenue non jouée : {error:#}");
            cli.show = None;
            show_plugin(cli)
        }
    }
}

/// Le plugin des contrôles : MIDI du système (sauf `--no-midi`) et profils de `assets/controllers`.
///
/// # Errors
///
/// Un profil du dossier est invalide.
pub fn controls_plugin(cli: &Cli) -> anyhow::Result<ControlsPlugin> {
    let dir = cli.assets_dir.join("controllers");
    Ok(ControlsPlugin::new(ControlsConfig {
        midi: if cli.no_midi {
            MidiChoice::Off
        } else {
            MidiChoice::System
        },
        library: library(&dir)?,
        // Hors du dossier des profils livrés (versionné) : ce que l'opérateur apprend lui appartient.
        user_profile_path: Some(cli.assets_dir.join("local/user-controls.ron")),
    }))
}

fn library(dir: &Path) -> anyhow::Result<ControllerLibrary> {
    if !dir.is_dir() {
        // Sans le dossier, seul le clavier par défaut (intégré) reste : le show démarre quand même.
        tracing::warn!(dir = %dir.display(), "pas de dossier de profils de contrôle");
        return Ok(ControllerLibrary::new());
    }
    ControllerLibrary::load_dir(dir).with_context(|| format!("profils de contrôle de {}", dir.display()))
}

#[cfg(test)]
mod tests {
    use clap::Parser as _;

    use super::*;

    fn cli(args: &[&str]) -> Cli {
        Cli::try_parse_from(std::iter::once("cyber-funk").chain(args.iter().copied())).expect("arguments valides")
    }

    fn assets() -> String {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets").to_owned()
    }

    #[test]
    fn le_show_de_demonstration_se_charge_avec_ses_profils() {
        let assets = assets();
        let show = format!("{assets}/shows/demo.ron");
        let cli = cli(&["--show", &show, "--assets-dir", &assets, "--no-midi"]);
        let plugin = show_plugin(&cli).expect("show de démonstration");
        assert!(plugin.show.is_some());
        assert!(!plugin.titles.is_empty());
        assert!(controls_plugin(&cli).is_ok());
    }

    #[test]
    fn sans_show_le_plugin_est_vide() {
        let plugin = show_plugin(&cli(&[])).expect("pas de show, pas d'erreur");
        assert!(plugin.show.is_none());
    }

    #[test]
    fn un_show_invalide_est_une_erreur_de_demarrage() {
        let assets = assets();
        let broken = format!("{assets}/README.md");
        let cli = cli(&["--show", &broken, "--assets-dir", &assets]);
        assert!(show_plugin(&cli).is_err());
    }

    #[test]
    fn startup_show_path_falls_back_when_profile_invalid() {
        let root = tempfile::tempdir().expect("dossier temporaire");
        let profiles = root.path().join("profiles");
        std::fs::create_dir(&profiles).expect("dossier");
        std::fs::write(profiles.join("Soirée.ron"), "(version: 1, cassé").expect("écriture");
        let assets = assets();
        let last = PathBuf::from(format!("{assets}/shows/demo.ron"));
        let prefs = OperatorPrefs {
            active_profile: Some("Soirée".to_owned()),
            last_event_path: Some(last.display().to_string()),
            ..OperatorPrefs::default()
        };
        let base = cli(&["--assets-dir", &assets, "--no-midi"]);
        // Profil illisible : on retombe sur le dernier fichier, joué comme un `--show`.
        assert_eq!(
            remembered_show(&base, Some(&prefs), Some(&profiles)),
            Some(last.clone())
        );
        let mut played = cli(&["--assets-dir", &assets, "--no-midi"]);
        let plugin = show_plugin_with_fallback(&mut played, Some(last.clone())).expect("show retenu");
        assert!(plugin.show.is_some());
        assert_eq!(played.show, Some(last));

        // `--show`, `--soak` et `--no-operator` gardent leur comportement.
        let explicit = cli(&["--show", "x.ron"]);
        assert_eq!(remembered_show(&explicit, Some(&prefs), Some(&profiles)), None);
        assert_eq!(
            remembered_show(&cli(&["--no-operator"]), Some(&prefs), Some(&profiles)),
            None
        );
        assert_eq!(remembered_show(&base, None, Some(&profiles)), None);

        // Un show retenu qui ne se charge pas : aucun show, jamais d'erreur de démarrage.
        let mut broken = cli(&["--assets-dir", &assets]);
        let plugin = show_plugin_with_fallback(&mut broken, Some(PathBuf::from(format!("{assets}/README.md"))))
            .expect("le démarrage continue");
        assert!(plugin.show.is_none());
        assert_eq!(broken.show, None);
    }
}
