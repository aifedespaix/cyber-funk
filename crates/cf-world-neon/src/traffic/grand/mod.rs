//! Les événements du trafic, lot B : vol et démesure (J7-05, portage de `traffic/events/` de Sillage).
//!
//! Douze événements : `landmark`, `aurora`, `swarm`, `crossroad`, `tower`, `race`, `dancer`, `tunnel`, `storm`,
//! `fireworks`, `blackout`, `city-pulse`. Chacun est une implémentation de [`TrafficEvent`], inscrite au registre par
//! [`GrandEventsPlugin`] sous son nom ([`EventName`]) ; le directeur le tire selon les portes du look, ou le lance à la
//! demande (`force`, et l'`Action::Trigger` de l'opérateur, voir `systems::take_triggers`).
//!
//! # Ce qui est dessiné, et comment
//!
//! Les structures (dalle et piliers de la route croisée, méga-tour, portiques, rideaux, danseur, aurore) ne sont pas des
//! entités : l'événement repose à chaque image des quadrilatères dans [`Scene::panels`](super::scene::Scene) (capacité
//! fixe), et deux maillages dynamiques les dessinent (`render::panels`). Les drones, les éclairs et les salves sont des
//! points lumineux. Les véhicules (route croisée, tour, course) viennent du pool de la flotte et lui sont rendus.
//!
//! # Règle 7 : aucun éclat hors du limiteur
//!
//! Tout ce qui fait varier fortement la lumière d'une grande surface passe par le limiteur de flashs du trafic
//! ([`TrafficFlash`](super::TrafficFlash)) :
//!
//! - **orage** : chaque coup *demande* un éclair ; c'est le niveau accordé qui blanchit les façades ;
//! - **feux d'artifice** : chaque salve *demande* un éclat ; le point blanc n'a que l'éclat accordé ;
//! - **panne de courant** : la chute et le retour de la lumière *demandent* une variation de toute la ville ; sans accord,
//!   la panne est adoucie ([`blackout::SOFT_DARK`]) ;
//! - **pouls** (rails, rideaux, tunnel, danseur, aurore, pouls des fenêtres, balises) : toujours le pouls accordé
//!   (`SceneFx::kick`), jamais le kick brut.
//!
//! # Écarts communs
//!
//! Voir la PR, section « Écarts ». Les immeubles-vitrines eux-mêmes sont ceux de J7-08 : tant qu'elle n'a rien inscrit,
//! `landmark` montre une vitrine de secours ([`landmark::Showcase`]).

mod aurora;
mod blackout;
mod city_pulse;
mod crossroad;
mod dancer;
mod fireworks;
pub mod landmark;
mod moto_jump;
pub(crate) mod portal;
mod race;
mod storm;
mod swarm;
mod tower;
mod tunnel;

use std::sync::Mutex;

use bevy::app::{App, Plugin};

pub use landmark::{
    Landmark, LandmarkDebug, LandmarkEvent, LandmarkFactory, LandmarkFrame, LandmarkVariation, RoadBand, Showcase,
    VariationHistory,
};

use super::event::{EventCtx, EventFactory, EventRegistry, TrafficEvent};
use super::events::factory;
use super::names::EventName;
use aurora::AuroraEvent;
use blackout::BlackoutEvent;
use city_pulse::CityPulseEvent;
use crossroad::CrossRoadEvent;
use dancer::DancerEvent;
use fireworks::FireworksEvent;
use moto_jump::MotoJumpEvent;
use race::RaceEvent;
use storm::StormEvent;
use swarm::SwarmEvent;
use tower::TowerEvent;
use tunnel::TunnelEvent;

/// Les événements du lot B, dans l'ordre de la fiche J7-05.
pub const GRAND_EVENTS: [EventName; 13] = [
    EventName::Landmark,
    EventName::Aurora,
    EventName::Swarm,
    EventName::Crossroad,
    EventName::Tower,
    EventName::Race,
    EventName::Dancer,
    EventName::Tunnel,
    EventName::Storm,
    EventName::Fireworks,
    EventName::Blackout,
    EventName::CityPulse,
    EventName::MotoJump,
];

