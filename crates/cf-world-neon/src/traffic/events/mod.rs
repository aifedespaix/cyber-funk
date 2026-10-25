//! Les événements du trafic, lot A : calmes et urbains (J7-04, portage de `traffic/events/` de Sillage).
//!
//! Onze événements : `lanterns`, `fish`, `refuel`, `airship`, `escort`, `chase`, `convoy`, `bikes`, `toll`, `adship`,
//! `holo-title`. Chacun est une implémentation de [`TrafficEvent`], inscrite au registre par [`CalmEventsPlugin`] sous son
//! nom ([`EventName`]) ; le directeur le tire selon les portes du look, ou le lance à la demande (`force`).
//!
//! Règles tenues : aucun `spawn` en régime établi (on prend des véhicules au pool de la flotte et on les rend) ; les pas
//! des véhicules scénarisés sont écrits par l'événement (`Motion::Scripted`) ; tout ce qui clignote ou éclate passe par
//! le limiteur de flashs côté matière, et ne clignote pas ici (voir les écarts de chaque fichier).
//!
//! Écarts communs (voir la PR, section « Écarts ») : les teintes viennent du cycle de la palette (les teintes nommées sont
//! maintenant dans le contexte, mais le lot A ne les a pas reprises) ; le vaisseau publicitaire ne pend pas ses panneaux
//! (le prêteur de panneaux est dans le contexte, `EventCtx::signs`, mais pas encore utilisé).
//!
//! Branchement : [`CalmEventsPlugin`] est ajouté par [`TrafficPlugin`](super::TrafficPlugin), comme le lot B.

mod adship;
mod airship;
mod bikes;
mod chase;
pub(crate) mod common;
mod convoy;
mod escort;
mod fish;
mod holo_title;
mod lanterns;
mod refuel;
#[cfg(test)]
pub(crate) mod test_rig;
mod toll;

use bevy::app::{App, Plugin};

pub use holo_title::{HoloPose, HoloTitleEvent};

use super::event::{EventCtx, EventFactory, EventRegistry, TrafficEvent};
use super::names::EventName;
use adship::AdShipEvent;
use airship::AirshipEvent;
use bikes::BikesEvent;
use chase::ChaseEvent;
use convoy::ConvoyEvent;
use escort::EscortEvent;
use fish::FishEvent;
use lanterns::LanternEvent;
use refuel::RefuelEvent;
use toll::TollEvent;

/// Les événements du lot A, dans l'ordre de la fiche J7-04.
pub const CALM_EVENTS: [EventName; 11] = [
    EventName::Lanterns,
    EventName::Fish,
    EventName::Refuel,
    EventName::Airship,
    EventName::Escort,
    EventName::Chase,
    EventName::Convoy,
    EventName::Bikes,
    EventName::Toll,
    EventName::Adship,
    EventName::HoloTitle,
];

/// Titre affiché quand le spectacle ne donne aucun nom : le nom de la ville.
pub const DEFAULT_TITLE: &str = "NEON CITY";

/// Fabrique une fabrique à partir du constructeur d'un événement sans paramètre.
pub(crate) fn factory<E: TrafficEvent + 'static>(build: fn(&mut EventCtx<'_>) -> E) -> EventFactory {
    Box::new(move |ctx: &mut EventCtx<'_>| -> Box<dyn TrafficEvent> { Box::new(build(ctx)) })
}

/// Les fabriques du lot A, prêtes à inscrire au registre. `title` sert au titre holographique.
#[must_use]
pub fn calm_factories(title: &str) -> Vec<(EventName, EventFactory)> {
    let title = title.to_owned();
    // Le nom du DJ en cours (ou de l'événement) quand le spectacle le donne ; sinon le titre du plugin.
    let holo: EventFactory = Box::new(move |ctx: &mut EventCtx<'_>| -> Box<dyn TrafficEvent> {
        let text = ctx.frame.dj_title.unwrap_or(title.as_str());
        Box::new(HoloTitleEvent::new(ctx, text))
    });
    vec![
        (EventName::Lanterns, factory(LanternEvent::new)),
        (EventName::Fish, factory(FishEvent::new)),
        (EventName::Refuel, factory(RefuelEvent::new)),
        (EventName::Airship, factory(AirshipEvent::new)),
        (EventName::Escort, factory(EscortEvent::new)),
        (EventName::Chase, factory(ChaseEvent::new)),
        (EventName::Convoy, factory(ConvoyEvent::new)),
        (EventName::Bikes, factory(BikesEvent::new)),
        (EventName::Toll, factory(TollEvent::new)),
        (EventName::Adship, factory(AdShipEvent::new)),
        (EventName::HoloTitle, holo),
    ]
}

