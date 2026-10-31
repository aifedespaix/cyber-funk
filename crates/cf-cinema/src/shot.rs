//! Le contrat des plans (`Shot` / `ShotSpec` de `cinema/types.ts`).
//!
//! Le catalogue est une liste de fiches [`ShotSpec`] (des valeurs statiques) ; quand le réalisateur choisit une fiche,
//! elle fabrique un [`Shot`] vivant qui écrit la pose à chaque image. Les plans ne connaissent ni la musique (au-delà de
//! leurs repères), ni le catalogue, ni les transitions.

use cf_render::CameraPose;

use crate::rng::CinemaRng;
use crate::types::{Cue, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Un plan en cours.
pub trait Shot: Send + Sync {
    /// Écrit la pose voulue dans `out`. Renvoie `false` quand le plan est terminé de lui-même (sujet disparu,
    /// séquence finie).
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool;

    /// Vrai tant que couper serait une faute (astre en vol jusqu'à l'impact, tour de l'anneau) : seul un arrêt du mode
    /// coupe.
    fn holding(&self) -> bool {
        false
    }

    /// Ralenti du monde demandé par ce plan, `(facteur 0.1..=1, durée en s)` : lu une seule fois par le réalisateur
    /// juste après la fabrication du plan (`requestTimeScale` du bullet-time de Sillage), qui le transmet au chef du
    /// spectacle ([`crate::choreo::ShowSide::request_time_scale`]). La caméra, elle, garde le temps réel.
    fn take_time_scale(&mut self) -> Option<(f32, f32)> {
        None
    }

    /// Le plan demande au monde de lancer un astre (`launchMeteor` de Sillage) : lu une seule fois par le réalisateur
    /// juste après la fabrication du plan, qui le transmet par [`crate::director::CinemaDirector::take_meteor_launch`]
    /// (message [`crate::world::LaunchMeteor`] côté Bevy). Sillage recevait l'identifiant de l'astre sur le champ ; ici
    /// l'astre apparaît quelques images plus tard dans les sujets et le plan l'y retrouve.
    fn take_meteor_launch(&mut self) -> bool {
        false
    }

    /// Réaction aux temps musicaux : un plan peut changer de côté, de rotation, d'angle sur une mesure.
    fn cue(&mut self, _cue: &Cue, _frame: &CinemaFrame<'_>) {}
}

/// Fiche d'un plan dans le catalogue du réalisateur.
pub trait ShotSpec: Send + Sync + std::fmt::Debug {
    /// Identifiant stable (`"chase"`, `"orbit"`…), celui de Sillage.
    fn id(&self) -> &'static str;
    /// Rang.
    fn tier(&self) -> ShotTier;
    /// Plage d'énergie (intensity 0 → 1) où le plan est à sa place.
    fn energy(&self) -> (f32, f32);
    /// Durée normale d'un plan (s) : le réalisateur ne coupe pas avant `min_seconds` (sauf drop)…
    fn min_seconds(&self) -> f32;
    /// … et coupe après `max_seconds`.
    fn max_seconds(&self) -> f32;
    /// Plan écrit à la seconde près (astre, anneau, lune, tonneau) : ses durées ne sont pas étirées par le rythme.
    fn scripted(&self) -> bool {
        false
    }
    /// Plan qui s'éloigne de la voiture bien au-delà des bornes ordinaires (tour de la planète) : le réalisateur lui
    /// laisse alors la vue large (`WIDE_*` de `director::guard`).
    fn wide_view(&self) -> bool {
        false
    }
    /// Plan qui joue une animation de la voiture (décollage, atterrissage, esquive) : le réalisateur y bascule dès
    /// qu'elle commence, sans attendre un temps musical, et y entre en panoramique éclair.
    fn reactive(&self) -> bool {
        false
    }
    /// Plan centré sur la voiture (`CAR_IDS` de Sillage) : la synchro caméra / voiture s'y applique.
    fn follows_car(&self) -> bool {
        false
    }
    /// Délai (s) avant que ce plan puisse revenir.
    fn cooldown(&self) -> f32;
    /// Poids de tirage dans son rang.
    fn weight(&self) -> f32;
    /// Transition d'entrée préférée (le réalisateur peut la forcer en coupe sur un drop).
    fn entry(&self) -> TransitionKind;
    /// Le monde permet-il ce plan maintenant (sujet présent, voiture au sol…) ?
    fn available(&self, frame: &CinemaFrame<'_>) -> bool;
    /// Occasion à saisir, 0 → 1. Au-delà de 0,7 le réalisateur y bascule dès le prochain temps.
    fn opportunity(&self, _frame: &CinemaFrame<'_>) -> f32 {
        0.0
    }
    /// Fabrique le plan. `rng` : aléatoire pour les variantes (côté, hauteur, sens, focale).
    ///
    /// Appelé au moment d'une coupe : la seule allocation du réalisateur, une par plan (quelques secondes), jamais par
    /// image.
    fn create(&self, frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot>;
}
