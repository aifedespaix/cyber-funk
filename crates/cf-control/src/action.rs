//! Le vocabulaire `Action` : tout ce que l'opérateur, le DJ ou un logiciel de mix peut demander au spectacle.
//!
//! Les `Action` sont produites par le [`Mapper`](crate::Mapper) (clavier, MIDI, OSC) ou directement par l'interface
//! opérateur, et consommées par `cf-show`. Elles ne portent jamais l'état du spectacle : une bascule dit « allume »,
//! « éteins » ou « inverse » ([`Switch`]), un niveau dit « mets à » ou « décale de » ([`Level`]). C'est `cf-show`, seul
//! détenteur de l'état, qui résout. Ainsi un pad MIDI, une touche et un bouton de l'interface qui agissent sur la même
//! bascule ne se désynchronisent jamais.

use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Nombre de macros (boutons rotatifs assignables) d'un look : `MacroIndex` va de 0 à `MACRO_COUNT - 1`.
/// Repris de `docs/ARCHITECTURE.md` §2.2 (« jusqu'à 8 boutons rotatifs assignables »).
pub const MACRO_COUNT: u8 = 8;

/// Rang d'un cue dans la liste du créneau en cours, à partir de 0 (le cue 0 de la « Montée automatique » est le
/// niveau 0 de Sillage, la pré-intro). Un rang hors de la liste est ignoré par `cf-show`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CueIndex(pub u16);

/// Rang d'un créneau DJ dans l'ordre de passage de l'événement, à partir de 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DjIndex(pub u16);

/// Rang d'une macro du look en cours, de 0 à [`MACRO_COUNT`] − 1 (vérifié au chargement d'un profil).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MacroIndex(pub u8);

/// Identifiant d'un carton-titre déclaré dans le fichier de show ; `cf-show` le résout en texte.
///
/// `Arc<str>` : cloner l'identifiant (à chaque déclenchement d'un binding) ne coûte qu'un compteur, jamais une
/// allocation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TitleId(pub Arc<str>);

/// Identifiant d'un déclencheur ponctuel du monde (événement du trafic, cascade…), par exemple `"storm"` ou
/// `"convoy"`. `cf-show` le transmet ; un identifiant inconnu du monde en cours est ignoré.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TriggerId(pub Arc<str>);

impl TitleId {
    /// Construit un identifiant (alloue : à faire au chargement, pas pendant le show).
    #[must_use]
    pub fn new(id: &str) -> Self {
        Self(Arc::from(id))
    }
}

impl TriggerId {
    /// Construit un identifiant (alloue : à faire au chargement, pas pendant le show).
    #[must_use]
    pub fn new(id: &str) -> Self {
        Self(Arc::from(id))
    }
}

/// Les couches de rendu empilées (`docs/ARCHITECTURE.md` §1.5), dans l'ordre de dessin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Layer {
    /// Ciel, étoiles, planètes : dessinée en premier, à l'infini.
    Background,
    /// La ville, la route, la voiture, le trafic.
    World,
    /// Objets en repère caméra (logo du DJ en hologramme, pluie sur l'objectif), jamais coupés par le monde.
    Foreground,
    /// Surcouche 2D : cartons-titres, nom du DJ.
    Overlay,
}

/// Ordre donné à une bascule. Le détenteur de l'état (`cf-show`) le résout avec [`Switch::apply`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Switch {
    On,
    Off,
    /// Inverse l'état courant (touche ou pad en mode bascule).
    Toggle,
}

impl Switch {
    /// Le nouvel état de la bascule, à partir de l'état courant.
    #[must_use]
    pub fn apply(self, current: bool) -> bool {
        match self {
            Self::On => true,
            Self::Off => false,
            Self::Toggle => !current,
        }
    }
}

/// Ordre donné à une grandeur continue normalisée 0..1. Le détenteur de la valeur la résout avec [`Level::apply`].
///
/// Un potard envoie une position (`Set`), un encodeur sans fin envoie un décalage (`Shift`) : le décalage s'applique
/// à la valeur réelle du spectacle, pas à une copie tenue par le contrôleur, donc aucun saut quand un cue ou
/// l'interface a changé la valeur entre-temps.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Level {
    /// Position absolue, 0..1.
    Set(f32),
    /// Décalage signé, en fraction de la course totale (+0,1 = un dixième vers le haut).
    Shift(f32),
}

impl Level {
    /// La nouvelle valeur, bornée à 0..1. Une valeur non finie (NaN, infini) laisse la valeur courante intacte.
    #[must_use]
    pub fn apply(self, current: f32) -> f32 {
        let next = match self {
            Self::Set(v) => v,
            Self::Shift(d) => current + d,
        };
        if next.is_finite() {
            next.clamp(0.0, 1.0)
        } else {
            current
        }
    }
}

/// Les familles d'actions de `docs/ARCHITECTURE.md` §2.4, plus les commandes de l'application. Sert à ranger les
/// actions dans l'interface (MIDI learn, panneau des raccourcis).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionFamily {
    /// Conduite de la liste de cues et des créneaux.
    Conduct,
    /// Ordres ponctuels au réalisateur et au monde.
    OneShot,
    /// Grandeurs continues (macros, master, couches).
    Continuous,
    /// Sécurité du public et replis.
    Safety,
    /// Tempo et phase.
    Tempo,
    /// Commandes de l'application, sans effet sur l'image projetée.
    App,
}

