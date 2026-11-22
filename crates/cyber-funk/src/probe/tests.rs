//! Tests de la sonde : une `App` minimale, un vrai socket local.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use cf_control::{Action, Level};
use cf_probe_proto::{ProbeReply, ProbeRequest, decode_line, encode_line};
use cf_render::{CfSet, ControlAction, ControlBridge, ControlsStatus};

use super::bridge::{ProbeInbox, ProbeStats};
use super::{STATE_FILE, server, write_state_file};

/// Les messages lus par l'application de test.
#[derive(Resource, Default)]
struct Seen(Vec<ControlAction>);

fn collect(mut reader: MessageReader<ControlAction>, mut seen: ResMut<Seen>) {
    seen.0.extend(reader.read().cloned());
}

fn probe_app() -> (App, u16) {
    probe_app_with_timeout(Duration::from_secs(2))
}

fn probe_app_with_timeout(timeout: Duration) -> (App, u16) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let stats = ProbeStats::default();
    let (inbox, _handle) = server::spawn_with_timeout(listener, Arc::clone(&stats.malformed), timeout);
    let mut app = App::new();
    app.add_message::<ControlAction>()
        .init_resource::<Seen>()
        .insert_resource(stats)
        .insert_resource(ProbeInbox::new(inbox))
        .add_systems(Update, collect.after(CfSet::Input));
    super::install_systems(&mut app);
    (app, port)
}

/// Envoie les lignes sur une seule connexion et rend les réponses, en faisant tourner l'application pendant ce temps
/// (le client attend sa réponse, que seul le système Bevy produit).
fn exchange(app: &mut App, port: u16, lines: Vec<String>) -> Vec<ProbeReply> {
    let client = std::thread::spawn(move || {
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut stream = stream;
        let mut replies = Vec::new();
        for line in lines {
            stream.write_all(line.as_bytes()).unwrap();
            let mut answer = String::new();
            reader.read_line(&mut answer).unwrap();
            replies.push(decode_line::<ProbeReply>(&answer).unwrap());
        }
        replies
    });
    while !client.is_finished() {
        app.update();
        std::thread::sleep(Duration::from_millis(2));
    }
    client.join().unwrap()
}

fn seen(app: &App) -> &[ControlAction] {
    &app.world().resource::<Seen>().0
}

#[test]
fn une_action_envoyee_devient_un_message_control_action() {
    let (mut app, port) = probe_app();
    let replies = exchange(&mut app, port, vec![encode_line(&ProbeRequest::SendAction(Action::Go))]);
    assert_eq!(replies, vec![ProbeReply::Ack]);
    app.update();
    assert_eq!(seen(&app).len(), 1);
    assert_eq!(seen(&app)[0].0.action, Action::Go);
}

#[test]
fn une_valeur_hors_bornes_est_refusee() {
    let (mut app, port) = probe_app();
    let requests = [
        ProbeRequest::SendAction(Action::Master(Level::Set(2.0))),
        ProbeRequest::SendAction(Action::Master(Level::Set(f32::NAN))),
        ProbeRequest::SendAction(Action::NudgePhase { beats: f32::INFINITY }),
    ];
    let lines = requests.iter().map(encode_line).collect();
    let replies = exchange(&mut app, port, lines);
    assert!(
        replies.iter().all(|reply| matches!(reply, ProbeReply::Error(_))),
        "{replies:?}"
    );
    app.update();
    assert!(
        seen(&app).is_empty(),
        "aucun message ne doit sortir d'une valeur refusée"
    );
}

#[test]
fn une_ligne_malformee_est_comptee_et_ignoree() {
    let (mut app, port) = probe_app();
    let lines = vec!["ceci n'est pas du json\n".to_owned(), encode_line(&ProbeRequest::Ping)];
    let replies = exchange(&mut app, port, lines);
    assert!(matches!(replies[0], ProbeReply::Error(_)), "{replies:?}");
    assert_eq!(replies[1], ProbeReply::Pong, "la connexion reste utilisable");
    assert_eq!(app.world().resource::<ProbeStats>().malformed(), 1);
}

#[test]
fn une_ligne_trop_longue_est_comptee_et_coupe_la_connexion() {
    let (mut app, port) = probe_app();
    let long = "x".repeat(cf_probe_proto::MAX_LINE_BYTES + 10) + "\n";
    let replies = exchange(&mut app, port, vec![long]);
    assert!(matches!(replies[0], ProbeReply::Error(_)), "{replies:?}");
    assert_eq!(app.world().resource::<ProbeStats>().malformed(), 1);
}

