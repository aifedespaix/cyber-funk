//! Les événements du trafic dans une vraie `App` Bevy, par le chemin de l'opérateur : une `Action::Trigger` entre par les
//! contrôles, le chef du spectacle la garde dans sa file d'ordres, et le système du trafic lance l'événement qu'elle
//! nomme. Les 23 événements des lots A (J7-04) et B (J7-05) sont déclenchés ainsi et vivent 60 s simulées.
//!
//! Sans rendu : le temps est manuel (60 i/s) et seule la partie logique des plugins tourne. Le dernier test mesure ce que
//! la ville montre (`FacadeLook` : éclair, pouls des fenêtres, panne de courant) sous une charge hostile : tout passe par
//! le limiteur de flashs du trafic (règle 7, WCAG 2.3.1).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::input::keyboard::KeyboardInput;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::WindowFocused;
use cf_analysis::Section;
use cf_control::{Action, Switch, TimedAction, TriggerId};
use cf_render::{
    AbsolutePosition, AudioFeatures, ControlAction, ControllerLibrary, ControlsConfig, ControlsPlugin, CorePlugin,
    CullingPlugin, MidiChoice, ShowPlugin, ShowRuntime, StreamFocus,
};
use cf_show::limiter::{MAX_FLASH_PEAK, MAX_FLASHES_PER_SECOND, MIN_FADE};
use cf_show::load::{LoadedShow, LookLibrary};
use cf_world_neon::FacadeLook;
use cf_world_neon::traffic::{EventName, TrafficDirector, TrafficPlugin, TrafficStats};

const FRAME_S: f32 = 1.0 / 60.0;
/// Vitesse de la voiture, m/s.
const SPEED: f64 = 60.0;
const EPS: f32 = 1e-3;

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// Une application avec le spectacle de démonstration, les contrôles et le trafic complet (les 23 événements).
fn app() -> App {
    let looks = LookLibrary::load_dir(&assets().join("looks")).expect("looks livrés");
    let show = LoadedShow::load(&assets().join("shows/demo.ron"), &looks).expect("show de démonstration");
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin, CorePlugin))
        .init_asset::<Mesh>()
        .init_resource::<AudioFeatures>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<FacadeLook>()
        .add_message::<KeyboardInput>()
        .add_message::<WindowFocused>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(f64::from(
            FRAME_S,
        ))))
        .add_plugins((
            CullingPlugin::default(),
            ControlsPlugin::new(ControlsConfig {
                user_profile_path: None,
                midi: MidiChoice::Off,
                library: ControllerLibrary::new(),
            }),
            ShowPlugin::new(show),
            TrafficPlugin {
                assets_dir: assets(),
                ..TrafficPlugin::default()
            },
        ));
    // La voiture roule à 60 m/s vers −z, à 400 km du départ.
    app.world_mut()
        .spawn((StreamFocus, AbsolutePosition(DVec3::new(0.0, 0.0, -400_000.0))));
    app.update();
    app
}

fn send(app: &mut App, action: Action) {
    let at = app.world().resource::<Time>().elapsed_secs_f64();
    app.world_mut()
        .write_message(ControlAction(TimedAction { time_s: at, action }));
}

fn trigger(app: &mut App, slug: &str) {
    send(app, Action::Trigger(TriggerId::new(slug)));
}

/// Une image : la voiture avance, l'audio est un kick toutes les 28 images (128 BPM) avec un drop toutes les 10 s.
fn frame(app: &mut App, i: usize, hostile: bool) {
    {
        let world = app.world_mut();
        let mut cars = world.query_filtered::<&mut AbsolutePosition, With<StreamFocus>>();
        for mut position in cars.iter_mut(world) {
            position.0.z -= SPEED * f64::from(FRAME_S);
        }
    }
    let period = if hostile { 6 } else { 28 };
    {
        let mut audio = app.world_mut().resource_mut::<AudioFeatures>();
        audio.0.live = true;
        audio.0.section = Section::Peak;
        audio.0.intensity = 0.9;
        audio.0.kick = i.is_multiple_of(period);
        audio.0.pulse = if i % period < 3 {
            1.0 - (i % period) as f32 / 3.0
        } else {
            0.0
        };
        audio.0.bass = 0.7;
        audio.0.mid = 0.5;
        audio.0.count = (i / period) as u32;
        audio.0.beat = i as f64 / period as f64;
    }
    app.update();
}

