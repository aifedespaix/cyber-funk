//! Les douze scripts de cascades (J7-07) : portage de `stunts/scripts/*.ts` de Sillage. Chacun est une ligne de temps en
//! temps musicaux qui écrit une [`StuntPose`](super::StuntPose) pour la voiture et déplace ses propres véhicules (pris à
//! la flotte, rendus dans `end`). Le directeur ([`super::StuntDirector`]) les tire, les cale et fond leur pose.
//!
//! # Règles tenues
//!
//! - hasard : toujours le [`Rand`] injecté par le contexte (graine du directeur), jamais global : mêmes entrées, même
//!   cascade ;
//! - pas d'allocation par image : un script est alloué une fois au départ (le `Box` et, pour le slalom ou la poursuite,
//!   un petit tableau) ; `step` n'alloue rien ;
//! - règle 7 : aucune cascade ne produit de flash ni de variation de luminance. Elles n'écrivent que la pose de la
//!   voiture et la position de leurs véhicules (matériaux et traînées de la flotte inchangés) ;
//! - règle 8 : aucun matériau, shader ni asset nouveau. Les véhicules sont ceux de la flotte du trafic, déjà préchauffés
//!   par `TrafficPlugin` ; il n'y a donc rien à déclarer à `warm_material` ;
//! - niveau 0 : la voiture ne vole jamais. La garde est celle du directeur (`max_floor > 0`, voir
//!   [`super::permit::level_open`]) ; elle coupe aussi une cascade en cours.
//!
//! # Inscription
//!
//! [`register_all`] inscrit les douze scripts ; [`StuntsPlugin`](super::StuntsPlugin) l'appelle. Chaque script reste
//! inscriptible seul (`app.register_stunt(...)`), notamment pour les tests.

mod ascension;
mod big_jump;
mod head_on;
mod inverted;
mod loop_;
mod near_miss;
mod off_road;
mod rooftop;
mod sign_hop;
mod slalom;
mod threading;
mod wall_ride;

#[cfg(test)]
mod tests;

use bevy::app::App;
use bevy::math::DVec3;

use super::script::{StuntAppExt, StuntCtx, StuntPose, StuntRun};
use super::world::StuntWorld;
use crate::pilot::rand::Rand;
use crate::traffic::CraftId;

pub use ascension::{ASCENSION_PHASES, Ascension};
pub use big_jump::BigJump;
pub use head_on::HeadOn;
pub use inverted::Inverted;
pub use loop_::Loop;
pub use near_miss::NearMiss;
pub use off_road::OffRoad;
pub use rooftop::Rooftop;
pub use sign_hop::SignHop;
pub use slalom::Slalom;
pub use threading::Threading;
pub use wall_ride::WallRide;

/// Inscrit les douze cascades au registre (`SCRIPTS` de `scripts/index.ts`).
pub fn register_all(app: &mut App) {
    app.register_stunt(OffRoad)
        .register_stunt(Slalom)
        .register_stunt(NearMiss)
        .register_stunt(Loop)
        .register_stunt(HeadOn)
        .register_stunt(Inverted)
        .register_stunt(WallRide)
        .register_stunt(Threading)
        .register_stunt(SignHop)
        .register_stunt(Rooftop)
        .register_stunt(BigJump)
        .register_stunt(Ascension);
}

/// Indice tiré dans `0..n` (`Math.floor(Math.random() * n)`).
fn pick_index(rng: &mut Rand, n: usize) -> usize {
    ((rng.unit() * n as f64) as usize).min(n.saturating_sub(1))
}

/// Vrai avec la probabilité `p` (`Math.random() < p`).
fn chance(rng: &mut Rand, p: f64) -> bool {
    rng.unit() < p
}

/// Pose un véhicule à `(x, y, z)` ; `None` si la flotte est pleine (la cascade se joue alors sans lui).
fn spawn_at(world: &mut dyn StuntWorld, variant: u8, at: (f64, f64, f64), scale: f64, tint: i64) -> Option<CraftId> {
    world.spawn(variant, DVec3::new(at.0, at.1, at.2), scale as f32, tint)
}

/// Rend un véhicule posé, s'il l'a été.
fn release(world: &mut dyn StuntWorld, id: Option<CraftId>) {
    if let Some(id) = id {
        world.release(id);
    }
}

/// Place le point filmé sur un véhicule posé, s'il est encore vivant.
fn focus_on(world: &mut dyn StuntWorld, id: Option<CraftId>, out: &mut StuntPose) {
    if let Some(craft) = id.and_then(|id| world.craft_mut(id)) {
        out.focus = craft.position;
    }
}

/// Le côté (−1 ou 1) d'un signe de flottant, pour interroger la ville.
fn side_of(side: f64) -> i8 {
    if side < 0.0 { -1 } else { 1 }
}

/// Une cascade vide : sert quand la structure visée a disparu entre `available` et `begin`. Finie dès la première image.
#[derive(Debug)]
struct IdleRun;

impl StuntRun for IdleRun {
    fn duration(&self) -> f64 {
        0.0
    }

    fn step(&mut self, _t: f64, _ctx: &mut StuntCtx<'_>, _out: &mut StuntPose) -> bool {
        false
    }

    fn end(&mut self, _world: &mut dyn StuntWorld) {}
}
