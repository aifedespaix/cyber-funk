//! Tests de l'enregistrement de la trace de synchronisation et du choix de piste : une `App` minimale, requêtes
//! déposées directement dans le canal (le déroulé image par image doit être exact).

#![allow(clippy::panic)] // un test qui reçoit la mauvaise réponse doit échouer bruyamment

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_audio::playlist::Track;
use cf_probe_proto::{ProbeReply, ProbeRequest, SyncTrace};
use cf_render::audio::{AudioCommand, AudioStatus, FilteredSignals};
use cf_render::postfx::GradeUniforms;
use cf_render::{Compositor, CorePlugin, FloatingOrigin, LayersPlugin, PostFxPlugin};

use super::bridge::ProbeInbox;
use super::server::Pending;
use super::sync_recorder::{MAX_SYNC_SAMPLES, SyncRecorder, visible_light};

const FRAME: Duration = Duration::from_micros(16_667);

fn app_with_channel() -> (App, Sender<Pending>) {
    let (tx, rx) = mpsc::channel();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<cf_render::ControlAction>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME))
        .insert_resource(ProbeInbox::new(rx));
    super::install_systems(&mut app);
    (app, tx)
}

/// Application avec le vrai post-process : la lumière enregistrée est celle que `update_postfx` écrit.
fn app_with_postfx() -> (App, Sender<Pending>) {
    let (tx, rx) = mpsc::channel();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), CorePlugin))
        .init_asset::<Image>()
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .init_resource::<FloatingOrigin>()
        .add_message::<cf_render::ControlAction>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME))
        .insert_resource(ProbeInbox::new(rx))
        .add_plugins((
            LayersPlugin {
                fallback_size: UVec2::new(64, 36),
            },
            PostFxPlugin,
        ));
    super::install_systems(&mut app);
    app.update();
    app.update();
    (app, tx)
}

fn ask(tx: &Sender<Pending>, request: ProbeRequest) -> Receiver<ProbeReply> {
    let (reply, answer) = mpsc::channel();
    tx.send(Pending {
        request,
        deadline: Instant::now() + Duration::from_secs(60),
        reply,
    })
    .unwrap();
    answer
}

fn start(app: &mut App, tx: &Sender<Pending>) {
    let answer = ask(tx, ProbeRequest::StartSyncRecording);
    app.update();
    assert_eq!(answer.try_recv().unwrap(), ProbeReply::Ack);
}

fn stop(app: &mut App, tx: &Sender<Pending>) -> SyncTrace {
    let answer = ask(tx, ProbeRequest::StopSyncRecording);
    app.update();
    match answer.try_recv().unwrap() {
        ProbeReply::Trace(trace) => trace,
        other => panic!("réponse inattendue : {other:?}"),
    }
}

fn set_light(app: &mut App, bloom: f32, flash: f32, sweep: f32) {
    let world = app.world_mut();
    let mut query = world.query::<(&mut GradeUniforms, &mut Bloom)>();
    for (mut grade, mut b) in query.iter_mut(world) {
        grade.flash = flash;
        grade.sweep = sweep;
        b.intensity = bloom;
    }
}

#[test]
fn la_lumiere_visible_somme_bloom_flash_et_vague_bornee_a_1() {
    assert!((visible_light(Some(0.4), 0.1, 0.2) - 0.7).abs() < 1e-6);
    assert!(
        (visible_light(None, 0.1, 0.2) - 0.3).abs() < 1e-6,
        "bloom coupé : le flash seul"
    );
    assert!((visible_light(Some(0.9), 0.5, 0.5) - 1.0).abs() < 1e-6, "bornée à 1");
    assert!(
        visible_light(Some(f32::NAN), 0.0, 0.0).abs() < 1e-6,
        "non fini : 0, jamais NaN"
    );
}

