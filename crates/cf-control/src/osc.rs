//! Entrée OSC (feature `osc`, J3-05) : un serveur UDP qui traduit des messages `/cf/...` en [`Action`].
//!
//! Les adresses sont documentées dans `docs/osc.md`. Le serveur tourne dans son propre thread : il lit un datagramme,
//! le décode avec `rosc`, traduit chaque message (les bundles sont aplatis) et pousse les actions dans un canal borné.
//! L'application les relit une fois par image avec [`OscInput::try_recv`], comme le MIDI. Un datagramme invalide est
//! compté puis ignoré, jamais une panne.

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use rosc::{OscMessage, OscPacket, OscType};

use crate::{Action, CueIndex, DjIndex, Layer, Level, MACRO_COUNT, MacroIndex, Switch};

/// Port d'écoute par défaut.
pub const DEFAULT_OSC_PORT: u16 = 9000;
/// Capacité du canal vers l'application.
pub const QUEUE_CAPACITY: usize = 256;
/// Plus gros datagramme accepté (le maximum d'un datagramme UDP).
const MAX_DATAGRAM: usize = 65_536;
/// Période à laquelle le thread vérifie qu'on lui demande de s'arrêter.
const POLL: Duration = Duration::from_millis(100);

/// Compteurs du serveur, lisibles depuis n'importe quel thread.
#[derive(Debug, Default)]
pub struct OscStats {
    /// Messages traduits en actions.
    pub accepted: AtomicU64,
    /// Messages valides mais inconnus de `/cf/...` (adresse ou arguments non compris).
    pub ignored: AtomicU64,
    /// Datagrammes qui ne sont pas de l'OSC.
    pub malformed: AtomicU64,
    /// Actions jetées parce que l'application ne les lisait pas assez vite.
    pub dropped: AtomicU64,
}

/// Le serveur OSC. S'arrête quand on le lâche.
#[derive(Debug)]
pub struct OscInput {
    receiver: Receiver<Action>,
    stats: Arc<OscStats>,
    stop: Arc<AtomicBool>,
    local_addr: SocketAddr,
    worker: Option<JoinHandle<()>>,
}

impl OscInput {
    /// Ouvre le port UDP `addr` (par exemple `0.0.0.0:9000`, ou `127.0.0.1:0` pour un port libre) et lance le thread.
    ///
    /// # Errors
    ///
    /// Port déjà pris ou adresse refusée par le système.
    pub fn bind(addr: SocketAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_read_timeout(Some(POLL))?;
        let local_addr = socket.local_addr()?;
        let (sender, receiver) = bounded(QUEUE_CAPACITY);
        let stats = Arc::new(OscStats::default());
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let (stats, stop) = (Arc::clone(&stats), Arc::clone(&stop));
            std::thread::Builder::new()
                .name("cf-osc".to_owned())
                .spawn(move || serve(&socket, &sender, &stats, &stop))?
        };
        Ok(Self {
            receiver,
            stats,
            stop,
            local_addr,
            worker: Some(worker),
        })
    }

    /// L'adresse réellement écoutée (utile quand on a demandé le port 0).
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// L'action suivante, sans attendre.
    #[must_use]
    pub fn try_recv(&self) -> Option<Action> {
        self.receiver.try_recv().ok()
    }

    /// Les compteurs du serveur.
    #[must_use]
    pub fn stats(&self) -> &OscStats {
        &self.stats
    }
}

impl Drop for OscInput {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            // Une panique du thread ne doit pas se propager dans l'application.
            let _ = worker.join();
        }
    }
}

