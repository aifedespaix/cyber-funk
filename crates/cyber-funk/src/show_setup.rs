//! Chargement du show et des profils de contrôle demandés par la ligne de commande.

use std::path::Path;

use anyhow::Context as _;
use cf_render::{ControllerLibrary, ControlsConfig, ControlsPlugin, MidiChoice, ShowPlugin};
use cf_show::drop::DropTitles;
use cf_show::load::{LoadedShow, LookLibrary};

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
    let looks = LookLibrary::load_dir(&cli.assets_dir.join("looks"))
        .with_context(|| format!("looks de {}", cli.assets_dir.display()))?;
    let show = LoadedShow::load(path, &looks).with_context(|| format!("show {}", path.display()))?;
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
}