/// L'immeuble-vitrine de secours quand J7-08 n'a rien inscrit.
fn showcase_provider() -> LandmarkFactory {
    Box::new(|_ctx: &mut EventCtx<'_>| -> Box<dyn Landmark> { Box::new(Showcase::new()) })
}

/// Les fabriques du lot B, prêtes à inscrire au registre. `landmarks` fournit l'immeuble de chaque passage de `landmark` ;
/// sans lui, la vitrine de secours.
#[must_use]
pub fn grand_factories(landmarks: Option<LandmarkFactory>) -> Vec<(EventName, EventFactory)> {
    let mut provider = landmarks.unwrap_or_else(showcase_provider);
    let mut history = VariationHistory::default();
    let landmark: EventFactory = Box::new(move |ctx: &mut EventCtx<'_>| -> Box<dyn TrafficEvent> {
        let landmark = provider(ctx);
        // Hors spectacle, côté et teinte au hasard ; en spectacle, ils alternent d'un passage à l'autre.
        let variation = ctx
            .frame
            .show
            .is_some()
            .then(|| history.next(ctx, landmark.spans_road()));
        Box::new(LandmarkEvent::new(ctx, landmark, None, variation))
    });
    vec![
        (EventName::Landmark, landmark),
        (EventName::Aurora, factory(AuroraEvent::new)),
        (EventName::Swarm, factory(SwarmEvent::new)),
        (EventName::Crossroad, factory(CrossRoadEvent::new)),
        (EventName::Tower, factory(TowerEvent::new)),
        (EventName::Race, factory(RaceEvent::new)),
        (EventName::Dancer, factory(DancerEvent::new)),
        (EventName::Tunnel, factory(TunnelEvent::new)),
        (EventName::Storm, factory(StormEvent::new)),
        (EventName::Fireworks, factory(FireworksEvent::new)),
        (EventName::Blackout, factory(BlackoutEvent::new)),
        (EventName::CityPulse, factory(CityPulseEvent::new)),
        (EventName::MotoJump, factory(MotoJumpEvent::new)),
    ]
}

/// Inscrit le lot B au registre du trafic.
#[derive(Default)]
pub struct GrandEventsPlugin {
    landmarks: Mutex<Option<LandmarkFactory>>,
}

impl std::fmt::Debug for GrandEventsPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GrandEventsPlugin").finish_non_exhaustive()
    }
}

impl GrandEventsPlugin {
    /// Le lot B avec les immeubles-vitrines de `landmarks` (J7-08) à la place de la vitrine de secours.
    #[must_use]
    pub fn with_landmarks(landmarks: LandmarkFactory) -> Self {
        Self {
            landmarks: Mutex::new(Some(landmarks)),
        }
    }
}

