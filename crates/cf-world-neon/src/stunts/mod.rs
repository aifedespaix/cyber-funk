//! Les cascades de la voiture (J7-06) : portage de `stunts/director.ts`, `permit.ts`, `types.ts`, `world.ts`, `math.ts`
//! et `scripts/index.ts` de Sillage. Quand le pilote conduit seul et que le look le permet, la voiture s'amuse :
//! tonneaux, saltos, slaloms, sauts… Chaque cascade est une ligne de temps (en temps musicaux) qui écrit une pose pour la
//! voiture ; le directeur choisit, cale sur la mesure, le drop ou la phrase, et fond la pose sur la conduite normale.
//!
//! # Contenu
//!
//! - [`kind`] : le catalogue ([`StuntKind`], [`StuntTier`], [`StuntAlign`]) et la validation des identifiants que
//!   `cf-show` ne connaît que comme des chaînes (liste blanche des looks, cascade du drop préparé) ;
//! - [`script`] : le contrat d'une cascade ([`StuntScript`], [`StuntRun`], [`StuntPose`], [`StuntEnv`]) et le registre
//!   ([`StuntRegistry`], [`StuntAppExt::register_stunt`]). **Les scripts concrets sont J7-07** : le registre est prêt à
//!   les recevoir, comme celui des événements du trafic ;
//! - [`world`] : ce que les cascades savent de la ville ([`StuntWorld`], [`TrafficStuntWorld`]) ;
//! - [`permit`] : les portes (rangs, liste blanche, Ascension et son délai, ordre de l'opérateur) ;
//! - [`director`] : [`StuntDirector`], pur (temps et hasard injectés) ;
//! - [`math`] : `between`, `bump`, `turn`, `Rate`, `move_craft` ;
//! - [`systems`] : la couche Bevy, mince : [`StuntOrders`] est ce que lisent la voiture (effets) et le réalisateur.
//!
//! # Portes du look
//!
//! | porte | effet |
//! | --- | --- |
//! | `stunt_tiers`, `stunt_kinds` | rangs et liste blanche ; l'une des deux vide : aucune cascade (niveaux 0 et 1) |
//! | `stunt_gap_scale` | multiplie l'attente entre deux cascades |
//! | `ascension` | l'Ascension (niveau 5) a sa propre porte, et un délai de 30 à 40 min |
//! | `max_floor` | à 0, la voiture ne quitte jamais le sol : aucune cascade, même coupée en route (garde du pilote) |
//!
//! # Branchement
//!
//! [`StuntsPlugin`] s'ajoute après le pilote et le trafic. Dans l'image : pilote ([`PilotSet`](crate::pilot::PilotSet)),
//! puis [`StuntSet`] (la pose est fondue dans l'abscisse et l'animation du pilote), puis le pas du trafic et le
//! réalisateur, qui lisent donc la voiture telle qu'elle sera dessinée.
//!
//! # Règles tenues
//!
//! - aucune allocation par image : tampons du tirage réservés au départ ; le script est alloué une fois par cascade ;
//! - règle 7 : une cascade ne produit aucun flash ni variation de luminance, elle n'écrit que la pose de la voiture et
//!   déplace ses propres véhicules ; les éclats du trafic restent derrière `TrafficFlash`.

pub mod director;
pub mod kind;
pub mod math;
pub mod permit;
pub mod script;
pub mod systems;
pub mod world;

#[cfg(test)]
mod tests;

use bevy::prelude::*;
use cf_render::CfSet;

pub use director::{StuntDirector, StuntFrame, StuntView, fold_period};
pub use kind::{STUNT_COUNT, StuntAlign, StuntKind, StuntTier, UnknownStunt, unknown_kinds, validate_gates};
pub use permit::{AscensionGate, NoLink, StuntLink};
pub use script::{
    Planned, StuntAppExt, StuntCtx, StuntEnv, StuntPose, StuntRegistry, StuntRun, StuntScript, StuntShow,
};
pub use systems::{StuntOrders, StuntSettings};
pub use world::{StuntWorld, TrafficStuntWorld};

use crate::pilot::PilotSet;
use crate::traffic::{TrafficDirector, TrafficSet};

/// L'étape des cascades dans l'image, en `CfSet::Simulate` : après le pilote, avant le pas du trafic.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StuntSet;

/// Installe les cascades. À ajouter après le pilote ([`PilotPlugin`](crate::PilotPlugin)) et le trafic
/// ([`TrafficPlugin`](crate::TrafficPlugin)) ; sans eux, le plugin ne fait rien.
#[derive(Debug, Clone, Copy, Default)]
pub struct StuntsPlugin {
    /// Réglages de départ.
    pub settings: StuntSettings,
}

impl Plugin for StuntsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.settings)
            .insert_resource(StuntDirector::new(self.settings.seed))
            .init_resource::<StuntRegistry>()
            .init_resource::<StuntOrders>()
            .configure_sets(
                Update,
                StuntSet
                    .in_set(CfSet::Simulate)
                    .after(PilotSet)
                    .before(TrafficSet::Step),
            )
            .add_systems(
                Update,
                systems::drive_stunts
                    .in_set(StuntSet)
                    .run_if(resource_exists::<TrafficDirector>)
                    .run_if(resource_exists::<crate::CarAnimation>),
            );
    }
}
