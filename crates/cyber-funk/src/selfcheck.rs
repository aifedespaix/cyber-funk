//! Mode `--self-check` : le garde-fou de la release.
//!
//! L'application est assemblée exactement comme pour un show (mêmes plugins, mêmes options), puis chaque système de
//! chaque ordonnancement est initialisé sans qu'aucune image ne soit jouée. Un conflit d'accès entre paramètres d'un
//! système (erreur B0002 de Bevy) ne se révèle sinon qu'à la première exécution de son ordonnancement, donc au
//! lancement chez l'utilisateur pour ceux qui attendent une fenêtre. Ici il fait échouer la release avant publication.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use bevy::app::PluginsState;
use bevy::prelude::*;

/// Délai accordé aux plugins pour finir leur préparation asynchrone (création du moteur de rendu).
const PLUGINS_TIMEOUT: Duration = Duration::from_secs(120);

/// Termine la construction de l'application, contrôle ses ordonnancements et rend le code de sortie du processus.
pub fn run(app: &mut App) -> ExitCode {
    // Ce que fait la boucle de `winit` avant la première image : attendre les plugins, puis `finish` et `cleanup`.
    let start = Instant::now();
    while app.plugins_state() == PluginsState::Adding {
        if start.elapsed() > PLUGINS_TIMEOUT {
            tracing::error!("autocontrôle : les plugins ne sont pas prêts après {PLUGINS_TIMEOUT:?}");
            return ExitCode::FAILURE;
        }
        bevy::tasks::tick_global_task_pools_on_main_thread();
        std::thread::sleep(Duration::from_millis(5));
    }
    app.finish();
    app.cleanup();

    let report = cf_render::check_schedules(app);
    for failure in &report.failures {
        tracing::error!("autocontrôle : {failure}");
    }
    if report.is_ok() {
        tracing::info!(
            schedules = report.checked,
            "autocontrôle : tous les systèmes s'initialisent"
        );
        ExitCode::SUCCESS
    } else {
        tracing::error!(failures = report.failures.len(), "autocontrôle : échec");
        ExitCode::FAILURE
    }
}
