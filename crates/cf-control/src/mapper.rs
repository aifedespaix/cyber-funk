//! La couche de correspondance : entrées brutes → [`Action`].
//!
//! Le [`Mapper`] tient la table effective, fusion de plusieurs profils empilés (typiquement : clavier par défaut,
//! puis profil de l'événement, puis profil du créneau DJ en cours). Un profil plus haut dans la pile l'emporte sur les
//! gestes qu'il redéfinit ; ces remplacements sont rendus au chargement ([`Override`]) pour que l'interface les
//! montre. Deux bindings du même profil qui se disputent un geste, eux, sont une erreur (voir
//! [`ControlProfile::issues`]).
//!
//! [`Mapper::map`] n'alloue pas : parcours linéaire de la table (quelques dizaines à quelques centaines de lignes),
//! et les identifiants des actions sont partagés (`Arc<str>`).

use crate::action::{Action, Level, Switch};
use crate::error::Result;
use crate::profile::{Binding, ControlProfile};
use crate::raw::{PortId, RawEvent, RawInput};
use crate::source::InputSource;
use crate::template::{ActionKind, ActionTemplate, Mode, RELATIVE_LEVEL_STEP};

/// Seuil d'une commande continue lue comme un bouton (CC 0..=127 : 64 et plus = enfoncé ; OSC : 0,5 et plus).
const CC_PRESS_THRESHOLD: u8 = 64;
const OSC_PRESS_THRESHOLD: f32 = 0.5;
/// Plus grande valeur d'un CC 14 bits (2^14 − 1).
const WIDE_CC_MAX: f32 = 16383.0;
/// Seuil d'une position absolue qui commande une bascule.
const SWITCH_LEVEL_THRESHOLD: f32 = 0.5;

/// Une action et l'instant de l'entrée qui l'a produite (secondes, horloge de [`RawInput::time_s`]).
#[derive(Debug, Clone, PartialEq)]
pub struct TimedAction {
    pub time_s: f64,
    pub action: Action,
}

/// Un binding d'un profil inférieur masqué par un binding d'un profil supérieur, au chargement.
#[derive(Debug, Clone, PartialEq)]
pub struct Override {
    /// La source du binding masqué.
    pub source: InputSource,
    pub replaced: ActionTemplate,
    pub replaced_profile: String,
    pub by: ActionTemplate,
    pub by_profile: String,
}

/// Une ligne de la table effective et son état de lecture.
#[derive(Debug, Clone)]
struct Entry {
    binding: Binding,
    /// Dernier état « haut/bas » vu, pour ne réagir qu'aux fronts quand une commande continue sert de bouton
    /// (un bouton en CC peut renvoyer 127 plusieurs fois). `None` tant que rien n'est arrivé.
    high: Option<bool>,
    /// Les deux moitiés du dernier CC 14 bits reçu (sans effet pour les autres sources).
    wide: WideCc,
}

/// Recomposition d'un CC 14 bits : octet de poids fort sur `cc`, de poids faible sur `cc + 32`.
///
/// On émet à l'arrivée de chaque moitié (avec l'autre moitié telle qu'on la connaît) plutôt que d'attendre la paire :
/// certains contrôleurs n'envoient l'octet de poids faible que s'il a changé, et attendre la paire figerait le
/// potard. Contrepartie : entre l'octet de poids fort et l'octet de poids faible, la valeur peut être fausse de
/// 127/16383 (0,8 %) au plus, pendant moins d'une milliseconde.
#[derive(Debug, Clone, Copy, Default)]
struct WideCc {
    msb: u8,
    lsb: u8,
    last: Option<u16>,
}

impl WideCc {
    /// Intègre une moitié ; rend la position 0..1 si elle a changé.
    fn update(&mut self, is_msb: bool, value: u8) -> Option<f32> {
        if is_msb {
            self.msb = value & 0x7F;
        } else {
            self.lsb = value & 0x7F;
        }
        let v = u16::from(self.msb) << 7 | u16::from(self.lsb);
        (self.last != Some(v)).then(|| {
            self.last = Some(v);
            f32::from(v) / WIDE_CC_MAX
        })
    }
}

/// Traduit les [`RawInput`] en [`TimedAction`] selon une pile de profils.
#[derive(Debug, Clone, Default)]
pub struct Mapper {
    entries: Vec<Entry>,
    /// Nom de chaque port MIDI ouvert, indexé par [`PortId`].
    ports: Vec<Option<String>>,
}

