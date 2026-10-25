//! Banc d'essai des événements : ce que le directeur leur donne à chaque image, sans Bevy, pour les tests des lots A et B.

use bevy::color::LinearRgba;
use bevy::math::DVec3;

use crate::pilot::rand::Rand;
use crate::traffic::event::{EventCtx, TrafficEvent};
use crate::traffic::fleet::Fleet;
use crate::traffic::frame::TrafficFrame;
use crate::traffic::glints::Glints;
use crate::traffic::scene::Scene;
use crate::traffic::walls::LightWalls;

/// Le monde d'un événement de test : voiture en croisière à 30 m/s, cycle de teintes distinctes.
#[derive(Debug)]
pub(crate) struct Rig {
    pub(crate) fleet: Fleet,
    pub(crate) glints: Glints,
    pub(crate) walls: LightWalls,
    pub(crate) scene: Scene,
    pub(crate) rng: Rand,
    pub(crate) car_z: f64,
    pub(crate) time: f64,
}

/// Vitesse de la voiture du banc, m/s.
pub(crate) const SPEED: f32 = 30.0;

impl Rig {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            fleet: Fleet::new(),
            glints: Glints::new(),
            walls: LightWalls::new(),
            scene: Scene::new(),
            rng: Rand::new(seed),
            car_z: 0.0,
            time: 0.0,
        }
    }

    /// L'image courante : la voiture en vol de croisière, six teintes distinctes dans le cycle.
    pub(crate) fn frame(&self) -> TrafficFrame<'static> {
        TrafficFrame {
            dt: 1.0 / 60.0,
            time: self.time,
            car: DVec3::new(0.0, 0.0, self.car_z),
            speed: SPEED,
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

    /// Fabrique un événement avec l'image courante.
    pub(crate) fn start<E>(&mut self, build: impl FnOnce(&mut EventCtx<'_>) -> E) -> E {
        let frame = self.frame();
        let mut ctx = EventCtx {
            frame: &frame,
            fleet: &mut self.fleet,
            glints: &mut self.glints,
            walls: &mut self.walls,
            rng: &mut self.rng,
            scene: &mut self.scene,
            signs: None,
        };
        build(&mut ctx)
    }

    /// Avance d'une image de `dt` s : la voiture roule, les points et la scène sont vidés, l'événement est mis à jour,
    /// puis la flotte et les murs. `tweak` règle l'image avant l'appel (audio, spectacle…). Faux quand l'événement finit.
    pub(crate) fn step<E: TrafficEvent + ?Sized>(
        &mut self,
        event: &mut E,
        dt: f32,
        tweak: impl FnOnce(&mut TrafficFrame<'_>),
    ) -> bool {
        self.car_z -= f64::from(SPEED * dt);
        self.time += f64::from(dt);
        let mut frame = self.frame();
        frame.dt = dt;
        tweak(&mut frame);
        self.glints.begin(frame.car);
        self.scene.begin(frame.car);
        let alive = {
            let mut ctx = EventCtx {
                frame: &frame,
                fleet: &mut self.fleet,
                glints: &mut self.glints,
                walls: &mut self.walls,
                rng: &mut self.rng,
                scene: &mut self.scene,
                signs: None,
            };
            event.update(dt, &mut ctx)
        };
        self.fleet.update(dt, frame.car.z, |_| true);
        self.walls.update(frame.time);
        alive
    }

    /// Arrête l'événement, comme le directeur le fait de force ou à sa fin.
    pub(crate) fn end<E: TrafficEvent + ?Sized>(&mut self, event: &mut E) {
        let frame = self.frame();
        let mut ctx = EventCtx {
            frame: &frame,
            fleet: &mut self.fleet,
            glints: &mut self.glints,
            walls: &mut self.walls,
            rng: &mut self.rng,
            scene: &mut self.scene,
            signs: None,
        };
        event.end(&mut ctx);
    }

    /// Nombre de véhicules vivants dans la flotte.
    pub(crate) fn active_crafts(&self) -> usize {
        self.fleet.crafts().iter().filter(|c| c.active).count()
    }
}
