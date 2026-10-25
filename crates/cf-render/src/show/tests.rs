//! Tests du pont de bout en bout : clavier ou MIDI factice ─▶ `ControlAction` ─▶ chef du spectacle ─▶ `CurrentShow`,
//! passation de DJ et tempo. Le temps de l'image est manuel (60 i/s) ; seuls le MIDI factice et le tap restent en
//! temps réel, parce que leurs horodatages sont pris dans le callback.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::input::keyboard::{Key, KeyCode};
use cf_analysis::TempoSource;
use cf_control::{Action, Binding, ControlProfile, FakeMidi, InputSource, Switch};
use cf_show::director::CueCause;
use cf_show::load::{LoadedShow, LookLibrary};
use cf_show::model::ControlProfileId;

use super::*;
use crate::audio::AudioFeatures;
use crate::controls::tests::{controls_app_with, press, release, seen};
use crate::controls::{ControllerLibrary, ControlsConfig, ControlsStatus, MidiChoice, TextInputFocus};

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// Le show de démonstration, avec ces profils de contrôle pour toute la soirée.
fn show(controls: &[&str]) -> LoadedShow {
    let library = LookLibrary::load_dir(&assets().join("looks")).expect("looks livrés");
    let mut event = LoadedShow::load(&assets().join("shows/demo.ron"), &library)
        .expect("show de démonstration")
        .event()
        .clone();
    event.settings.controls = controls
        .iter()
        .map(|id| ControlProfileId::new(*id).expect("identifiant de profil"))
        .collect();
    LoadedShow::new(event, &library, "test").expect("événement valide")
}

fn library() -> ControllerLibrary {
    let mut library = ControllerLibrary::load_dir(&assets().join("controllers")).expect("profils livrés");
    // Un bouton de tap tempo sur la touche P.
    let mut tap = ControlProfile::new("tap");
    tap.bindings.push(Binding::new(
        InputSource::key(cf_control::Key::Char('p')),
        cf_control::ActionTemplate::TapTempo,
    ));
    library.insert("tap", tap);
    library
}

#[derive(Resource, Default)]
struct Facts(Vec<ShowEvent>);

fn collect_facts(mut facts: ResMut<Facts>, mut events: MessageReader<ShowEventMessage>) {
    facts.0.extend(events.read().map(|e| e.0.clone()));
}

fn app(controls: &[&str], midi: MidiChoice) -> App {
    let mut app = controls_app_with(
        ControlsConfig {
            midi,
            library: library(),
        },
        |app| {
            app.add_plugins(ShowPlugin::new(show(controls)))
                .init_resource::<Facts>()
                .add_systems(Update, collect_facts.after(CfSet::Show));
        },
    );
    // Premières images : le créneau 0 est chargé et ses profils appliqués.
    app.update();
    app.update();
    app
}

fn facts(app: &App) -> &[ShowEvent] {
    &app.world().resource::<Facts>().0
}

fn state(app: &App) -> &ShowState {
    &app.world().resource::<CurrentShow>().0
}

fn called_cue(app: &App, cue: usize) -> bool {
    facts(app)
        .iter()
        .any(|e| matches!(e, ShowEvent::CueCalled { slot: 0, cue: c, .. } if *c == cue))
}