/// Une demande adressée au spectacle. Le moteur ne connaît que ce type : clavier, MIDI, OSC et interface opérateur
/// passent tous par lui.
///
/// Correspondance avec `OperatorAction` de Sillage (`engine/show/types.ts`) : `forceDrop` → `ForceDrop`,
/// `skipShot` → `SkipShot`, `nextLevel`/`prevLevel` → `Go`/`Back`, `gotoLevel:N` → `GotoCue(N)`,
/// `nextDj`/`prevDj` → `NextDj`/`PrevDj`, `showDj` → `ShowDj`, `ascension` → `Ascension`,
/// `randomEvent` → `RandomEvent`, `toggleFlashes` → `FlashKill(Toggle)`, `toggleBreathing` → `Breathing(Toggle)`,
/// `toggleOperatorHud` → `OperatorOverlay(Toggle)`. La commande de page `togglePlay` devient `Playback(Toggle)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    // --- Conduite ---
    /// Passe au cue suivant (bouton GO).
    Go,
    /// Revient au cue précédent.
    Back,
    /// Saute au cue de ce rang dans le créneau en cours.
    GotoCue(CueIndex),
    /// Fige l'avancée automatique (règles de temps, de drop, de mesures) ; GO reste possible.
    Hold(Switch),
    /// Passe au créneau DJ suivant (joue la passation vers le DJ armé, sinon vers le suivant dans l'ordre).
    NextDj,
    /// Revient au créneau DJ précédent.
    PrevDj,
    /// Arme ce créneau : le prochain `NextDj` y mène (passation hors de l'ordre prévu).
    ArmDj(DjIndex),

    // --- Ponctuels ---
    /// Déclenche un drop tout de suite, sans montée.
    ForceDrop,
    /// Coupe au plan caméra suivant.
    SkipShot,
    /// Plan sur l'enseigne du DJ et rappel de son nom.
    ShowDj,
    /// Affiche ce carton-titre.
    Title(TitleId),
    /// Lance ce déclencheur du monde (événement du trafic, cascade…).
    Trigger(TriggerId),
    /// Lance un événement du trafic au hasard parmi ceux que le cue permet.
    RandomEvent,
    /// Lance l'Ascension (si le cue la permet).
    Ascension,

    // --- Continus ---
    /// Règle une macro du look en cours.
    Macro { index: MacroIndex, value: Level },
    /// Intensité globale du spectacle.
    Master(Level),
    /// Opacité d'une couche de rendu.
    LayerFade { layer: Layer, value: Level },

    // --- Sécurité ---
    /// Noir total sur le projecteur.
    Blackout(Switch),
    /// Repli immédiat sur le look de secours (calme).
    SafeLook,
    /// Coupe tous les flashs (le limiteur photosensible reste actif dans tous les cas).
    FlashKill(Switch),
    /// Mode respiration : moins d'effets, plans lents.
    Breathing(Switch),

    // --- Tempo ---
    /// Un appui de tap tempo, horodaté par le [`TimedAction`](crate::TimedAction) qui le porte.
    TapTempo,
    /// Recale le temps 1 de la mesure sur le prochain kick.
    Resync,
    /// Décale la phase, en temps (beats), signé : négatif = en arrière.
    NudgePhase { beats: f32 },

    // --- Application ---
    /// Lecture/pause du lecteur de démo (`Maj+Espace` dans Sillage).
    Playback(Switch),
    /// Surcouche d'informations dans la fenêtre opérateur, jamais sur le projecteur (touche O de Sillage).
    OperatorOverlay(Switch),
}

impl Action {
    /// La famille de l'action, pour l'interface.
    #[must_use]
    pub fn family(&self) -> ActionFamily {
        match self {
            Self::Go | Self::Back | Self::GotoCue(_) | Self::Hold(_) | Self::NextDj | Self::PrevDj | Self::ArmDj(_) => {
                ActionFamily::Conduct
            }
            Self::ForceDrop
            | Self::SkipShot
            | Self::ShowDj
            | Self::Title(_)
            | Self::Trigger(_)
            | Self::RandomEvent
            | Self::Ascension => ActionFamily::OneShot,
            Self::Macro { .. } | Self::Master(_) | Self::LayerFade { .. } => ActionFamily::Continuous,
            Self::Blackout(_) | Self::SafeLook | Self::FlashKill(_) | Self::Breathing(_) => ActionFamily::Safety,
            Self::TapTempo | Self::Resync | Self::NudgePhase { .. } => ActionFamily::Tempo,
            Self::Playback(_) | Self::OperatorOverlay(_) => ActionFamily::App,
        }
    }
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Background => "arrière-plan",
            Self::World => "monde",
            Self::Foreground => "avant-plan",
            Self::Overlay => "surcouche 2D",
        };
        f.write_str(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_resolves_against_current_state() {
        assert!(Switch::On.apply(false));
        assert!(!Switch::Off.apply(true));
        assert!(Switch::Toggle.apply(false));
        assert!(!Switch::Toggle.apply(true));
    }

    #[test]
    fn level_is_clamped_and_ignores_non_finite() {
        assert!((Level::Set(0.4).apply(0.9) - 0.4).abs() < f32::EPSILON);
        assert!((Level::Shift(0.3).apply(0.9) - 1.0).abs() < f32::EPSILON);
        assert!(Level::Shift(-2.0).apply(0.5).abs() < f32::EPSILON);
        assert!((Level::Set(f32::NAN).apply(0.25) - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn cloning_an_id_shares_the_string() {
        let a = Action::Trigger(TriggerId::new("storm"));
        let b = a.clone();
        let (Action::Trigger(x), Action::Trigger(y)) = (&a, &b) else {
            unreachable!("les deux actions sont des Trigger");
        };
        assert!(Arc::ptr_eq(&x.0, &y.0));
    }
}
