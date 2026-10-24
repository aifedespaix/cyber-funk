//! Préchargement des fichiers que le show référence (J4-07) : musiques de démo, logos des DJ ; le show et ses looks sont
//! déjà chargés et validés avant le démarrage de l'application (voir `show_setup`).
//!
//! La voiture et les shaders se déclarent dans leurs plugins (`cf-world-neon`) : chaque monde connaît ses assets.

use std::path::PathBuf;

use bevy::prelude::*;
use cf_audio::playlist::scan_music_dir;
use cf_render::warmup::FileJobs;
use cf_render::LoadTracker;
use cf_show::load::LoadedShow;

use crate::cli::Cli;

/// Ce qu'il y a à précharger, décidé avant le démarrage de l'application.
#[derive(Resource, Debug, Clone, Default)]
pub struct PreloadPlan {
    /// Pistes de démo (`--music-dir` et son sous-dossier `local/`).
    pub music: Vec<PathBuf>,
    /// Logos des DJ référencés par l'événement (chemins sous `--assets-dir`).
    pub logos: Vec<PathBuf>,
    /// Description du show chargé, pour l'écran de progression (rien sans `--show`).
    pub show: Option<String>,
}

impl PreloadPlan {
    /// Établit le plan d'après la ligne de commande et le show chargé.
    #[must_use]
    pub fn new(cli: &Cli, show: Option<&LoadedShow>) -> Self {
        let music = match scan_music_dir(&cli.music_dir) {
            Ok(tracks) => tracks.into_iter().map(|track| track.path).collect(),
            Err(error) => {
                // Même dossier illisible que pour le lecteur : lui le signale à l'opérateur, ici on ne précharge rien.
                tracing::warn!(%error, "préchargement : dossier de pistes illisible");
                Vec::new()
            }
        };
        let (logos, show) = show.map_or_else(
            || (Vec::new(), None),
            |show| {
                let event = show.event();
                let logos = event
                    .slots
                    .iter()
                    .filter_map(|slot| slot.dj.logo.as_ref())
                    .map(|logo| cli.assets_dir.join(logo.as_str()))
                    .collect();
                let summary = format!("Show « {} » : {} créneau(x), {} look(s)", event.settings.name, event.slots.len(), show.looks().count());
                (logos, Some(summary))
            },
        );
        Self { music, logos, show }
    }
}

/// `Startup` : inscrit les étapes de chargement et lance la lecture des fichiers en tâche de fond.
pub fn start_preload(plan: Res<PreloadPlan>, mut tracker: ResMut<LoadTracker>, mut jobs: ResMut<FileJobs>) {
    if let Some(show) = &plan.show {
        tracker.register_done(show.clone());
    }
    jobs.spawn(&mut tracker, format!("Musiques ({})", plan.music.len()), plan.music.clone());
    if !plan.logos.is_empty() {
        jobs.spawn(&mut tracker, format!("Logos DJ ({})", plan.logos.len()), plan.logos.clone());
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser as _;

    use super::*;

    fn assets() -> String {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets").to_owned()
    }

    #[test]
    fn le_plan_liste_les_musiques_du_depot() {
        let assets = assets();
        let music = format!("{assets}/music");
        let cli = Cli::try_parse_from(["cyber-funk", "--music-dir", &music, "--assets-dir", &assets]).expect("arguments");
        let plan = PreloadPlan::new(&cli, None);
        assert!(!plan.music.is_empty(), "les pistes libres du dépôt sont listées");
        assert!(plan.show.is_none());
    }

    #[test]
    fn un_dossier_de_musiques_absent_ne_donne_rien_a_charger() {
        let cli = Cli::try_parse_from(["cyber-funk", "--music-dir", "/nulle/part"]).expect("arguments");
        assert!(PreloadPlan::new(&cli, None).music.is_empty());
    }
}