/// Ce qu'une entrée brute signifie pour un binding donné, avant de devenir une action.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Signal {
    Press,
    Release,
    /// Position 0..1.
    Level(f32),
    /// Décalage signé, en crans.
    Delta(f32),
}

impl Mapper {
    /// Un mapper sans aucun binding.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Un mapper chargé avec cette pile de profils (du plus bas au plus haut).
    ///
    /// # Errors
    /// Voir [`Mapper::load`].
    pub fn from_profiles(profiles: &[&ControlProfile]) -> Result<(Self, Vec<Override>)> {
        let mut mapper = Self::new();
        let overrides = mapper.load(profiles)?;
        Ok((mapper, overrides))
    }

    /// Remplace la table par la fusion de ces profils, du plus bas au plus haut dans la pile (le dernier gagne sur
    /// les gestes partagés). Les noms de ports déjà connus sont gardés. Appelé hors du show ou à la passation d'un
    /// DJ, jamais par image : la fusion alloue.
    ///
    /// # Errors
    /// [`Error::InvalidProfile`](crate::Error::InvalidProfile) pour le premier profil invalide ; la table en place
    /// reste alors intacte.
    pub fn load(&mut self, profiles: &[&ControlProfile]) -> Result<Vec<Override>> {
        let mut entries: Vec<(Entry, usize)> = Vec::new();
        let mut overrides = Vec::new();
        for (p, profile) in profiles.iter().enumerate() {
            profile.validate()?;
            for binding in &profile.bindings {
                entries.retain(|(old, owner)| {
                    let masked = old.binding.source.overlaps(&binding.source);
                    if masked {
                        overrides.push(Override {
                            source: old.binding.source.clone(),
                            replaced: old.binding.action.clone(),
                            replaced_profile: profiles[*owner].name.clone(),
                            by: binding.action.clone(),
                            by_profile: profile.name.clone(),
                        });
                    }
                    !masked
                });
                entries.push((
                    Entry {
                        binding: binding.clone(),
                        high: None,
                        wide: WideCc::default(),
                    },
                    p,
                ));
            }
        }
        self.entries = entries.into_iter().map(|(entry, _)| entry).collect();
        Ok(overrides)
    }

    /// La table effective, après fusion.
    pub fn bindings(&self) -> impl Iterator<Item = &Binding> {
        self.entries.iter().map(|e| &e.binding)
    }

    /// Déclare (`Some`) ou retire (`None`) le nom du port MIDI `port`. Appelé par la couche MIDI à chaque
    /// (re)connexion, hors du callback temps réel.
    pub fn set_port_name(&mut self, port: PortId, name: Option<&str>) {
        let i = usize::from(port.0);
        if self.ports.len() <= i {
            self.ports.resize(i + 1, None);
        }
        self.ports[i] = name.map(str::to_owned);
    }

    /// Le nom du port MIDI ouvert `port`, s'il est connu (le MIDI learn s'en sert pour nommer la source).
    #[must_use]
    pub fn port_name(&self, port: PortId) -> Option<&str> {
        self.ports.get(usize::from(port.0)).and_then(|p| p.as_deref())
    }

    /// L'action produite par cette entrée, ou `None` si aucun binding ne la reconnaît ou si elle ne change rien
    /// (relâchement d'un bouton bascule, répétition automatique d'une touche, encodeur immobile…).
    pub fn map(&mut self, input: &RawInput) -> Option<TimedAction> {
        let ports = &self.ports;
        let port_name = |id: PortId| ports.get(usize::from(id.0)).and_then(|p| p.as_deref());
        // Au plus une ligne peut correspondre : les conflits sont refusés au chargement et la fusion retire les
        // lignes masquées.
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.binding.source.matches(&input.event, port_name))?;
        let signal = read(entry, &input.event)?;
        let action = produce(entry, signal)?;
        Some(TimedAction {
            time_s: input.time_s,
            action,
        })
    }
}