/// Inscrit le lot A au registre du trafic. Le titre holographique est [`DEFAULT_TITLE`] tant que le spectacle ne fournit
/// pas les noms.
#[derive(Debug, Clone)]
pub struct CalmEventsPlugin {
    /// Le texte du titre holographique.
    pub title: String,
}

impl Default for CalmEventsPlugin {
    fn default() -> Self {
        Self {
            title: DEFAULT_TITLE.to_owned(),
        }
    }
}

impl Plugin for CalmEventsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EventRegistry>();
        let mut registry = app.world_mut().resource_mut::<EventRegistry>();
        for (name, build) in calm_factories(&self.title) {
            registry.register(name, build);
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::pilot::rand::Rand;
    use crate::traffic::fleet::Fleet;
    use crate::traffic::frame::TrafficFrame;
    use crate::traffic::glints::Glints;
    use crate::traffic::walls::LightWalls;
    use bevy::color::LinearRgba;
    use bevy::math::DVec3;

    fn registry() -> EventRegistry {
        let mut registry = EventRegistry::default();
        for (name, build) in calm_factories(DEFAULT_TITLE) {
            registry.register(name, build);
        }
        registry
    }

    /// Une image de test : voiture en vol de croisière, cycle de teintes distinctes.
    fn frame(car_z: f64, time: f64) -> TrafficFrame<'static> {
        TrafficFrame {
            dt: 1.0 / 60.0,
            time,
            car: DVec3::new(0.0, 0.0, car_z),
            speed: 30.0,
            playing: true,
            cycle: [
                LinearRgba::rgb(1.0, 0.2, 0.4),
                LinearRgba::rgb(0.2, 1.0, 0.6),
                LinearRgba::rgb(0.3, 0.4, 1.0),
                LinearRgba::rgb(1.0, 0.8, 0.2),
                LinearRgba::rgb(0.8, 0.3, 1.0),
                LinearRgba::rgb(0.2, 0.9, 1.0),
            ],
            ..TrafficFrame::default()
        }
    }

    /// Bilan d'une simulation : véhicules et points lumineux au plus fort de la course.
    #[derive(Debug, Clone, Copy, Default)]
    struct Peak {
        crafts: usize,
        glints: usize,
        pins: usize,
    }

    /// Fait vivre `name` pendant `seconds` secondes simulées, comme le ferait le directeur à 60 i/s.
    fn simulate(name: EventName, seconds: f32) -> (Fleet, Peak) {
        let mut registry = registry();
        let mut fleet = Fleet::new();
        let mut glints = Glints::new();
        let mut walls = LightWalls::new();
        let mut scene = crate::traffic::scene::Scene::new();
        let mut rng = Rand::new(0x5EED);
        let mut car_z = 0.0;
        let mut time = 0.0;
        let frame0 = frame(car_z, time);
        let mut event = {
            let mut ctx = EventCtx {
                frame: &frame0,
                fleet: &mut fleet,
                glints: &mut glints,
                walls: &mut walls,
                rng: &mut rng,
                scene: &mut scene,
                signs: None,
            };
            registry
                .create(name, &mut ctx)
                .expect("l'événement du lot A est inscrit")
        };
        let dt = 1.0_f32 / 60.0;
        let steps = (seconds / dt) as usize;
        let mut running = true;
        let mut peak = Peak::default();
        for _ in 0..steps {
            car_z -= f64::from(30.0 * dt);
            time += f64::from(dt);
            let frame = frame(car_z, time);
            glints.begin(frame.car);
            scene.begin(frame.car);
            if running {
                let mut ctx = EventCtx {
                    frame: &frame,
                    fleet: &mut fleet,
                    glints: &mut glints,
                    walls: &mut walls,
                    rng: &mut rng,
                    scene: &mut scene,
                    signs: None,
                };
                running = event.update(dt, &mut ctx);
            }
            fleet.update(dt, frame.car.z, |_| true);
            walls.update(time);
            assert!(glints.used() <= crate::traffic::glints::GLINT_CAPACITY);
            peak.glints = peak.glints.max(glints.used());
            peak.pins = peak.pins.max(scene.pins().len());
            peak.crafts = peak.crafts.max(fleet.crafts().iter().filter(|c| c.active).count());
            for craft in fleet.crafts().iter().filter(|c| c.active) {
                assert!(craft.position.is_finite(), "{name:?} : position non finie");
            }
        }
        let frame_end = frame(car_z, time);
        let mut ctx = EventCtx {
            frame: &frame_end,
            fleet: &mut fleet,
            glints: &mut glints,
            walls: &mut walls,
            rng: &mut rng,
            scene: &mut scene,
            signs: None,
        };
        event.end(&mut ctx);
        (fleet, peak)
    }

    #[test]
    fn chaque_evenement_du_lot_est_inscrit_sous_son_nom() {
        let registry = registry();
        assert_eq!(registry.len(), CALM_EVENTS.len());
        for name in CALM_EVENTS {
            assert!(registry.contains(name), "{name:?} absent du registre");
        }
    }

    #[test]
    fn chaque_evenement_se_declenche_par_son_slug() {
        // `Action::Trigger(TriggerId)` porte le slug de l'événement (`"chase"`, `"holo-title"`…) : il doit s'y résoudre.
        for name in CALM_EVENTS {
            assert_eq!(EventName::from_slug(name.as_str()), Some(name));
        }
    }

    #[test]
    fn chaque_evenement_vit_60_secondes_sans_erreur_et_rend_ses_vehicules() {
        for name in CALM_EVENTS {
            let (fleet, peak) = simulate(name, 60.0);
            let left = fleet.crafts().iter().filter(|c| c.active).count();
            assert_eq!(left, 0, "{name:?} : {left} véhicule(s) non rendus après end");
            // Le titre holographique ne pose ni véhicule ni point : sa pose est testée dans `holo_title`.
            if name != EventName::HoloTitle {
                assert!(peak.crafts + peak.glints > 0, "{name:?} n'a rien montré en 60 s");
            }
            // Les projecteurs qui balaient la route : le cargo, le pétrolier, la police et le vaisseau publicitaire.
            let projects = matches!(
                name,
                EventName::Airship | EventName::Refuel | EventName::Chase | EventName::Adship
            );
            assert_eq!(peak.pins > 0, projects, "{name:?} : {} projecteur(s)", peak.pins);
            assert!(peak.pins <= crate::traffic::scene::MAX_PINS);
        }
    }

    #[test]
    fn le_titre_holographique_prend_le_nom_du_dj_quand_le_spectacle_le_donne() {
        let mut registry = registry();
        let mut rig = crate::traffic::events::test_rig::Rig::new(4);
        let title = |registry: &mut EventRegistry, rig: &mut crate::traffic::events::test_rig::Rig, dj: Option<&str>| {
            let frame = TrafficFrame {
                dj_title: dj,
                ..rig.frame()
            };
            let mut ctx = EventCtx {
                frame: &frame,
                fleet: &mut rig.fleet,
                glints: &mut rig.glints,
                walls: &mut rig.walls,
                rng: &mut rig.rng,
                scene: &mut rig.scene,
                signs: None,
            };
            let event = registry.create(EventName::HoloTitle, &mut ctx).expect("inscrit");
            format!("{event:?}")
        };
        assert!(title(&mut registry, &mut rig, Some("DJ AIFE")).contains("DJ AIFE"));
        assert!(title(&mut registry, &mut rig, None).contains(DEFAULT_TITLE));
    }

    #[test]
    fn le_plugin_remplit_le_registre_et_ne_panique_pas() {
        let mut app = App::new();
        app.add_plugins(CalmEventsPlugin::default());
        let registry = app.world().resource::<EventRegistry>();
        assert_eq!(registry.len(), CALM_EVENTS.len());
    }
}