#[test]
fn une_image_un_echantillon_avec_la_lumiere_et_les_signaux() {
    let (mut app, tx) = app_with_channel();
    app.world_mut()
        .spawn((Compositor, GradeUniforms::default(), Bloom::default()));
    app.insert_resource(FilteredSignals::default());
    start(&mut app, &tx);
    // Image 1 (celle du Start) : lumière au repos. Images 2 et 3 : un kick, la grille crue, plus de lumière.
    for (kick, trust, bloom) in [(false, 0.2, 0.3), (true, 0.7, 0.5)] {
        set_light(&mut app, bloom, 0.0, 0.0);
        let mut signals = app.world_mut().resource_mut::<FilteredSignals>();
        signals.kick_now = kick;
        signals.grid_trust = trust;
        app.update();
    }
    let trace = stop(&mut app, &tx);
    assert_eq!(trace.samples.len(), 3, "{trace:?}");
    assert!(!trace.samples[1].kick_now && trace.samples[2].kick_now);
    assert!((trace.samples[1].luminance - 0.3).abs() < 1e-6, "{trace:?}");
    assert!((trace.samples[2].luminance - 0.5).abs() < 1e-6);
    assert!((trace.samples[2].grid_trust - 0.7).abs() < 1e-6);
    // L'horloge est celle de la simulation : strictement croissante, au pas imposé.
    let dt = trace.samples[2].t_s - trace.samples[1].t_s;
    assert!((dt - FRAME.as_secs_f64()).abs() < 1e-6, "{dt}");
}

#[test]
fn rien_n_est_enregistre_avant_start_ni_apres_stop() {
    let (mut app, tx) = app_with_channel();
    app.insert_resource(FilteredSignals::default());
    for _ in 0..3 {
        app.update();
    }
    start(&mut app, &tx);
    app.update();
    let during = stop(&mut app, &tx);
    assert_eq!(
        during.samples.len(),
        2,
        "l'image du Start et la suivante, pas les trois d'avant"
    );
    for _ in 0..3 {
        app.update();
    }
    assert!(
        app.world().resource::<SyncRecorder>().is_empty(),
        "arrêté : plus rien ne s'ajoute"
    );
}

#[test]
fn stop_sans_start_et_double_start_sont_refuses() {
    let (mut app, tx) = app_with_channel();
    let answer = ask(&tx, ProbeRequest::StopSyncRecording);
    app.update();
    assert!(matches!(answer.try_recv().unwrap(), ProbeReply::Error(why) if why.contains("aucun enregistrement")));
    start(&mut app, &tx);
    let answer = ask(&tx, ProbeRequest::StartSyncRecording);
    app.update();
    assert!(matches!(answer.try_recv().unwrap(), ProbeReply::Error(why) if why.contains("déjà en cours")));
    // Le refus n'a pas vidé ce qui était enregistré.
    assert_eq!(stop(&mut app, &tx).samples.len(), 2);
}

#[test]
fn au_plafond_l_enregistrement_garde_le_debut_sans_jamais_reallouer() {
    let (mut app, tx) = app_with_channel();
    app.insert_resource(SyncRecorder::with_capacity(4));
    start(&mut app, &tx);
    let capacity = app.world().resource::<SyncRecorder>().capacity();
    assert!(capacity >= 4);
    for _ in 0..20 {
        app.update();
    }
    let recorder = app.world().resource::<SyncRecorder>();
    assert_eq!(recorder.capacity(), capacity, "le tampon n'a pas grandi");
    assert!(recorder.overflowed());
    let trace = stop(&mut app, &tx);
    assert_eq!(trace.samples.len(), 4, "le début est gardé, la suite jetée");
    assert!(
        !app.world().resource::<SyncRecorder>().overflowed(),
        "un nouvel enregistrement repart propre"
    );
}

#[test]
fn le_plafond_par_defaut_tient_dans_une_reponse() {
    // 36 000 échantillons : le test de `cf-probe-proto` vérifie qu'ils passent le codec de réponse.
    assert_eq!(MAX_SYNC_SAMPLES, 36_000);
    assert_eq!(
        SyncRecorder::default().capacity(),
        0,
        "rien n'est réservé tant qu'on n'enregistre pas"
    );
}