fn active(app: &App) -> Vec<EventName> {
    app.world().resource::<TrafficDirector>().active_names().collect()
}

#[test]
fn chaque_evenement_se_declenche_par_action_trigger_et_vit_60_secondes_simulees() {
    for name in EventName::ALL {
        let mut app = app();
        for i in 0..5 {
            frame(&mut app, i, false);
        }
        trigger(&mut app, name.as_str());
        let mut seen_at = None;
        let mut peak_events = 0;
        for i in 5..(5 + 60 * 60) {
            frame(&mut app, i, false);
            if seen_at.is_none() && active(&app).contains(&name) {
                seen_at = Some(i);
            }
            peak_events = peak_events.max(app.world().resource::<TrafficStats>().events);
        }
        assert!(peak_events >= 1, "{name:?} : aucun événement compté");
        let at = seen_at.unwrap_or_else(|| panic!("{name:?} ne s'est pas lancé sur Action::Trigger"));
        assert!(at <= 8, "{name:?} lancé tardivement : image {at}");
        // La file d'ordres est vidée : le déclencheur a été pris.
        let runtime = app.world().resource::<ShowRuntime>();
        assert!(
            runtime.director().state().orders.trigger_pending().is_none(),
            "{name:?}"
        );
    }
}

#[test]
fn un_declencheur_qui_n_est_pas_un_evenement_du_trafic_reste_pour_son_destinataire() {
    let mut app = app();
    for i in 0..5 {
        frame(&mut app, i, false);
    }
    // Une cascade (J7-06/J7-07), pas un événement du trafic : le trafic n'y touche pas.
    let before = active(&app);
    trigger(&mut app, "wall-ride");
    for i in 5..65 {
        frame(&mut app, i, false);
    }
    let runtime = app.world().resource::<ShowRuntime>();
    let pending = runtime
        .director()
        .state()
        .orders
        .trigger_pending()
        .map(|id| id.0.to_string());
    assert_eq!(pending.as_deref(), Some("wall-ride"));
    assert_eq!(active(&app), before, "le trafic n'a rien lancé pour une cascade");
}

/// Ce que la ville montre à une image.
#[derive(Clone, Copy)]
struct City {
    flash: f32,
    win_pulse: f32,
    /// Part de lumière restante en un point fixe de la ville (1 : éclairé).
    light: f32,
}

/// Rejoue `seconds` secondes avec l'orage, les feux d'artifice, la panne et le pouls de la ville lancés sans arrêt, des
/// kicks à 10 Hz et `FlashKill` à `kill_at` (s) s'il y en a un.
fn hostile_city(seconds: f32, kill_at: Option<f32>) -> Vec<City> {
    let mut app = app();
    let mut out = Vec::new();
    let frames = (seconds / FRAME_S) as usize;
    let mut slugs = ["storm", "fireworks", "blackout", "city-pulse"].into_iter().cycle();
    for i in 0..frames {
        let t = i as f32 * FRAME_S;
        // Un déclencheur toutes les 0,25 s, dans l'ordre : l'orage, les salves, la panne et le pouls repartent sans cesse.
        if i % 15 == 0
            && let Some(slug) = slugs.next()
        {
            trigger(&mut app, slug);
        }
        if kill_at.is_some_and(|k| (t - k).abs() < FRAME_S / 2.0) {
            send(&mut app, Action::FlashKill(Switch::On));
        }
        frame(&mut app, i, true);
        out.push(city_now(&app));
    }
    out
}