fn serve(socket: &UdpSocket, sender: &Sender<Action>, stats: &OscStats, stop: &AtomicBool) {
    let mut buffer = vec![0_u8; MAX_DATAGRAM];
    while !stop.load(Ordering::Relaxed) {
        let len = match socket.recv_from(&mut buffer) {
            Ok((len, _)) => len,
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => continue,
            // Windows signale ainsi un ICMP « port injoignable » d'un envoi précédent : sans importance.
            Err(e) if e.kind() == io::ErrorKind::ConnectionReset => continue,
            Err(e) => {
                tracing::error!(error = %e, "OSC : lecture impossible, serveur arrêté");
                return;
            }
        };
        match rosc::decoder::decode_udp(&buffer[..len]) {
            Ok((_, packet)) => handle(&packet, sender, stats),
            Err(_) => {
                stats.malformed.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

fn handle(packet: &OscPacket, sender: &Sender<Action>, stats: &OscStats) {
    match packet {
        OscPacket::Message(message) => match action_from_message(message) {
            Some(action) => {
                stats.accepted.fetch_add(1, Ordering::Relaxed);
                if let Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) = sender.try_send(action) {
                    stats.dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
            None => {
                stats.ignored.fetch_add(1, Ordering::Relaxed);
            }
        },
        OscPacket::Bundle(bundle) => {
            for inner in &bundle.content {
                handle(inner, sender, stats);
            }
        }
    }
}

/// Lit un argument comme un nombre (entier, flottant ou booléen : de nombreuses applis OSC envoient 0/1).
fn number(arg: &OscType) -> Option<f32> {
    match arg {
        OscType::Float(v) => Some(*v),
        #[allow(
            clippy::cast_possible_truncation,
            reason = "OSC double vers f32 : la précision perdue est sans effet"
        )]
        OscType::Double(v) => Some(*v as f32),
        #[allow(clippy::cast_precision_loss, reason = "entiers OSC courts (rangs, 0/1)")]
        OscType::Int(v) => Some(*v as f32),
        OscType::Bool(b) => Some(f32::from(u8::from(*b))),
        _ => None,
    }
}

/// Le rang entier du premier argument, rejeté s'il est négatif ou fractionnaire.
fn index(args: &[OscType]) -> Option<u16> {
    match args.first()? {
        OscType::Int(v) => u16::try_from(*v).ok(),
        other => {
            let v = number(other)?;
            (v.fract() == 0.0 && (0.0..=f32::from(u16::MAX)).contains(&v)).then_some(v as u16)
        }
    }
}

/// Bascule : sans argument `Toggle`, sinon vrai (≠ 0) = `On`, faux = `Off`.
fn switch(args: &[OscType]) -> Option<Switch> {
    match args.first() {
        None => Some(Switch::Toggle),
        Some(arg) => Some(if number(arg)? == 0.0 { Switch::Off } else { Switch::On }),
    }
}

fn level(args: &[OscType]) -> Option<Level> {
    let v = number(args.first()?)?;
    v.is_finite().then(|| Level::Set(v.clamp(0.0, 1.0)))
}

/// Traduit un message OSC en action, ou `None` s'il n'est pas compris.
///
/// Toutes les adresses commencent par `/cf/`. Les rangs (cue, DJ, macro) comptent à partir de 0, comme dans les
/// fichiers de show. Voir `docs/osc.md` pour la liste complète.
#[must_use]
pub fn action_from_message(message: &OscMessage) -> Option<Action> {
    let path = message.addr.strip_prefix("/cf/")?;
    let args = message.args.as_slice();
    let mut parts = path.split('/');
    let (head, tail) = (parts.next()?, parts.next());
    if parts.next().is_some() {
        return None;
    }
    match (head, tail) {
        ("go", None) => Some(Action::Go),
        ("back", None) => Some(Action::Back),
        ("cue", None) => Some(Action::GotoCue(CueIndex(index(args)?))),
        ("hold", None) => Some(Action::Hold(switch(args)?)),
        ("dj", Some("next")) => Some(Action::NextDj),
        ("dj", Some("prev")) => Some(Action::PrevDj),
        ("dj", Some("arm")) => Some(Action::ArmDj(DjIndex(index(args)?))),
        ("dj", Some("show")) => Some(Action::ShowDj),
        ("drop", None) => Some(Action::ForceDrop),
        ("shot", Some("skip")) => Some(Action::SkipShot),
        ("event", Some("random")) => Some(Action::RandomEvent),
        ("ascension", None) => Some(Action::Ascension),
        ("master", None) => Some(Action::Master(level(args)?)),
        ("macro", Some(i)) => {
            let index = i.parse::<u8>().ok().filter(|i| *i < MACRO_COUNT)?;
            Some(Action::Macro {
                index: MacroIndex(index),
                value: level(args)?,
            })
        }
        ("layer", Some(name)) => {
            let layer = match name {
                "background" => Layer::Background,
                "world" => Layer::World,
                "foreground" => Layer::Foreground,
                "overlay" => Layer::Overlay,
                _ => return None,
            };
            Some(Action::LayerFade {
                layer,
                value: level(args)?,
            })
        }
        ("blackout", None) => Some(Action::Blackout(switch(args)?)),
        ("safelook", None) => Some(Action::SafeLook),
        ("flashkill", None) => Some(Action::FlashKill(switch(args)?)),
        ("breathing", None) => Some(Action::Breathing(switch(args)?)),
        ("tap", None) => Some(Action::TapTempo),
        ("resync", None) => Some(Action::Resync),
        ("nudge", None) => {
            let beats = number(args.first()?)?;
            beats.is_finite().then_some(Action::NudgePhase { beats })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(addr: &str, args: Vec<OscType>) -> OscMessage {
        OscMessage {
            addr: addr.to_owned(),
            args,
        }
    }

    #[test]
    fn simple_addresses() {
        assert_eq!(action_from_message(&msg("/cf/go", vec![])), Some(Action::Go));
        assert_eq!(action_from_message(&msg("/cf/dj/next", vec![])), Some(Action::NextDj));
        assert_eq!(
            action_from_message(&msg("/cf/cue", vec![OscType::Int(3)])),
            Some(Action::GotoCue(CueIndex(3)))
        );
    }

    #[test]
    fn switches_default_to_toggle_and_read_booleans_or_numbers() {
        assert_eq!(
            action_from_message(&msg("/cf/hold", vec![])),
            Some(Action::Hold(Switch::Toggle))
        );
        assert_eq!(
            action_from_message(&msg("/cf/blackout", vec![OscType::Float(1.0)])),
            Some(Action::Blackout(Switch::On))
        );
        assert_eq!(
            action_from_message(&msg("/cf/blackout", vec![OscType::Bool(false)])),
            Some(Action::Blackout(Switch::Off))
        );
    }

    #[test]
    fn levels_are_clamped_and_macros_are_range_checked() {
        assert_eq!(
            action_from_message(&msg("/cf/macro/2", vec![OscType::Float(1.7)])),
            Some(Action::Macro {
                index: MacroIndex(2),
                value: Level::Set(1.0)
            })
        );
        assert_eq!(
            action_from_message(&msg("/cf/macro/8", vec![OscType::Float(0.5)])),
            None
        );
        assert_eq!(
            action_from_message(&msg("/cf/master", vec![OscType::Float(f32::NAN)])),
            None
        );
    }

    #[test]
    fn bad_addresses_and_arguments_are_rejected() {
        assert_eq!(action_from_message(&msg("/other/go", vec![])), None);
        assert_eq!(action_from_message(&msg("/cf/go/extra/deep", vec![])), None);
        assert_eq!(action_from_message(&msg("/cf/cue", vec![OscType::Int(-1)])), None);
        assert_eq!(action_from_message(&msg("/cf/cue", vec![OscType::Float(1.5)])), None);
        assert_eq!(
            action_from_message(&msg("/cf/cue", vec![OscType::String("2".into())])),
            None
        );
    }

    #[test]
    fn udp_loopback_delivers_actions_and_survives_garbage() {
        let osc = OscInput::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        client.send_to(b"pas de l'osc", osc.local_addr()).unwrap();
        for (addr, args) in [("/cf/go", vec![]), ("/cf/macro/1", vec![OscType::Float(0.25)])] {
            let packet = OscPacket::Message(msg(addr, args));
            let bytes = rosc::encoder::encode(&packet).unwrap();
            client.send_to(&bytes, osc.local_addr()).unwrap();
        }
        let mut received = Vec::new();
        for _ in 0..100 {
            while let Some(action) = osc.try_recv() {
                received.push(action);
            }
            if received.len() == 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            received,
            [
                Action::Go,
                Action::Macro {
                    index: MacroIndex(1),
                    value: Level::Set(0.25)
                }
            ]
        );
        assert_eq!(osc.stats().malformed.load(Ordering::Relaxed), 1);
    }
}