/// Joue des images (et laisse respirer les threads) jusqu'à ce que `done` soit vrai ; échoue au bout de `max_s`.
///
/// Le délai est borné en **temps réel**, et en nombre d'images : les threads MIDI (ouverture des ports, callbacks) n'ont
/// que le temps réel pour avancer, et une machine chargée les retarde de plusieurs centaines de millisecondes. Un délai
/// compté seulement en images (3 s d'horloge de l'application = 180 images, soit moins d'une demi-seconde réelle) faisait
/// échouer ces tests à vide.
fn until(app: &mut App, max_s: f64, what: &str, done: impl Fn(&App) -> bool) {
    let frames = (max_s * 60.0) as usize;
    let deadline = std::time::Instant::now() + Duration::from_secs_f64(max_s);
    let mut played = 0;
    loop {
        app.update();
        played += 1;
        if done(app) {
            return;
        }
        if played >= frames && std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(done(app), "{what} : rien après {max_s} s");
}

#[test]
fn le_spectacle_s_ouvre_et_publie_son_etat() {
    let app = app(&[], MidiChoice::Off);
    assert_eq!(state(&app).slot, 0);
    assert_eq!(state(&app).cue, 0);
    assert!(
        facts(&app)
            .iter()
            .any(|e| matches!(e, ShowEvent::SlotLoaded { slot: 0, .. }))
    );
}

#[test]
fn fleche_droite_change_de_cue() {
    let mut app = app(&[], MidiChoice::Off);
    press(&mut app, KeyCode::ArrowRight, Key::ArrowRight);
    app.update();
    assert_eq!(seen(&app), [Action::Go]);
    assert!(
        facts(&app).iter().any(|e| matches!(
            e,
            ShowEvent::CueCalled {
                slot: 0,
                cue: 1,
                cause: CueCause::Go,
                ..
            }
        )),
        "le cue 1 est appelé par le GO du clavier : {:?}",
        facts(&app)
    );
    // Le cue appelé entre au bout de son entrée (fondu de quatre mesures) : le panneau le verra.
    until(&mut app, 30.0, "entrée du cue 1", |a| state(a).cue == 1);
}

#[test]
fn un_pad_midi_change_de_cue() {
    let fake = FakeMidi::new();
    fake.plug("Pads de test");
    let mut app = app(&["generic-8knobs-8pads"], MidiChoice::Fake(fake.clone()));
    until(&mut app, 3.0, "ouverture du port MIDI", |_| {
        !fake.open_ports().is_empty()
    });
    // Pad 1 (note 36, canal 1) : GO.
    assert!(fake.send("Pads de test", &[0x90, 36, 100]));
    assert!(fake.send("Pads de test", &[0x80, 36, 0]));
    until(&mut app, 3.0, "action du pad", |a| seen(a).contains(&Action::Go));
    app.update();
    assert!(called_cue(&app, 1), "le pad a appelé le cue 1");
    // Le panneau voit le port et le dernier message. L'instantané des ports n'est rafraîchi que quelques fois par seconde
    // (`STATUS_PERIOD_S`, il prend un verrou et clone) : on l'attend, au lieu de supposer qu'un rafraîchissement a eu lieu
    // entre l'ouverture du port et ce test (c'était la cause de l'échec intermittent sous charge : le test finissait avant
    // le rafraîchissement suivant).
    until(&mut app, 3.0, "le port dans le panneau", |a| {
        let status = a.world().resource::<ControlsStatus>();
        status.ports.connected.iter().any(|p| p.name == "Pads de test")
    });
    let status = app.world().resource::<ControlsStatus>();
    assert!(status.last_input.is_some());
}

#[test]
fn rien_ne_part_quand_egui_a_le_clavier_et_le_cue_ne_bouge_pas() {
    let mut app = app(&[], MidiChoice::Off);
    app.world_mut().resource_mut::<TextInputFocus>().0 = true;
    for (code, key) in [
        (KeyCode::ArrowRight, Key::ArrowRight),
        (KeyCode::KeyD, Key::Character("d".into())),
    ] {
        press(&mut app, code, key);
        app.update();
    }
    assert_eq!(seen(&app), []);
    assert!(
        !facts(&app)
            .iter()
            .any(|e| matches!(e, ShowEvent::CueCalled { cue: 1, .. }))
    );
}

#[test]
fn la_passation_relache_le_noir_tenu_et_change_de_table() {
    let fake = FakeMidi::new();
    fake.plug("Pads de test");
    let mut app = app(&["generic-8knobs-8pads"], MidiChoice::Fake(fake.clone()));
    until(&mut app, 3.0, "ouverture du port MIDI", |_| {
        !fake.open_ports().is_empty()
    });
    // Pad 5 (note 40) tenu : noir.
    assert!(fake.send("Pads de test", &[0x90, 40, 100]));
    until(&mut app, 3.0, "noir demandé", |a| state(a).blackout_on);
    // DJ suivant (touche G) : la passation attend un temps fort, fond la palette, puis charge le créneau 1.
    press(&mut app, KeyCode::KeyG, Key::Character("g".into()));
    release(&mut app, KeyCode::KeyG, Key::Character("g".into()));
    until(&mut app, 60.0, "créneau 1 chargé", |a| {
        facts(a)
            .iter()
            .any(|e| matches!(e, ShowEvent::SlotLoaded { slot: 1, .. }))
    });
    // Le bouton tenu a été relâché par `Mapper::reload` : le noir s'éteint sans que le doigt ait quitté le pad.
    until(&mut app, 3.0, "noir relâché", |a| !state(a).blackout_on);
    assert!(seen(&app).contains(&Action::Blackout(Switch::Off)));
    // Le vrai relâchement du pad, ensuite, ne produit rien de plus (la table du créneau ne l'a pas vu enfoncé).
    let before = seen(&app).len();
    assert!(fake.send("Pads de test", &[0x80, 40, 0]));
    for _ in 0..30 {
        app.update();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(seen(&app).len(), before);
}

#[test]
fn le_tap_tempo_devient_la_source_de_tempo_publiee() {
    let mut app = app(&["tap"], MidiChoice::Off);
    // Cinq frappes espacées de 300 ms (200 BPM), en temps réel : l'horodatage vient de l'horloge des contrôles.
    for _ in 0..5 {
        press(&mut app, KeyCode::KeyP, Key::Character("p".into()));
        app.update();
        release(&mut app, KeyCode::KeyP, Key::Character("p".into()));
        std::thread::sleep(Duration::from_millis(300));
    }
    app.update();
    let features = app.world().resource::<AudioFeatures>();
    assert_eq!(features.tempo_source, TempoSource::Tap);
    let bpm = features.bpm.expect("tempo publié");
    assert!((150.0..250.0).contains(&bpm), "tempo tapé : {bpm}");
}

#[test]
fn l_horloge_midi_prend_la_main_et_se_publie() {
    let fake = FakeMidi::new();
    fake.plug("Logiciel de mix");
    let mut app = app(&[], MidiChoice::Fake(fake.clone()));
    until(&mut app, 3.0, "ouverture du port MIDI", |_| {
        !fake.open_ports().is_empty()
    });
    assert!(fake.send("Logiciel de mix", &[0xFA]));
    // ~125 BPM : 24 impulsions par noire, une toutes les 20 ms, pendant deux secondes.
    for _ in 0..100 {
        assert!(fake.send("Logiciel de mix", &[0xF8]));
        std::thread::sleep(Duration::from_millis(20));
        app.update();
    }
    let features = app.world().resource::<AudioFeatures>();
    assert_eq!(features.tempo_source, TempoSource::Clock);
    let bpm = features.bpm.expect("tempo publié");
    assert!((90.0..170.0).contains(&bpm), "tempo de l'horloge : {bpm}");
}

#[test]
fn sans_show_les_controles_tournent_quand_meme() {
    let mut app = controls_app_with(
        ControlsConfig {
            midi: MidiChoice::Off,
            library: ControllerLibrary::new(),
        },
        |app| {
            app.add_plugins(ShowPlugin {
                show: None,
                titles: DropTitles::empty(),
                seed: 0,
                utc_offset_s: 0.0,
            });
        },
    );
    press(&mut app, KeyCode::ArrowRight, Key::ArrowRight);
    app.update();
    assert_eq!(seen(&app), [Action::Go]);
    assert!(app.world().get_resource::<CurrentShow>().is_none());
}
