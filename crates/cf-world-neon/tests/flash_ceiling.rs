//! Plafond de flashs sur la **sortie finale** (J8-07, règle 7, WCAG 2.3.1) : le vrai chef du spectacle (show de
//! démonstration) publie `CurrentShowFx` par le pont de `ShowPlugin`, le post-process et la voiture le lisent, et on
//! mesure ce que leur limiteur laisse passer à l'écran (le niveau de flash et de vague du post-process, le pouls et le
//! clignotant de la voiture), sous une charge hostile : kicks à 10 Hz au sommet, drops forcés et changements de cue
//! sans arrêt, puis `FlashKill`.
//!
//! Sans rendu : le temps est manuel (60 i/s) et seule la partie logique des plugins tourne.

#![allow(clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::WindowFocused;
use cf_analysis::Section;
use cf_control::{Action, Switch, TimedAction};
use cf_render::{
    AudioFeatures, ControlAction, ControllerLibrary, ControlsConfig, ControlsPlugin, CorePlugin, CurrentShowFx,
    MidiChoice, PostFxPlugin, PostFxState, ShowPlugin,
};
use cf_show::limiter::{MAX_FLASH_PEAK, MAX_FLASHES_PER_SECOND, MAX_SWEEP_PEAK, MIN_FADE};
use cf_show::load::{LoadedShow, LookLibrary};
use cf_world_neon::car::safety::CarFlash;

const FRAME_S: f32 = 1.0 / 60.0;
/// Marge numérique sur les plafonds (flottants).
const EPS: f32 = 1e-3;

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn app() -> App {
    let looks = LookLibrary::load_dir(&assets().join("looks")).expect("looks livrés");
    let show = LoadedShow::load(&assets().join("shows/demo.ron"), &looks).expect("show de démonstration");
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CorePlugin))
        .init_resource::<AudioFeatures>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_message::<KeyboardInput>()
        .add_message::<WindowFocused>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(f64::from(FRAME_S))))
        .add_plugins((
            ControlsPlugin::new(ControlsConfig {
                midi: MidiChoice::Off,
                library: ControllerLibrary::new(),
            }),
            ShowPlugin::new(show),
            PostFxPlugin,
        ));
    app.update();
    app
}

fn send(app: &mut App, action: Action, at: f64) {
    app.world_mut()
        .write_message(ControlAction(TimedAction { time_s: at, action }));
}

/// Ce que l'écran montre à une image.
#[derive(Clone, Copy)]
struct Output {
    flash: f32,
    sweep: f32,
    car_kick: f32,
    car_blink: f32,
}

/// Les sorties d'une charge hostile de `seconds`, avec `FlashKill` envoyé à `kill_at` (s) s'il y en a un.
fn run(seconds: f32, kill_at: Option<f32>) -> Vec<Output> {
    let mut app = app();
    let mut car = CarFlash::default();
    let mut outputs = Vec::new();
    let frames = (seconds / FRAME_S) as usize;
    let mut pulse = 0.0_f32;
    for frame in 0..frames {
        let t = frame as f32 * FRAME_S;
        // Kicks à 10 Hz au sommet : le pire cas du détecteur de front de chaque effet.
        pulse = if frame % 6 == 0 { 1.0 } else { pulse * 0.8 };
        {
            let mut audio = app.world_mut().resource_mut::<AudioFeatures>();
            audio.0.live = true;
            audio.0.section = Section::Peak;
            audio.0.intensity = 0.95;
            audio.0.pulse = pulse;
            audio.0.kick = frame % 6 == 0;
        }
        let now = f64::from(t);
        // Un drop forcé toutes les 0,3 s, un changement de cue toutes les 0,5 s : la vague et le flash du drop demandés
        // bien plus vite que le limiteur ne les permet.
        if frame % 18 == 0 {
            send(&mut app, Action::ForceDrop, now);
        }
        if frame % 30 == 0 {
            send(&mut app, Action::Go, now);
        }
        if kill_at.is_some_and(|k| (t - k).abs() < FRAME_S / 2.0) {
            send(&mut app, Action::FlashKill(Switch::On), now);
        }
        app.update();

        let fx = app.world().resource::<CurrentShowFx>().0;
        let policy = Some((fx.flashes, fx.breathing));
        // Le clignotant bat à 10 Hz aussi (le vrai est à 1,5 Hz) : pire cas.
        car.update(FRAME_S, now, policy, pulse, (frame / 3) % 2 == 0);
        let post = app.world().resource::<PostFxState>().show_fx().clone();
        outputs.push(Output {
            flash: post.flash(),
            sweep: post.sweep(),
            car_kick: car.kick(),
            car_blink: car.blink(),
        });
    }
    outputs
}