#[test]
fn le_vrai_post_process_est_lu_dans_la_meme_image_que_le_kick() {
    let (mut app, tx) = app_with_postfx();
    app.insert_resource(FilteredSignals::default());
    start(&mut app, &tx);
    for frame in 0..8 {
        // Le kick tombe à l'image 3 : `update_postfx` en tire un bloom plus fort dans cette même image.
        let mut signals = app.world_mut().resource_mut::<FilteredSignals>();
        signals.kick_now = frame == 3;
        if frame == 3 {
            signals.register_kick();
        }
        app.update();
    }
    let trace = stop(&mut app, &tx);
    let samples = &trace.samples;
    let kick = samples.iter().position(|s| s.kick_now).expect("le kick est enregistré");
    // Le saut est dans l'image du kick elle-même : enregistrée avant `update_postfx`, elle l'aurait vu une image plus
    // tard. (Le pouls ne retombe pas ici : le moteur de routage qui l'éteint n'est pas dans cette application.)
    assert!(
        samples[kick].luminance > samples[0].luminance + 0.02,
        "le kick allume l'image : {samples:?}"
    );
    assert!(
        (samples[kick - 1].luminance - samples[0].luminance).abs() < 1e-6,
        "l'image d'avant le kick est encore au repos : {samples:?}"
    );
}

fn tracks(titles: &[(&str, &str)]) -> Arc<[Track]> {
    titles
        .iter()
        .map(|(artist, title)| Track {
            title: (*title).to_owned(),
            artist: (*artist).to_owned(),
            path: format!("{title}.mp3").into(),
            local: false,
            duration_s: None,
            error: None,
        })
        .collect()
}

#[derive(Resource, Default)]
struct Heard(Vec<AudioCommand>);

fn collect_audio(mut reader: MessageReader<AudioCommand>, mut seen: ResMut<Heard>) {
    seen.0.extend(reader.read().cloned());
}

fn app_with_playlist() -> (App, Sender<Pending>) {
    let (mut app, tx) = app_with_channel();
    app.init_resource::<Heard>()
        .add_systems(Update, collect_audio.after(cf_render::CfSet::Input));
    app.insert_resource(AudioStatus {
        tracks: tracks(&[
            ("Snabisch", "Night Prowler"),
            ("Tozan", "Winning the Race"),
            ("Yongki", "Chase"),
        ]),
        ..AudioStatus::default()
    });
    (app, tx)
}

fn select(app: &mut App, tx: &Sender<Pending>, name: &str) -> ProbeReply {
    let answer = ask(tx, ProbeRequest::SelectTrack { name: name.into() });
    app.update();
    answer.try_recv().unwrap()
}

#[test]
fn une_piste_se_choisit_par_son_titre_sans_casse() {
    let (mut app, tx) = app_with_playlist();
    assert_eq!(select(&mut app, &tx, "chase"), ProbeReply::Ack);
    assert_eq!(app.world().resource::<Heard>().0, vec![AudioCommand::DemoSelect(2)]);
}

#[test]
fn une_piste_se_choisit_par_artiste_et_titre_ou_par_fragment_unique() {
    let (mut app, tx) = app_with_playlist();
    assert_eq!(select(&mut app, &tx, "Tozan - Winning the Race"), ProbeReply::Ack);
    assert_eq!(select(&mut app, &tx, "prowler"), ProbeReply::Ack);
    assert_eq!(
        app.world().resource::<Heard>().0,
        vec![AudioCommand::DemoSelect(1), AudioCommand::DemoSelect(0)]
    );
}

#[test]
fn une_piste_inconnue_ou_ambigue_est_refusee_avec_la_liste() {
    let (mut app, tx) = app_with_playlist();
    // « i » est dans les trois titres (ou artistes) : ambigu. « zzz » n'existe pas.
    for name in ["i", "zzz", ""] {
        let ProbeReply::Error(why) = select(&mut app, &tx, name) else {
            panic!("{name:?} aurait dû être refusé");
        };
        assert!(why.contains("Chase"), "la liste des pistes aide à corriger : {why}");
    }
    assert!(
        app.world().resource::<Heard>().0.is_empty(),
        "aucun ordre audio sur un refus"
    );
}

#[test]
fn sans_playlist_le_choix_de_piste_est_refuse() {
    let (mut app, tx) = app_with_channel();
    app.init_resource::<Heard>();
    assert!(matches!(select(&mut app, &tx, "Chase"), ProbeReply::Error(why) if why.contains("aucune piste")));
}
