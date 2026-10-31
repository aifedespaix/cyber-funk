//! Les immeubles-vitrines dans l'onglet « Debug monde » : un bouton par immeuble.
//!
//! Le déclencheur `landmark:<nom>` force l'immeuble de la scène puis lance l'événement `landmark` du trafic, qui le
//! montre. Le `match` des libellés est exhaustif : un immeuble ajouté sans libellé ne compile pas.

use bevy::prelude::*;
use cf_render::{ShowRuntime, WorldEvent};

use super::landmark::LandmarkKind;
use super::plugin::LandmarkHandle;
use crate::traffic::{EventName, TrafficDirector};

/// Famille de ces événements dans le catalogue.
pub const FAMILY: &str = "Immeubles-vitrines";

/// Préfixe des identifiants de déclencheur.
const PREFIX: &str = "landmark:";

fn label(kind: LandmarkKind) -> &'static str {
    match kind {
        LandmarkKind::Error => "ERROR (panneau rouge)",
        LandmarkKind::Matrix => "Mur de code qui tombe",
        LandmarkKind::Flux => "Mur de code qui file",
        LandmarkKind::Binary => "Mur binaire",
        LandmarkKind::Mecha => "Chœur de mecha",
        LandmarkKind::Arch => "Arche de voxels",
        LandmarkKind::Lava => "Façade de lave",
        LandmarkKind::Arcade => "Borne d'arcade",
        LandmarkKind::Board => "Tour carte mère",
        LandmarkKind::Organ => "Orgue de cathédrale",
        LandmarkKind::Aquarium => "Aquarium géant",
        LandmarkKind::Greenhouse => "Serre jungle",
        LandmarkKind::Lighthouse => "Phare",
        LandmarkKind::Clock => "Tour horloge",
        LandmarkKind::Ice => "Falaise de glace",
        LandmarkKind::Radio => "Tour radio",
        LandmarkKind::Pagoda => "Pagode néon",
        LandmarkKind::Console => "Manette géante",
        LandmarkKind::Hotel => "Hôtel de verre",
    }
}

/// Les immeubles, à inscrire au catalogue. L'identifiant est celui de fraîcheur du chef du spectacle (`landmark:<nom>`).
#[must_use]
pub fn world_events() -> Vec<WorldEvent> {
    LandmarkKind::ALL
        .into_iter()
        .map(|kind| WorldEvent {
            id: kind.fresh_id(),
            label: label(kind),
            family: FAMILY,
            note: "",
        })
        .collect()
}

/// L'immeuble qu'un identifiant de déclencheur désigne.
#[must_use]
pub fn kind_of(trigger: &str) -> Option<LandmarkKind> {
    trigger.strip_prefix(PREFIX).and_then(LandmarkKind::from_name)
}

/// `CfSet::Simulate`, avant le trafic : prend un déclencheur `landmark:<nom>`, force cet immeuble et lance l'événement.
///
/// Un autre identifiant reste dans la file pour le système à qui il est destiné.
pub(super) fn take_landmark_triggers(
    runtime: Option<ResMut<ShowRuntime>>,
    handle: Res<LandmarkHandle>,
    director: Option<ResMut<TrafficDirector>>,
) {
    // Sans trafic (tests de logique) ou sans spectacle, personne ne lance d'immeuble.
    let (Some(mut runtime), Some(mut director)) = (runtime, director) else {
        return;
    };
    let wanted = runtime
        .director()
        .state()
        .orders
        .trigger_pending()
        .and_then(|id| kind_of(&id.0));
    if let Some(kind) = wanted
        && runtime.director_mut().take_trigger().is_some()
    {
        handle.force(kind);
        director.force(EventName::Landmark);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaque_immeuble_est_dans_le_catalogue_et_son_id_se_relit() {
        let events = world_events();
        assert_eq!(events.len(), LandmarkKind::ALL.len());
        for kind in LandmarkKind::ALL {
            let event = events
                .iter()
                .find(|e| e.id == kind.fresh_id())
                .expect("immeuble absent");
            assert_ne!(event.label, "");
            assert_eq!(kind_of(event.id), Some(kind));
        }
    }

    #[test]
    fn un_autre_declencheur_n_est_pas_un_immeuble() {
        assert_eq!(kind_of("storm"), None);
        assert_eq!(kind_of("landmark:inconnu"), None);
        assert_eq!(kind_of("landmark"), None);
    }
}
