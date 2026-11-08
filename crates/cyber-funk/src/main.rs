//! cyber-funk : visuels 3D temps réel pour soirées DJ.
//!
//! Le binaire ne fait qu'assembler les plugins : il lit la ligne de commande, installe les journaux, puis règle les
//! deux fenêtres (projecteur et opérateur).

mod assets;
mod cli;
mod icon;
mod logging;
mod preload;
mod projector;
mod show_setup;
mod soak;
mod update;
mod wiring;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::ensure;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::winit::WinitSettings;
use cf_cinema::CinemaPlugin;
use cf_operator::{OperatorPlugin, PrepConfig, QuitGuardPlugin};
use cf_render::{
    ActiveEventsPlugin, AudioConfig, AudioSource, CfAudioPlugin, ControlsPlugin, CorePlugin, CullingPlugin,
    DiagnosticsPlugin, LayersPlugin, OutputChoice, PalettePlugin, PostFxPlugin, QualityPlugin, SettingsPlugin,
    ShowPlugin, TitlesPlugin, WarmupPlugin, settings,
};
use cf_show::persist::AppSettings;
use cf_world_neon::{CarPlugin, NeonCinemaPlugin, NeonWorldPlugin, PilotPlugin, TrafficPlugin};
use clap::Parser as _;

use crate::cli::Cli;
use crate::logging::LogConfig;
use crate::preload::PreloadPlan;
use crate::projector::{ProjectorConfig, ProjectorPlugin};

/// Show demandé par `--show`. Le chargement arrive avec `cf-show` (J2) ; en attendant la ressource porte le chemin.
#[derive(Resource, Debug, Clone)]
pub struct ShowRequest(pub Option<PathBuf>);

fn main() -> anyhow::Result<ExitCode> {
    let mut cli = Cli::parse();

    // Fichier journal en release seulement : en développement la console suffit et on évite d'encombrer le disque.
    let log_dir = if cfg!(debug_assertions) {
        None
    } else {
        logging::default_log_dir()
    };
    // Journal de session structuré (relu par la skill `debug-logs`). Un échec n'empêche pas de jouer.
    let session = open_session_log();
    let _log_guard = logging::init(
        &LogConfig {
            level: cli.log,
            dir: log_dir,
        },
        session.as_ref().map(|(recorder, _)| recorder.clone()),
    )?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "cyber-funk démarre");

    if let Some(show) = &cli.show {
        ensure!(show.is_file(), "le fichier de show {} n'existe pas", show.display());
    }
    let settings = load_settings(&cli);
    // Sans `--show`, la soirée retenue (profil actif, sinon dernier fichier) est jouée ; illisible, on démarre sans.
    let remembered = show_setup::remembered_show(
        &cli,
        settings.as_ref().map(|plugin| &plugin.initial.operator),
        profiles_dir().as_deref(),
    );
    let show = show_setup::show_plugin_with_fallback(&mut cli, remembered)?;
    let controls = show_setup::controls_plugin(&cli)?;
    let preload = PreloadPlan::new(&cli, show.show.as_ref());

    let mut app = build_app(&cli, controls, show, preload, settings);
    if let Some((recorder, _)) = &session {
        app.add_plugins(cf_render::DebugLogPlugin {
            recorder: recorder.clone(),
        })
        // Le journal en direct de l'onglet Statistiques : la même poignée que celle que remplit l'enregistreur. Sans
        // enregistreur (disque refusé), la ressource manque et l'onglet dit « Journal indisponible ».
        .insert_resource(cf_operator::OperatorLog(recorder.log_buffer()));
    }
    let exit = app.run();
    tracing::info!(?exit, "cyber-funk s'arrête");
    Ok(exit_code(&exit))
}

/// Dossier racine des journaux de session : `./logs` en développement (ignoré par git), le dossier de données en
/// release.
fn session_log_root() -> PathBuf {
    if cfg!(debug_assertions) {
        PathBuf::from("logs")
    } else {
        dirs::data_dir().map_or_else(|| PathBuf::from("logs"), |dir| dir.join("cyber-funk").join("sessions"))
    }
}

/// Ouvre le journal de session ; `None` (et un message sur la console) si le disque refuse.
// Les journaux `tracing` ne sont pas encore installés à ce stade : la console est le seul recours.
#[allow(clippy::print_stderr)]
fn open_session_log() -> Option<(cf_debuglog::Recorder, cf_debuglog::Guard)> {
    let options = cf_debuglog::StartOptions {
        args: std::env::args().skip(1).collect(),
        ..cf_debuglog::StartOptions::default()
    };
    match cf_debuglog::Recorder::start(&session_log_root(), &options) {
        Ok(session) => Some(session),
        Err(error) => {
            eprintln!("journal de session indisponible : {error}");
            None
        }
    }
}

