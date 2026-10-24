//! cyber-funk : visuels 3D temps réel pour soirées DJ.
//!
//! Le binaire ne fait qu'assembler les plugins : il lit la ligne de commande, installe les journaux, puis règle les
//! deux fenêtres (projecteur et opérateur).

mod cli;
mod logging;
mod preload;
mod projector;
mod show_setup;
mod test_scene;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::ensure;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::winit::WinitSettings;
use cf_cinema::CinemaPlugin;
use cf_operator::{OperatorPlugin, QuitGuardPlugin};
use cf_render::{
    AudioConfig, CfAudioPlugin, ControlsPlugin, CorePlugin, DiagnosticsPlugin, OutputChoice, ShowPlugin, WarmupPlugin,
};
use cf_world_neon::NeonWorldPlugin;
use clap::Parser as _;

use crate::cli::Cli;
use crate::logging::LogConfig;
use crate::preload::PreloadPlan;
use crate::projector::{ProjectorConfig, ProjectorPlugin};
use crate::test_scene::TestScenePlugin;

/// Show demandé par `--show`. Le chargement arrive avec `cf-show` (J2) ; en attendant la ressource porte le chemin.
#[derive(Resource, Debug, Clone)]
pub struct ShowRequest(pub Option<PathBuf>);

fn main() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();

    // Fichier journal en release seulement : en développement la console suffit et on évite d'encombrer le disque.
    let log_dir = if cfg!(debug_assertions) {
        None
    } else {
        logging::default_log_dir()
    };
    let _log_guard = logging::init(&LogConfig {
        level: cli.log,
        dir: log_dir,
    })?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "cyber-funk démarre");

    if let Some(show) = &cli.show {
        ensure!(show.is_file(), "le fichier de show {} n'existe pas", show.display());
    }
    let show = show_setup::show_plugin(&cli)?;
    let controls = show_setup::controls_plugin(&cli)?;
    let preload = PreloadPlan::new(&cli, show.show.as_ref());

    let exit = build_app(&cli, controls, show, preload).run();
    tracing::info!(?exit, "cyber-funk s'arrête");
    Ok(exit_code(&exit))
}

/// Assemble l'application : les plugins de cyber-funk, les fenêtres et la scène de test.
fn build_app(cli: &Cli, controls: ControlsPlugin, show: ShowPlugin, preload: PreloadPlan) -> App {
    let projector = ProjectorConfig {
        monitor: cli.projector_monitor.clone().unwrap_or_default(),
        windowed: cli.windowed,
    };

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            // Nos journaux sont installés dans `main` (voir `logging`), avant la création de l'application.
            .disable::<LogPlugin>()
            .set(projector::window_plugin(&projector)),
    )
    // Le projecteur doit continuer à tourner quand l'opérateur a le focus : pas de mode « économie d'énergie ».
    .insert_resource(WinitSettings::game())
    .insert_resource(ClearColor(Color::BLACK))
    .insert_resource(ShowRequest(cli.show.clone()))
    .insert_resource(preload)
    .add_plugins((
        CorePlugin,
        // Avant les mondes : états `Loading` → `Ready`, préchauffe des pipelines, détecteur de compilations tardives.
        WarmupPlugin,
        DiagnosticsPlugin::default(),
        CfAudioPlugin::new(audio_config(cli)),
        // Après l'audio (`AudioPump`), avant le spectacle : les actions de l'image sont prêtes quand le chef les lit.
        controls,
        show,
        ProjectorPlugin(projector),
        CinemaPlugin,
        NeonWorldPlugin::default(),
        TestScenePlugin,
    ));

    app.add_systems(Startup, preload::start_preload);

    if cli.no_operator {
        // Sans fenêtre opérateur, pas d'interface pour confirmer : double fermeture à la place du dialogue.
        app.add_plugins(QuitGuardPlugin::double_close());
    } else {
        app.add_plugins(OperatorPlugin);
    }
    app
}

/// Réglages audio tirés de la ligne de commande.
fn audio_config(cli: &Cli) -> AudioConfig {
    AudioConfig {
        initial_source: cli.audio_source.into(),
        music_dir: cli.music_dir.clone(),
        latency_ms: cli.latency_ms,
        output: if cli.null_audio_output {
            OutputChoice::Null { speed: 1.0 }
        } else {
            OutputChoice::Default
        },
        ..AudioConfig::default()
    }
}

/// Traduit la sortie de Bevy en code de retour du processus.
fn exit_code(exit: &AppExit) -> ExitCode {
    match exit {
        AppExit::Success => ExitCode::SUCCESS,
        AppExit::Error(code) => ExitCode::from(code.get()),
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use super::*;

    #[test]
    fn le_code_de_sortie_reprend_celui_de_bevy() {
        assert_eq!(exit_code(&AppExit::Success), ExitCode::SUCCESS);
        let failure = AppExit::Error(NonZeroU8::new(3).unwrap());
        assert_eq!(exit_code(&failure), ExitCode::from(3));
    }
}