impl Plugin for GrandEventsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EventRegistry>();
        // Le verrou n'est pris qu'ici, au démarrage ; un verrou empoisonné (panique d'un autre `build`) rend quand même son
        // contenu : la vitrine de secours vaut mieux qu'un événement absent.
        let landmarks = self
            .landmarks
            .lock()
            .map_or_else(|poisoned| poisoned.into_inner().take(), |mut slot| slot.take());
        let mut registry = app.world_mut().resource_mut::<EventRegistry>();
        for (name, build) in grand_factories(landmarks) {
            registry.register(name, build);
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::traffic::events::test_rig::Rig;
    use crate::traffic::events::{CALM_EVENTS, DEFAULT_TITLE, calm_factories};
    use crate::traffic::flash::TrafficFlash;
    use crate::traffic::glints::GLINT_CAPACITY;
    use crate::traffic::panels::PANEL_CAPACITY;
    use cf_show::limiter::{MAX_FLASHES_PER_SECOND, MIN_FADE};

    fn registry() -> EventRegistry {
        let mut registry = EventRegistry::default();
        for (name, build) in grand_factories(None) {
            registry.register(name, build);
        }
        registry
    }

    #[test]
    fn le_lot_b_et_le_lot_a_couvrent_ensemble_tout_le_catalogue() {
        let mut registry = registry();
        assert_eq!(registry.len(), GRAND_EVENTS.len());
        for (name, build) in calm_factories(DEFAULT_TITLE) {
            registry.register(name, build);
        }
        assert_eq!(registry.len(), EventName::ALL.len());
        for name in EventName::ALL {
            assert!(registry.contains(name), "{name:?} absent du registre");
        }
        // Aucun doublon entre les deux lots.
        assert_eq!(CALM_EVENTS.len() + GRAND_EVENTS.len(), EventName::ALL.len());
        for name in GRAND_EVENTS {
            assert!(!CALM_EVENTS.contains(&name), "{name:?} dans les deux lots");
        }
    }

    #[test]
    fn chaque_evenement_du_lot_se_declenche_par_son_slug() {
        for name in GRAND_EVENTS {
            assert_eq!(EventName::from_slug(name.as_str()), Some(name));
        }
    }

    /// Fait vivre `name` jusqu'à sa fin (ou 60 s), comme le ferait le directeur à 60 i/s, avec un kick à 128 BPM. Rend la
    /// flotte restante après `end`, ce que l'événement a montré (points, panneaux, véhicules) et sa durée en images.
    fn simulate(name: EventName, seconds: f32) -> (usize, usize, usize, usize, usize) {
        let mut registry = registry();
        let mut rig = Rig::new(0x5EED);
        let frame = rig.frame();
        let mut event = {
            let mut ctx = EventCtx {
                frame: &frame,
                fleet: &mut rig.fleet,
                glints: &mut rig.glints,
                walls: &mut rig.walls,
                rng: &mut rig.rng,
                scene: &mut rig.scene,
                signs: None,
            };
            registry
                .create(name, &mut ctx)
                .expect("l'événement du lot B est inscrit")
        };
        let steps = (seconds * 60.0) as usize;
        let (mut peak_glints, mut peak_panels, mut peak_crafts, mut frames) = (0, 0, 0, 0);
        for i in 0..steps {
            let alive = rig.step(event.as_mut(), 1.0 / 60.0, |frame| {
                frame.beat.kick = i % 28 == 0;
                frame.beat.bass = 0.7;
                frame.beat.mid = 0.5;
                frame.beat.pulse = if i % 28 < 7 { 1.0 - (i % 28) as f32 / 7.0 } else { 0.0 };
                frame.beat.count = (i / 28) as u32;
                frame.beat.beat = i as f64 / 28.0;
                frame.dance_beat = f64::from(frame.beat.count);
                frame.drive.drop = i % 600 == 599;
                frame.drive.intensity = 0.7;
            });
            frames += 1;
            assert!(rig.glints.used() <= GLINT_CAPACITY, "{name:?} : points");
            assert!(rig.scene.panels.solid().len() <= PANEL_CAPACITY, "{name:?} : panneaux");
            assert!(rig.scene.panels.light().len() <= PANEL_CAPACITY, "{name:?} : lumière");
            peak_glints = peak_glints.max(rig.glints.used());
            peak_panels = peak_panels.max(rig.scene.panels.used());
            peak_crafts = peak_crafts.max(rig.active_crafts());
            for craft in rig.fleet.crafts().iter().filter(|c| c.active) {
                assert!(craft.position.is_finite(), "{name:?} : position non finie");
            }
            for g in rig.glints.points() {
                assert!(g.offset.is_finite() && g.size.is_finite(), "{name:?} : point non fini");
            }
            for p in rig.scene.panels.solid().iter().chain(rig.scene.panels.light()) {
                assert!(
                    p.center.is_finite() && p.u.is_finite() && p.v.is_finite(),
                    "{name:?} : panneau non fini"
                );
            }
            if !alive {
                break;
            }
        }
        rig.end(event.as_mut());
        (rig.active_crafts(), peak_glints, peak_panels, peak_crafts, frames)
    }

    #[test]
    fn chaque_evenement_vit_60_secondes_sans_erreur_et_rend_ses_vehicules() {
        for name in GRAND_EVENTS {
            let (left, glints, panels, crafts, frames) = simulate(name, 60.0);
            assert_eq!(left, 0, "{name:?} : {left} véhicule(s) non rendus après end");
            // La voiture roule à 30 m/s dans le banc : la ville-vitrine n'a pas de limiteur, aucun éclair n'est accordé.
            // Chaque événement montre quelque chose, sauf le pouls des fenêtres et la panne, qui ne posent que des effets.
            if !matches!(name, EventName::CityPulse | EventName::Blackout) {
                assert!(
                    glints + panels + crafts > 0,
                    "{name:?} n'a rien montré en 60 s ({frames} images)"
                );
            }
            assert!(frames > 60, "{name:?} s'est arrêté après {frames} images");
        }
    }

    #[test]
    fn les_structures_se_declarent_et_les_autres_non() {
        let mut registry = registry();
        for name in GRAND_EVENTS {
            let mut rig = Rig::new(1);
            let frame = rig.frame();
            let mut ctx = EventCtx {
                frame: &frame,
                fleet: &mut rig.fleet,
                glints: &mut rig.glints,
                walls: &mut rig.walls,
                rng: &mut rig.rng,
                scene: &mut rig.scene,
                signs: None,
            };
            let mut event = registry.create(name, &mut ctx).expect("inscrit");
            // Le catalogue (`names`) et l'événement s'accordent : une structure est exclusive de ses semblables.
            assert_eq!(
                event.structure(),
                name.is_structure_kind(),
                "{name:?} : structure() diffère du catalogue"
            );
            event.end(&mut ctx);
        }
    }

    /// Fait tourner l'orage, les feux d'artifice et la panne ensemble sous le vrai limiteur du trafic, avec la pire
    /// charge : kicks à 10 Hz, drops en rafale. Rend, image par image, les niveaux que l'écran montrerait.
    fn run_through_limiter(policy: Option<(bool, bool)>, seconds: f32) -> Vec<(f32, f32, f32)> {
        let mut registry = registry();
        let mut rig = Rig::new(77);
        let mut flash = TrafficFlash::default();
        let mut events: Vec<Box<dyn TrafficEvent>> = Vec::new();
        for name in [EventName::Storm, EventName::Fireworks, EventName::Blackout] {
            let frame = rig.frame();
            let mut ctx = EventCtx {
                frame: &frame,
                fleet: &mut rig.fleet,
                glints: &mut rig.glints,
                walls: &mut rig.walls,
                rng: &mut rig.rng,
                scene: &mut rig.scene,
                signs: None,
            };
            events.push(registry.create(name, &mut ctx).expect("inscrit"));
        }
        let mut out = Vec::new();
        let dt = 1.0_f32 / 60.0;
        for i in 0..(seconds * 60.0) as usize {
            // `step` ne pilote qu'un événement : on enchaîne les trois sur la même image de la scène.
            let now = f64::from(dt) * i as f64;
            rig.car_z -= f64::from(30.0 * dt);
            rig.time = now;
            let mut frame = rig.frame();
            frame.beat.kick = i % 6 == 0;
            frame.beat.bass = 0.9;
            frame.beat.pulse = if i % 6 < 2 { 1.0 } else { 0.0 };
            frame.drive.drop = i % 90 == 89;
            frame.drive.intensity = 1.0;
            rig.glints.begin(frame.car);
            rig.scene.begin(frame.car);
            for event in &mut events {
                let mut ctx = EventCtx {
                    frame: &frame,
                    fleet: &mut rig.fleet,
                    glints: &mut rig.glints,
                    walls: &mut rig.walls,
                    rng: &mut rig.rng,
                    scene: &mut rig.scene,
                    signs: None,
                };
                event.update(dt, &mut ctx);
            }
            flash.update(dt, now, policy, frame.beat.pulse);
            flash.serve(now, &mut rig.scene.fx);
            let fx = rig.scene.fx;
            // Ce que l'écran montre : le blanchissement de la ville, l'éclat de la salve, la lumière restante en un point
            // fixe de la ville (à 400 m du départ de la voiture).
            let rel = (-400.0 - frame.car.z) as f32;
            let t = ((fx.power.front_rel_m - rel) / fx.power.edge_m.max(1.0) * 0.5 + 0.5).clamp(0.0, 1.0);
            let behind = t * t * (3.0 - 2.0 * t);
            out.push((fx.lightning, fx.burst, 1.0 - fx.power.depth * behind));
        }
        out
    }

    #[test]
    fn sans_spectacle_ou_flashs_coupes_aucun_eclat_ne_sort() {
        for policy in [None, Some((false, false)), Some((true, true))] {
            let frames = run_through_limiter(policy, 40.0);
            assert!(
                frames
                    .iter()
                    .all(|&(lightning, burst, _)| lightning == 0.0 && burst == 0.0),
                "{policy:?} : un éclat est sorti"
            );
        }
    }

    #[test]
    fn sous_charge_hostile_l_eclair_et_les_salves_respectent_le_limiteur() {
        let frames = run_through_limiter(Some((true, false)), 60.0);
        let peak = |f: fn(&(f32, f32, f32)) -> f32| frames.iter().map(f).fold(0.0, f32::max);
        let (lightning_peak, burst_peak) = (peak(|f| f.0), peak(|f| f.1));
        assert!(lightning_peak > 0.9, "l'éclair part : {lightning_peak}");
        assert!(burst_peak > 0.9, "les salves éclatent : {burst_peak}");
        // Plafond de la voie (normalisé : 1 = MAX_FLASH_PEAK).
        assert!(lightning_peak <= 1.0 + 1e-3 && burst_peak <= 1.0 + 1e-3);
        // Aucune variation plus rapide que `MIN_FADE` pour une course complète.
        let max_step = 1.0 / 60.0 / MIN_FADE + 1e-3;
        for pair in frames.windows(2) {
            for (a, b) in [(pair[0].0, pair[1].0), (pair[0].1, pair[1].1)] {
                assert!((a - b).abs() <= max_step, "saut de {} entre deux images", (a - b).abs());
            }
        }
        // Au plus trois départs par seconde glissante, sur chacune des deux voies.
        for channel in [|f: &(f32, f32, f32)| f.0, |f: &(f32, f32, f32)| f.1] {
            let starts: Vec<usize> = (1..frames.len())
                .filter(|&i| channel(&frames[i - 1]) < 0.5 && channel(&frames[i]) >= 0.5)
                .collect();
            for &s in &starts {
                let in_second = starts.iter().filter(|&&t| t >= s && t < s + 60).count();
                assert!(in_second <= MAX_FLASHES_PER_SECOND, "{in_second} départs en 1 s");
            }
        }
    }

    #[test]
    fn la_panne_ne_varie_jamais_plus_vite_que_min_fade_meme_sous_charge() {
        for policy in [Some((true, false)), Some((false, false))] {
            let frames = run_through_limiter(policy, 30.0);
            let darkest = frames.iter().map(|f| f.2).fold(1.0, f32::min);
            assert!(darkest < 0.6, "{policy:?} : la panne passe (plus sombre : {darkest})");
            let max_rate = frames
                .windows(2)
                .map(|w| (w[1].2 - w[0].2).abs() * 60.0)
                .fold(0.0, f32::max);
            assert!(max_rate <= 1.0 / MIN_FADE, "{policy:?} : pente {max_rate} /s");
        }
    }
}
