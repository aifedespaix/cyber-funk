//! Tests de la sonde : une `App` minimale, un vrai socket local.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::time::Duration;

use bevy::prelude::*;
use cf_control::{Action, Level};
use cf_probe_proto::{ProbeReply, ProbeRequest, decode_line, encode_line};
use cf_render::{CfSet, ControlAction};

use super::bridge::{ProbeInbox, drain_probe};
use super::{STATE_FILE, server, write_state_file};

/// Les messages lus par l'application de test.
#[derive(Resource, Default)]
struct Seen(Vec<ControlAction>);

fn collect(mut reader: MessageReader<ControlAction>, mut seen: ResMut<Seen>) {
    seen.0.extend(reader.read().cloned());
}

fn probe_app() -> (App, u16) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (inbox, _handle) = server::spawn(listener);
    let mut app = App::new();
    app.add_message::<ControlAction>()
        .init_resource::<Seen>()
        .insert_resource(ProbeInbox::new(inbox))
        .add_systems(Update, (drain_probe.in_set(CfSet::Input), collect.after(CfSet::Input)));
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
    let replies = exchange(&mut app, port, vec![encode_line(&ProbeRequest::State)]);
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