/// Ce que la ville montre à l'image qui vient de passer.
fn city_now(app: &App) -> City {
    let look = *app.world().resource::<FacadeLook>();
    // Un point de la ville à 150 m du départ de la voiture (le monde rendu est ici l'absolu : pas d'origine flottante).
    let z = -400_000.0 - 150.0;
    let power = look.power;
    let behind_t = (((z as f32 - power.x) * power.w) / power.y * 0.5 + 0.5).clamp(0.0, 1.0);
    let behind = behind_t * behind_t * (3.0 - 2.0 * behind_t);
    City {
        flash: look.flash,
        win_pulse: look.win_pulse,
        light: 1.0 - power.z * behind,
    }
}

fn max_starts_per_second(levels: &[f32]) -> usize {
    let mut starts = Vec::new();
    let mut rising = false;
    let mut previous = 0.0;
    for (i, &level) in levels.iter().enumerate() {
        let up = level > previous + 1e-6;
        if up && !rising {
            starts.push(i);
        }
        rising = up;
        previous = level;
    }
    let window = (1.0 / FRAME_S).round() as usize;
    starts
        .iter()
        .map(|&s| starts.iter().filter(|&&u| u >= s && u < s + window).count())
        .max()
        .unwrap_or(0)
}

#[test]
fn la_ville_tient_les_plafonds_du_limiteur_sous_une_charge_hostile() {
    let city = hostile_city(60.0, None);
    let flash: Vec<f32> = city.iter().map(|c| c.flash).collect();
    let pulse: Vec<f32> = city.iter().map(|c| c.win_pulse).collect();
    let light: Vec<f32> = city.iter().map(|c| c.light).collect();

    // Pas vide : l'éclair, le pouls et la panne ont bien eu lieu, sinon les plafonds ne prouveraient rien.
    assert!(flash.iter().copied().fold(0.0, f32::max) > 0.05, "aucun éclair");
    assert!(
        pulse.iter().copied().fold(0.0, f32::max) > 0.05,
        "aucun pouls de fenêtres"
    );
    assert!(light.iter().copied().fold(1.0, f32::min) < 0.6, "aucune panne");

    // Amplitude : l'éclair ne dépasse jamais le plafond de la voie.
    assert!(
        flash.iter().all(|&v| v <= MAX_FLASH_PEAK + EPS),
        "éclair au-dessus du plafond"
    );
    // Aucune variation plus rapide que `MIN_FADE`, ramenée à la pleine échelle de chacune.
    let max_step = FRAME_S / MIN_FADE + EPS;
    for (name, levels, span) in [
        ("éclair", &flash, MAX_FLASH_PEAK),
        ("pouls", &pulse, 1.0),
        ("panne", &light, 1.0),
    ] {
        let worst = levels
            .windows(2)
            .map(|w| (w[1] - w[0]).abs() / span)
            .fold(0.0, f32::max);
        assert!(worst <= max_step, "{name} : saut de {worst} entre deux images");
    }
    // Au plus trois éclairs par seconde glissante.
    assert!(
        max_starts_per_second(&flash) <= MAX_FLASHES_PER_SECOND,
        "plus de {MAX_FLASHES_PER_SECOND} éclairs par seconde"
    );
}

#[test]
fn flashs_coupes_plus_d_eclair_et_une_panne_adoucie() {
    // Les flashs sont coupés dès la première seconde.
    let city = hostile_city(40.0, Some(1.0));
    let after = &city[(3.0 / FRAME_S) as usize..];
    assert!(
        after.iter().all(|c| c.flash <= EPS),
        "un éclair est sorti après FlashKill : {}",
        after.iter().map(|c| c.flash).fold(0.0, f32::max)
    );
    // La panne n'a plus la profondeur pleine : la ville ne passe jamais sous 55 % de sa lumière.
    let darkest = after.iter().map(|c| c.light).fold(1.0, f32::min);
    assert!(
        darkest >= 0.55 - EPS,
        "panne trop sombre sans l'accord du limiteur : {darkest}"
    );
}
