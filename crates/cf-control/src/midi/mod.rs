//! Entrée MIDI : ports `midir`, analyseur d'octets, canal borné vers l'application, rebranchement à chaud, MIDI learn.
//!
//! ```text
//! contrôleur ─▶ callback midir (thread de l'hôte, temps réel)        thread cf-midi (superviseur)
//!                 InputCallback::handle                                scrute les ports toutes les 2 s,
//!                   MidiParser ─▶ MidiEvent { time_s, port, msg }      ouvre / referme / rouvre par nom,
//!                        │  try_send, sans alloc ni verrou ni log      annonce chaque port dans le même flux
//!                        ▼                                                    │
//!                 canal borné de MidiItem ◀──────────────────────────────────┘
//!                        │  MidiInput::try_recv (une fois par image, sans attendre)
//!                        ▼
//!                 dispatch(item, mapper, learner) ─▶ Action | Learned | Port | Other
//! ```
//!
//! Le callback ne fait que décoder, horodater et pousser : zéro allocation (vérifié sous `assert_no_alloc`), zéro
//! verrou, zéro journal. Les noms de ports ne le traversent pas : il porte un [`PortId`](crate::PortId), et le flux
//! annonce `PortEvent::Connected { port, name }` *avant* le premier message du port et `Disconnected` *après* le dernier.

mod backend;
mod callback;
mod dispatch;
mod event;
mod fake;
mod input;
mod learn;
mod message;
mod midir_backend;
mod port_name;
mod supervisor;

pub use backend::MidiBackend;
pub use callback::{InputCallback, MidiStats};
pub use dispatch::{Dispatch, dispatch};
pub use event::MidiEvent;
pub use fake::{FakeBackend, FakeConnection, FakeMidi};
pub use input::{
    ConnectedPort, DEFAULT_QUEUE_CAPACITY, DEFAULT_SCAN_PERIOD, MidiConfig, MidiInput, MidiItem, PortEvent,
    PortSelection, PortsSnapshot,
};
pub use learn::Learner;
pub use message::{MidiMessage, MidiParser};
pub use midir_backend::MidirBackend;
pub use port_name::{PortMatch, canonical_port_name, match_port_name, port_names_match};
