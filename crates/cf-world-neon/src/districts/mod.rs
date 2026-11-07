//! Le moteur de quartiers dans l'application : le gouverneur (`cf-show`) écoute la musique et le show, la ville
//! (`SceneryConfig`, plan de quartiers) et l'étalonnage (`cf-render`, `DistrictGradeState`) appliquent ses décisions.
//!
//! Chaque image, en `CfSet::Simulate` avant les tronçons :
//!
//! 1. un déclencheur `district:<type>` (`Action::Trigger`, catalogue « Quartiers », MIDI) devient un
//!    [`DistrictOrder::Force`] ;
//! 2. les ordres (forçage, gel) sont appliqués, puis le gouverneur rend sa décision ;
//! 3. une transition pose un segment dans le plan, au-delà de la dernière rangée générée (jamais de `replan_all`), avec
//!    un dégradé long de la distance parcourue pendant la durée réglée (≤ 10 s) ; l'étalonnage du nouveau quartier part
//!    quand la voiture entre dans la zone de dégradé (la couleur suit la voiture), le gouverneur reste en transition
//!    jusqu'à la fin réelle du dégradé ;
//! 4. [`DistrictState`] est publié pour le panneau F4.
//!
//! En qualité basse (niveau ≥ 2, celui des façades), l'étalonnage glisse vers le neutre en 1,5 s et en revient de même
//! ([`fade_grade_for_quality`]) ; les quartiers continuent de changer.
//!
//! [`DistrictState`] et [`DistrictOrder`] sont définis dans `cf-render` : la fenêtre opérateur ne dépend pas du monde.

mod state;
mod systems;

use bevy::prelude::*;
use cf_render::{CfSet, EventCategory, WorldEvent, WorldEventAppExt, WorldSet};
use cf_show::district::DistrictKind;

pub use cf_render::{DistrictOrder, DistrictState};
pub use state::{
    APPROACH_MAX_S, Change, DistrictBrain, DistrictDriver, DistrictInput, MAX_BLEND_ROWS, MIN_BLEND_ROWS, MIN_FADE_S,
    MUSIC_HOLD_MAX_S,
};
pub use systems::{DistrictSources, fade_grade_for_quality, start_districts, take_district_triggers, tick_districts};

/// Famille du catalogue d'événements.
pub const FAMILY: &str = "Quartiers";
/// Préfixe des déclencheurs.
const PREFIX: &str = "district:";

/// Le moteur de quartiers. Ajouté par `NeonWorldPlugin` ; se contente de ce qui existe (sans show, le gouverneur ne
/// propose rien ; sans étalonnage, seule la ville change ; sans monde, seul l'étalonnage change).
#[derive(Debug, Default, Clone, Copy)]
pub struct DistrictsPlugin;

impl Plugin for DistrictsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DistrictOrder>()
            .init_resource::<DistrictState>()
            .init_resource::<DistrictDriver>()
            .register_world_events(world_events())
            .add_systems(Startup, start_districts)
            .add_systems(
                Update,
                (take_district_triggers, tick_districts)
                    .chain()
                    .in_set(CfSet::Simulate)
                    .before(WorldSet::Chunks)
                    .run_if(resource_exists::<DistrictBrain>),
            )
            // Le repli de qualité basse ne dépend pas du gouverneur : il tourne même sans lui.
            .add_systems(Update, fade_grade_for_quality.in_set(CfSet::Simulate));
    }
}

/// L'identifiant de déclencheur d'un type de quartier.
#[must_use]
pub fn trigger_id(kind: DistrictKind) -> &'static str {
    match kind {
        DistrictKind::Neon => "district:neon",
        DistrictKind::Suburb => "district:suburb",
        DistrictKind::Residential => "district:residential",
    }
}

/// Le type de quartier qu'un identifiant de déclencheur désigne.
#[must_use]
pub fn kind_of(trigger: &str) -> Option<DistrictKind> {
    let slug = trigger.strip_prefix(PREFIX)?;
    DistrictKind::ALL.into_iter().find(|kind| kind.slug() == slug)
}

/// Les trois types, à inscrire au catalogue (la variante est tirée au déclenchement).
#[must_use]
pub fn world_events() -> Vec<WorldEvent> {
    DistrictKind::ALL
        .into_iter()
        .map(|kind| WorldEvent {
            id: trigger_id(kind),
            label: kind.label(),
            family: FAMILY,
            note: "variante tirée au hasard, transition en dégradé",
            category: Some(EventCategory::District),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaque_type_est_au_catalogue_et_son_id_se_relit() {
        let events = world_events();
        assert_eq!(events.len(), 3);
        for kind in DistrictKind::ALL {
            let event = events.iter().find(|e| e.id == trigger_id(kind)).expect("type absent");
            assert_eq!(event.family, FAMILY);
            assert_eq!(kind_of(event.id), Some(kind));
        }
        assert_eq!(kind_of("district:inconnu"), None);
        assert_eq!(kind_of("landmark:arcade"), None);
    }
}
