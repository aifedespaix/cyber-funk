//! Ce que l'interface (et le binaire qui assemble les plugins) apprend du spectacle : `ShowEvent` de Sillage, étendu
//! aux cues et aux créneaux. Pas d'écoute image par image : pour cela, lire `ShowState`.

use std::sync::Arc;

use crate::model::CueEntry;

/// Pourquoi un cue est appelé ou entre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CueCause {
    /// GO de l'opérateur (ou d'une note MIDI du DJ).
    Go,
    /// Retour au cue précédent.
    Back,
    /// Saut à un cue choisi (`GotoCue`).
    Goto,
    /// Règle d'avancée du cue précédent (modes Auto et Semi).
    Auto,
    /// Premier cue du créneau suivant, à la fin d'une passation.
    Handover,
    /// Ouverture du show, ou reprise après un arrêt.
    Open,
}

/// Les bascules que l'interface affiche.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShowToggle {
    /// L'avancée automatique est figée.
    Hold,
    /// Noir demandé (le noir lui-même glisse, voir `ShowState::blackout`).
    Blackout,
    /// Repli sur le look de secours.
    SafeLook,
    /// Flashs permis (faux : `FlashKill`).
    Flashes,
    /// Mode respiration.
    Breathing,
}

/// Un fait du spectacle, publié une fois. Vidé par `ShowDirector::drain_events`.
#[derive(Debug, Clone, PartialEq)]
pub enum ShowEvent {
    /// Un cue est appelé et attend son entrée (drop, phrase, temps fort) : l'interface affiche « entrée : au drop ».
    CueCalled {
        slot: usize,
        cue: usize,
        entry: CueEntry,
        cause: CueCause,
    },
    /// Un cue entre et devient le cue courant (`level` de Sillage).
    CueEntered {
        slot: usize,
        cue: usize,
        previous_slot: usize,
        previous_cue: usize,
        cause: CueCause,
    },
    /// Préparer ce créneau (logo, enseigne, textures du nom) : il a été armé ou une passation vers lui commence.
    /// Rien ne se charge depuis le disque pendant le show (tout est préchargé à l'ouverture) : c'est une mise en
    /// place, pas un chargement.
    Preload { slot: usize },
    /// La passation commence sur un temps fort : fondu de palette sur `fade_s`, logo du DJ suivant en avant-plan,
    /// enseigne.
    Handover { from: usize, to: usize, fade_s: f32 },
    /// Le créneau est chargé : appliquer son profil audio et ses profils de contrôle. Le changement de profil MIDI
    /// passe par `cf_control::Mapper::reload`, qui relâche proprement les boutons tenus (un Blackout tenu pendant la
    /// passation s'éteint) ; ses actions de relâchement sont à rendre au chef du spectacle à l'image suivante.
    SlotLoaded { slot: usize, previous: usize },
    /// Rappel du DJ (`showDj` de Sillage, `replay: true`).
    ShowDj { slot: usize },
    /// Logo du DJ en avant-plan (déclencheur de cue `DjLogo`).
    DjLogo { slot: usize },
    /// Carton-titre à afficher (déclencheur de cue, action `Title`, ou titre d'un drop).
    Title(Arc<str>),
    /// Une bascule a changé.
    Toggle { what: ShowToggle, on: bool },
}
