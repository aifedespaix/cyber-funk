//! Le scénario d'une soirée : des actions de l'opérateur et des gestes de la musique, à des heures du set.

use cf_control::Action;

use super::SyntheticMusic;

/// Une chose qui arrive à une heure du set.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Action de l'opérateur ou du DJ, remise au chef du spectacle.
    Act(Action),
    /// Le DJ change de tempo (BPM).
    Tempo(f32),
    /// La source audio apparaît ou disparaît.
    Live(bool),
    /// Un drop réel frappe, hors de la structure prévue.
    Strike,
    /// Un nouveau morceau commence (la structure repart du début).
    NewTrack,
}

/// Commandes triées par heure (secondes depuis le début du set).
#[derive(Debug, Clone, Default)]
pub struct Script {
    items: Vec<(f64, Command)>,
    next: usize,
}

impl Script {
    /// Scénario vide : la musique joue, personne ne touche à rien.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ajoute une commande à l'heure `at_s` (s). L'ordre d'ajout est gardé pour une même heure.
    #[must_use]
    pub fn at(mut self, at_s: f64, command: Command) -> Self {
        let index = self.items.partition_point(|(t, _)| *t <= at_s);
        self.items.insert(index, (at_s, command));
        self
    }

    /// Ajoute une action à l'heure `at_s`.
    #[must_use]
    pub fn act(self, at_s: f64, action: Action) -> Self {
        self.at(at_s, Command::Act(action))
    }

    /// Nombre de commandes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Vrai sans aucune commande.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Applique les commandes dues à l'heure `now_s` : les gestes de la musique tout de suite, les actions
    /// ajoutées à `actions` (que l'appelant vide après chaque image).
    pub(crate) fn due(&mut self, now_s: f64, music: &mut SyntheticMusic, actions: &mut Vec<Action>) {
        while let Some((at, command)) = self.items.get(self.next) {
            if *at > now_s {
                break;
            }
            match command {
                Command::Act(action) => actions.push(action.clone()),
                Command::Tempo(bpm) => music.set_bpm(*bpm),
                Command::Live(live) => music.set_live(*live),
                Command::Strike => music.strike(),
                Command::NewTrack => music.new_track(),
            }
            self.next += 1;
        }
    }
}
