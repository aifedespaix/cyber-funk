//! Moniteur MIDI : liste les ports, affiche en direct les messages reçus et, avec un profil, l'action qu'ils produisent.
//!
//! ```text
//! cargo run -p cf-control --example midi_monitor -- --list
//! cargo run -p cf-control --example midi_monitor                       # tous les ports, jusqu'à Ctrl+C
//! cargo run -p cf-control --example midi_monitor -- --port DDJ-FLX4 --seconds 30
//! cargo run -p cf-control --example midi_monitor -- --profile assets/controllers/generic-8knobs-8pads.ron
//! ```
//!
//! Sert à vérifier un contrôleur avant un show : quels numéros de notes et de CC il envoie, sur quel canal (affiché de
//! 1 à 16), et si le profil les reconnaît. Débranchez et rebranchez le contrôleur pour voir le rebranchement à chaud.

// Un outil en ligne de commande dont la sortie est le but : les lints du reste du dépôt contre `println!` ne s'y
// appliquent pas.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::process::ExitCode;
use std::time::{Duration, Instant};

use cf_control::{
    ControlProfile, Dispatch, Learner, Mapper, MidiBackend, MidiConfig, MidiInput, MidiItem, MidiMessage, MidirBackend,
    PortEvent, PortSelection, dispatch,
};

/// Période de lecture du flux : bien en dessous d'une image, assez large pour ne pas occuper un cœur.
const POLL: Duration = Duration::from_millis(2);

struct Args {
    list: bool,
    port: Option<String>,
    seconds: Option<f64>,
    profile: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        list: false,
        port: None,
        seconds: None,
        profile: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} attend une valeur"));
        match flag.as_str() {
            "--list" => args.list = true,
            "--port" => args.port = Some(value("--port")?),
            "--profile" => args.profile = Some(value("--profile")?),
            "--seconds" => {
                let text = value("--seconds")?;
                args.seconds = Some(
                    text.parse()
                        .map_err(|_| format!("--seconds : « {text} » n'est pas un nombre"))?,
                );
            }
            other => {
                return Err(format!(
                    "option inconnue « {other} » (--list, --port NOM, --seconds N, --profile FICHIER)"
                ));
            }
        }
    }
    Ok(args)
}

fn describe(message: MidiMessage) -> String {
    // Canaux affichés de 1 à 16, comme sur les fiches techniques.
    match message {
        MidiMessage::NoteOn {
            channel,
            note,
            velocity,
        } => format!(
            "note on   canal {:>2}  note {note:>3}  vélocité {velocity}",
            channel + 1
        ),
        MidiMessage::NoteOff { channel, note, .. } => format!("note off  canal {:>2}  note {note:>3}", channel + 1),
        MidiMessage::ControlChange {
            channel,
            controller,
            value,
        } => format!(
            "CC        canal {:>2}  cc   {controller:>3}  valeur    {value}",
            channel + 1
        ),
        MidiMessage::ProgramChange { channel, program } => {
            format!("programme canal {:>2}  n°   {program:>3}", channel + 1)
        }
        other => format!("{other:?}"),
    }
}

fn run(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    if args.list {
        let ports = MidirBackend.list_ports()?;
        if ports.is_empty() {
            println!("aucun port MIDI d'entrée");
        }
        for name in ports {
            println!(
                "{name}    (nom court pour un profil : « {} »)",
                cf_control::canonical_port_name(&name)
            );
        }
        return Ok(());
    }

    let profile = match args.profile.as_deref() {
        Some(path) => Some(ControlProfile::from_ron(&std::fs::read_to_string(path)?)?),
        None => None,
    };
    let mut mapper = match &profile {
        Some(p) => Mapper::from_profiles(&[p])?.0,
        None => Mapper::new(),
    };
    let mut learner = Learner::new();

    let selection = args
        .port
        .clone()
        .map_or(PortSelection::All, |name| PortSelection::Named(vec![name]));
    let input = MidiInput::open(MidiConfig {
        selection,
        ..MidiConfig::default()
    })?;
    println!("écoute (Ctrl+C pour quitter)…");

    let start = Instant::now();
    loop {
        if args.seconds.is_some_and(|s| start.elapsed().as_secs_f64() >= s) {
            break;
        }
        let Some(item) = input.try_recv() else {
            std::thread::sleep(POLL);
            continue;
        };
        let time_s = match &item {
            MidiItem::Message(event) => event.time_s,
            MidiItem::Port(_) => start.elapsed().as_secs_f64(),
        };
        let line = match &item {
            MidiItem::Port(PortEvent::Connected { port, name }) => format!("port {} ouvert : {name}", port.0),
            MidiItem::Port(PortEvent::Disconnected { port, name }) => format!("port {} fermé : {name}", port.0),
            MidiItem::Message(event) => format!("port {}  {}", event.port.0, describe(event.message)),
        };
        let mapped = match dispatch(item, &mut mapper, &mut learner) {
            Dispatch::Action(timed) => format!("  →  {:?}", timed.action),
            _ => String::new(),
        };
        println!("{time_s:>9.3}  {line}{mapped}");
    }
    let stats = input.stats();
    println!("{} messages, {} jetés", stats.delivered(), stats.dropped());
    Ok(())
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("erreur : {error}");
            ExitCode::FAILURE
        }
    }
}