#[test]
fn une_requete_expiree_est_jetee_sans_effet() {
    let (mut app, port) = probe_app_with_timeout(Duration::from_millis(50));
    // L'application ne tourne pas : le client reçoit `Error("délai")` et la requête reste en file.
    let client = std::thread::spawn(move || {
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut stream = stream;
        stream
            .write_all(encode_line(&ProbeRequest::SendAction(Action::Go)).as_bytes())
            .unwrap();
        let mut answer = String::new();
        reader.read_line(&mut answer).unwrap();
        decode_line::<ProbeReply>(&answer).unwrap()
    });
    assert_eq!(client.join().unwrap(), ProbeReply::Error("délai".into()));
    // Quand l'application se réveille, la requête périmée ne doit rien déclencher.
    app.update();
    app.update();
    assert!(seen(&app).is_empty(), "une requête expirée ne doit pas être appliquée");
}

#[test]
fn avec_un_pont_de_controles_l_action_est_horodatee_et_routee_comme_le_clavier() {
    // Le vrai `ControlBridge` : construit par `ControlsPlugin` dans une application à part, puis prélevé.
    let mut donor = App::new();
    donor.add_plugins(cf_render::ControlsPlugin::new(cf_render::ControlsConfig {
        midi: cf_render::MidiChoice::Off, // aucun matériel dans un test
        ..cf_render::ControlsConfig::default()
    }));
    let bridge = donor.world_mut().remove_resource::<ControlBridge>().unwrap();
    let status = donor.world_mut().remove_resource::<ControlsStatus>().unwrap();

    let (mut app, port) = probe_app();
    app.insert_resource(bridge).insert_resource(status);
    std::thread::sleep(Duration::from_millis(30)); // l'horloge du pont avance : `time_s` > 0 prouve que c'est elle.
    let actions = [
        Action::Go,
        Action::TapTempo,
        Action::Resync,
        Action::NudgePhase { beats: 0.25 },
    ];
    let lines = actions
        .iter()
        .cloned()
        .map(|action| encode_line(&ProbeRequest::SendAction(action)))
        .collect();
    let replies = exchange(&mut app, port, lines);
    assert!(replies.iter().all(|reply| *reply == ProbeReply::Ack), "{replies:?}");
    app.update();
    let emitted = seen(&app);
    assert_eq!(emitted.len(), 4);
    assert!(
        emitted.iter().all(|message| message.0.time_s >= 0.03),
        "horloge du pont : {emitted:?}"
    );
    assert!(emitted.windows(2).all(|pair| pair[0].0.time_s <= pair[1].0.time_s));
    // `last_action` n'est écrit que par `Emitter::emit`, le chemin qui route aussi tap, resync et nudge. Le routage
    // lui-même n'est PAS observable d'ici : le tap est `pub(super)` dans cf-render, et sans source de tempo l'arbitre
    // ignore resync et nudge (`beat_at` vaut `None`). On vérifie seulement qu'ils n'ont rien inventé.
    assert!(app.world().resource::<ControlBridge>().arbiter().beat().is_none());
    let status = app.world().resource::<ControlsStatus>();
    assert_eq!(status.last_action.as_ref().map(|(_, action)| action), Some(&actions[3]));
}

#[test]
fn un_client_qui_se_deconnecte_ne_bloque_pas_le_plugin() {
    let (mut app, port) = probe_app();
    {
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        stream.write_all(encode_line(&ProbeRequest::Ping).as_bytes()).unwrap();
    } // fermé sans lire la réponse
    for _ in 0..50 {
        app.update();
        std::thread::sleep(Duration::from_millis(2));
    }
    let replies = exchange(&mut app, port, vec![encode_line(&ProbeRequest::Ping)]);
    assert_eq!(replies, vec![ProbeReply::Pong]);
}

#[test]
fn les_requetes_pas_encore_branchees_repondent_non_implemente() {
    let (mut app, port) = probe_app();
    let replies = exchange(&mut app, port, vec![encode_line(&ProbeRequest::StartSyncRecording)]);
    assert_eq!(replies, vec![ProbeReply::Error("non implémenté".into())]);
}

#[test]
fn le_port_est_ecrit_dans_probe_json() {
    let dir = tempfile::tempdir().unwrap();
    write_state_file(dir.path(), 4242).unwrap();
    let text = std::fs::read_to_string(dir.path().join(STATE_FILE)).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["port"], 4242);
    assert_eq!(json["pid"], std::process::id());
}