/// Plus grand nombre de départs sur une fenêtre glissante d'une seconde, sur les voies données. Un départ est le début
/// d'une montée (le niveau se met à croître après avoir été stable ou en descente) : c'est l'instant de la demande, que
/// plafonne le limiteur, et non celui où le niveau franchit un seuil (qui dépend de la durée de montée).
fn max_starts_per_second(levels: &[Vec<f32>]) -> usize {
    let mut starts = Vec::new();
    for channel in levels {
        // Un niveau nul avant la première image : un départ à l'image 0 compte comme les autres.
        let levels: Vec<f32> = std::iter::once(0.0).chain(channel.iter().copied()).collect();
        let mut rising = false;
        for (frame, pair) in levels.windows(2).enumerate() {
            let up = pair[1] > pair[0] + 1e-6;
            if up && !rising {
                starts.push(frame);
            }
            rising = up;
        }
    }
    starts.sort_unstable();
    let window = (1.0 / FRAME_S).round() as usize;
    starts
        .iter()
        .map(|&s| starts.iter().filter(|&&u| u >= s && u < s + window).count())
        .max()
        .unwrap_or(0)
}

/// Variation maximale d'une image à l'autre, ramenée à la pleine échelle `span` : pas plus vite que `MIN_FADE`.
fn max_step(levels: impl Iterator<Item = f32>, span: f32) -> f32 {
    let levels: Vec<f32> = levels.collect();
    levels.windows(2).map(|w| (w[1] - w[0]).abs() / span).fold(0.0, f32::max)
}

#[test]
fn la_sortie_finale_tient_le_plafond_sous_une_charge_hostile() {
    let outputs = run(60.0, None);
    let flash: Vec<f32> = outputs.iter().map(|o| o.flash).collect();
    let sweep: Vec<f32> = outputs.iter().map(|o| o.sweep).collect();
    let kick: Vec<f32> = outputs.iter().map(|o| o.car_kick).collect();
    let blink: Vec<f32> = outputs.iter().map(|o| o.car_blink).collect();

    // Pas vide : des flashs ont bien eu lieu, sinon le plafond ne prouverait rien.
    assert!(flash.iter().copied().fold(0.0, f32::max) > 0.05, "aucun flash de post-process");
    assert!(kick.iter().copied().fold(0.0, f32::max) > 0.05, "aucun pouls de la voiture");

    // Amplitude.
    assert!(flash.iter().all(|&v| v <= MAX_FLASH_PEAK + EPS), "flash au-dessus du plafond");
    assert!(sweep.iter().all(|&v| v <= MAX_SWEEP_PEAK + EPS), "vague au-dessus du plafond");
    assert!(kick.iter().chain(&blink).all(|&v| v <= 1.0 + EPS), "voiture au-dessus de son plafond");

    // Vitesse : jamais plus vite que `MIN_FADE` pour la pleine échelle de la voie.
    let limit = FRAME_S / MIN_FADE + EPS;
    assert!(max_step(flash.iter().copied(), MAX_FLASH_PEAK) <= limit, "flash trop brusque");
    assert!(max_step(sweep.iter().copied(), MAX_SWEEP_PEAK) <= limit, "vague trop brusque");
    assert!(max_step(kick.iter().copied(), 1.0) <= limit, "pouls de la voiture trop brusque");
    assert!(max_step(blink.iter().copied(), 1.0) <= limit, "clignotant trop brusque");

    // Cadence : au plus 3 départs par seconde glissante (flash et vague du post-process ensemble ; chaque limiteur de la
    // voiture à part, voir `car::safety`).
    assert!(
        max_starts_per_second(&[flash.clone(), sweep.clone()]) <= MAX_FLASHES_PER_SECOND,
        "{} : plus de {MAX_FLASHES_PER_SECOND} flashs par seconde sur le post-process", max_starts_per_second(&[flash.clone(), sweep.clone()])
    );
    assert!(max_starts_per_second(&[kick]) <= MAX_FLASHES_PER_SECOND, "pouls de la voiture trop rapide");
    assert!(max_starts_per_second(&[blink]) <= MAX_FLASHES_PER_SECOND, "clignotant trop rapide");
}

#[test]
fn flash_kill_eteint_tout_en_un_fondu_et_le_garde_eteint() {
    let kill_at = 20.0;
    let outputs = run(40.0, Some(kill_at));
    // Après le fondu de sortie (`MIN_FADE`, plus une marge de quelques images), plus rien, jusqu'à la fin.
    let settled = ((kill_at + 4.0 * MIN_FADE) / FRAME_S) as usize;
    for (i, out) in outputs.iter().enumerate().skip(settled) {
        assert!(
            out.flash < EPS && out.sweep < EPS && out.car_kick < EPS && out.car_blink < EPS,
            "lumière après FlashKill à l'image {i} ({} s) : {} {} {} {}",
            i as f32 * FRAME_S, out.flash, out.sweep, out.car_kick, out.car_blink
        );
    }
    // Et avant le `FlashKill`, il y avait bien des flashs.
    let before = ((kill_at / FRAME_S) as usize).min(outputs.len());
    assert!(outputs[..before].iter().any(|o| o.flash > 0.05));
}
