//! cyber-funk : visuels 3D temps réel pour soirées DJ.
//!
//! Le binaire ne fait qu'assembler les plugins : il lit la ligne de commande, installe les journaux, puis règle les
//! deux fenêtres (projecteur et opérateur).

mod cli;
mod logging;
mod projector;
mod test_scene;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::ensure;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::winit::WinitSettings;
use cf_cinema::CinemaPlugin;
use cf_operator::{OperatorPlugin, QuitGuardPlugin};
use cf_render::{CorePlugin, DiagnosticsPlugin};
use cf_world_neon::NeonWorldPlugin;
use clap::Parser as _;

use crate::cli::Cli;
use crate::logging::LogConfig;
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
        tracing::info!(show = %show.display(), "show demandé (chargement prévu en J2)");
    }

    let exit = build_app(&cli).run();
    tracing::info!(?exit, "cyber-funk s'arrête");
    Ok(exit_code(&exit))
}

/// Assemble l'application : les plugins de cyber-funk, les fenêtres et la scène de test.
fn build_app(cli: &Cli) -> App {
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
    .add_plugins((
        CorePlugin,
        DiagnosticsPlugin::default(),
        ProjectorPlugin(projector),
        CinemaPlugin,
        NeonWorldPlugin,
        TestScenePlugin,
    ));

    if cli.no_operator {
        // Sans fenêtre opérateur, pas d'interface pour confirmer : double fermeture à la place du dialogue.
        app.add_plugins(QuitGuardPlugin::double_close());
    } else {
        app.add_plugins(OperatorPlugin);
    }
    app
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