/// Lit l'entrée brute selon la source et le mode du binding.
fn read(entry: &mut Entry, event: &RawEvent) -> Option<Signal> {
    let mode = entry.binding.mode;
    match event {
        RawEvent::Key(key) => (!key.repeat).then(|| button(mode, key.pressed, 1.0)),
        RawEvent::MidiNote { velocity, .. } => Some(button(mode, *velocity > 0, f32::from(*velocity) / 127.0)),
        RawEvent::MidiProgram { .. } => Some(button(mode, true, 1.0)),
        RawEvent::MidiCc { cc, value, .. } => match mode {
            Mode::Absolute => match entry.binding.source {
                InputSource::MidiCc14 { cc: msb, .. } => entry.wide.update(*cc == msb, *value).map(Signal::Level),
                _ => Some(Signal::Level(f32::from(*value) / 127.0)),
            },
            Mode::Relative => {
                let InputSource::MidiCc { encoding, .. } = entry.binding.source else {
                    return None;
                };
                Some(Signal::Delta(f32::from(encoding.decode(*value))))
            }
            Mode::Momentary | Mode::Toggle => edge(&mut entry.high, *value >= CC_PRESS_THRESHOLD)
                .map(|on| if on { Signal::Press } else { Signal::Release }),
        },
        RawEvent::Osc { value: None, .. } => Some(Signal::Press),
        RawEvent::Osc { value: Some(v), .. } if v.is_finite() => match mode {
            Mode::Absolute => Some(Signal::Level(v.clamp(0.0, 1.0))),
            Mode::Relative => Some(Signal::Delta(*v)),
            Mode::Momentary | Mode::Toggle => edge(&mut entry.high, *v >= OSC_PRESS_THRESHOLD)
                .map(|on| if on { Signal::Press } else { Signal::Release }),
        },
        RawEvent::Osc { .. } => None,
    }
}

/// Un bouton (touche, note) : appui/relâchement, ou position en mode absolu (vélocité à l'appui, 0 au relâchement).
fn button(mode: Mode, pressed: bool, velocity: f32) -> Signal {
    match (mode, pressed) {
        (Mode::Absolute, true) => Signal::Level(velocity),
        (Mode::Absolute, false) => Signal::Level(0.0),
        (_, true) => Signal::Press,
        (_, false) => Signal::Release,
    }
}

/// Front montant ou descendant d'une commande lue comme un bouton : `Some(état)` au changement, `None` sinon. Un
/// premier état « bas » n'est pas un relâchement (rien n'avait été enfoncé) : il est noté sans rien produire, pour
/// qu'effleurer un potard au repos n'éteigne pas une bascule allumée depuis l'interface.
fn edge(high: &mut Option<bool>, now: bool) -> Option<bool> {
    let changed = match *high {
        Some(was) => was != now,
        None => now,
    };
    *high = Some(now);
    changed.then_some(now)
}

/// Transforme le signal en action selon la forme de valeur et le mode (combinaisons validées au chargement ; une
/// combinaison sans effet rend `None`).
fn produce(entry: &mut Entry, signal: Signal) -> Option<Action> {
    let action = &entry.binding.action;
    match (action.kind(), entry.binding.mode, signal) {
        (ActionKind::Impulse, _, Signal::Press) => action.impulse(),
        (ActionKind::Switch, Mode::Momentary, Signal::Press) => action.with_switch(Switch::On),
        (ActionKind::Switch, Mode::Momentary, Signal::Release) => action.with_switch(Switch::Off),
        (ActionKind::Switch, Mode::Toggle, Signal::Press) => action.with_switch(Switch::Toggle),
        (ActionKind::Switch, Mode::Absolute, Signal::Level(v)) => edge(&mut entry.high, v >= SWITCH_LEVEL_THRESHOLD)
            .and_then(|on| action.with_switch(if on { Switch::On } else { Switch::Off })),
        (ActionKind::Level, Mode::Absolute, Signal::Level(v)) => action.with_level(Level::Set(v)),
        (ActionKind::Level, Mode::Relative, Signal::Delta(d)) if d != 0.0 => {
            action.with_level(Level::Shift(d * RELATIVE_LEVEL_STEP))
        }
        (ActionKind::Level, Mode::Momentary, Signal::Press) => action.with_level(Level::Set(1.0)),
        (ActionKind::Level, Mode::Momentary, Signal::Release) => action.with_level(Level::Set(0.0)),
        (ActionKind::Amount, Mode::Momentary, Signal::Press) => action.with_amount(1.0),
        (ActionKind::Amount, Mode::Relative, Signal::Delta(d)) if d != 0.0 => action.with_amount(d),
        _ => None,
    }
}