/// Assemble l'application : les plugins de cyber-funk, les fenêtres et la scène de test.
fn build_app(
    cli: &Cli,
    controls: ControlsPlugin,
    show: ShowPlugin,
    preload: PreloadPlan,
    settings: Option<SettingsPlugin>,
) -> App {
    // Écran du projecteur : la ligne de commande, sinon celui mémorisé par l'opérateur, sinon l'écran principal.
    let projector = ProjectorConfig::new(cli, settings.as_ref().map(|plugin| &plugin.initial.operator));

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
        // Rendu : les couches et leur compositeur, le culling, puis le post-process (qui équipe le compositeur), les
        // palettes et la qualité adaptative (qui règle l'échelle de rendu et le bloom). Tous en `CfSet::Present`
        // ou `Simulate` : l'ordre d'ajout ne change pas celui de l'image.
        LayersPlugin::default(),
        CullingPlugin::default(),
        PostFxPlugin,
        PalettePlugin,
        QualityPlugin,
        // Les cartons-titres (niveau, DJ, drop) sur la surcouche 2D.
        TitlesPlugin,
        // Registre des événements du monde en cours et mode manuel.
        ActiveEventsPlugin,
    ))
    .add_plugins((
        DiagnosticsPlugin::default(),
        CfAudioPlugin::new(audio_config(cli, settings.as_ref().map(|plugin| &plugin.initial))),
        // Après l'audio (`AudioPump`), avant le spectacle : les actions de l'image sont prêtes quand le chef les lit.
        controls,
        show,
        ProjectorPlugin(projector),
        CinemaPlugin::default(),
        // Le monde seul, puis la voiture et son pilote, puis le branchement sur le réalisateur (qui remplace la caméra
        // de poursuite provisoire : un seul écrivain de `CameraPose`).
        NeonWorldPlugin::embedded(),
        // La voiture se lit dans le dossier des ressources résolu, pas dans `assets/` du dossier de travail.
        CarPlugin {
            assets_dir: cli.assets_dir.clone(),
            variant: settings.as_ref().map_or_else(Default::default, |plugin| plugin.initial.car.into()),
            ..CarPlugin::default()
        },
        PilotPlugin::default(),
        // Le trafic : lit ses modèles dans le même dossier des ressources, écrit ce que le pilote doit voir avant lui.
        TrafficPlugin {
            assets_dir: cli.assets_dir.clone(),
            ..TrafficPlugin::default()
        },
        NeonCinemaPlugin,
        wiring::WiringPlugin,
        icon::WindowIconPlugin,
    ));

    app.add_systems(Startup, preload::start_preload);

    // Les réglages retenus d'un lancement à l'autre (audio, qualité, voiture, panneaux).
    if let Some(plugin) = settings {
        app.add_plugins(plugin);
    }

    if let Some(hours) = cli.soak {
        app.add_plugins(soak::SoakPlugin {
            hours,
            csv: cli.soak_csv.clone(),
        });
    }

    if cli.no_operator {
        // Sans fenêtre opérateur, pas d'interface pour confirmer : double fermeture à la place du dialogue.
        app.add_plugins(QuitGuardPlugin::double_close());
    } else {
        app.insert_resource(PrepConfig {
            assets_dir: cli.assets_dir.clone(),
            show_path: cli.show.clone(),
            // En `--soak`, ni réglages ni profils : une mesure ne touche pas aux soirées de l'opérateur.
            profiles_dir: if cli.soak.is_some() { None } else { profiles_dir() },
        })
        .add_plugins((OperatorPlugin, update::UpdatePlugin));
        // Le bouton Démarrer / Arrêter de l'opérateur pilote la séquence 3D. Sans opérateur, ou en `--soak` (une mesure
        // doit jouer d'emblée), personne ne peut l'appuyer : le show joue dès l'ouverture, comme avant.
        if cli.soak.is_none() {
            app.add_plugins(cf_world_neon::run::RunPlugin);
        }
    }
    app
}

/// Réglages audio tirés de la ligne de commande.
fn audio_config(cli: &Cli, settings: Option<&AppSettings>) -> AudioConfig {
    let mut config = AudioConfig {
        initial_source: AudioSource::Demo,
        music_dir: cli.music_dir.clone(),
        latency_ms: cli.latency_ms,
        output: if cli.null_audio_output {
            OutputChoice::Null { speed: 1.0 }
        } else {
            OutputChoice::Default
        },
        // La musique démarre avec `Démarrer` (séquence de démarrage). Sans opérateur ou en `--soak`, rien ne la pilote :
        // elle joue d'emblée.
        autoplay: cli.soak.is_some() || cli.no_operator,
        ..AudioConfig::default()
    };
    // Les réglages sauvegardés donnent la source et les périphériques ; la ligne de commande garde le dernier mot.
    if let Some(settings) = settings {
        settings::apply_audio(settings, &mut config, cli.audio_source.map(AudioSource::from));
    } else if let Some(source) = cli.audio_source {
        config.initial_source = source.into();
    }
    config
}

/// Relit les réglages sauvegardés. Sans dossier de configuration, ou en `--soak` (une mesure ne doit ni lire ni écrire
/// les réglages de l'opérateur), rien n'est retenu.
fn load_settings(cli: &Cli) -> Option<SettingsPlugin> {
    if cli.soak.is_some() {
        return None;
    }
    let path = dirs::config_dir()?.join("cyber-funk").join("settings.ron");
    let (initial, problem) = AppSettings::load_reporting(&path);
    if let Some(problem) = problem {
        tracing::warn!("{problem}");
    }
    Some(SettingsPlugin { path, initial })
}

/// Dossier des profils de soirée de l'onglet Réglages : `<config>/cyber-funk/profiles` (un fichier d'événement RON
/// par profil). `None` sans dossier de configuration : l'opérateur enregistre alors dans un fichier choisi.
fn profiles_dir() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("cyber-funk").join("profiles"))
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
