//! Ce que les cascades savent de la ville (`StuntWorld` de `stunts/types.ts`, `createStuntWorld` de `stunts/world.ts`) :
//! véhicules à piloter, toits, façades, structures au-dessus de la route.

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};

use crate::NeonWorld;
use crate::pilot::flight::{Obstruction, bridge_obstructions};
use crate::pilot::rand::Rand;
use crate::route::bridges::is_near_bridge;
use crate::traffic::motion::Motion;
use crate::traffic::{Craft, CraftId, TrafficDirector};

/// Pas (m) du balayage des arches-immeubles devant la voiture.
const SCAN: f64 = 30.0;

/// Le monde vu par les cascades. Les scripts (J7-07) n'en connaissent que ce trait : testable sans Bevy.
pub trait StuntWorld {
    /// Pose un véhicule que la cascade déplace elle-même (`move_craft`) ; `None` si la flotte est pleine. `tint` est un
    /// rang du cycle prismatique de la palette (l'indice boucle).
    fn spawn(&mut self, variant: u8, at: DVec3, scale: f32, tint: i64) -> Option<CraftId>;
    /// Rend un véhicule posé par [`Self::spawn`].
    fn release(&mut self, id: CraftId);
    /// Le véhicule `id`, s'il est encore vivant (la flotte recycle ce qui sort de la fenêtre).
    fn craft_mut(&mut self, id: CraftId) -> Option<&mut Craft>;
    /// Hauteur du toit sous `(x, z)`, `None` sans immeuble.
    fn roof_at(&self, x: f64, z: f64) -> Option<f32>;
    /// Distance à l'axe de la façade la plus proche d'un côté (−1 gauche, 1 droite) entre `z − depth` et `z + depth` ;
    /// `None` sans immeuble.
    fn facade(&self, side: i8, z: f64, depth: f64) -> Option<f32>;
    /// Structures qui enjambent la route (arches-immeubles, événements) autour de `car_z` : ajoutées à `out`.
    fn overhead(&self, out: &mut Vec<Obstruction>, car_z: f64);
    /// Une arche-immeuble coupe-t-elle la route entre `car_z + 20` et `car_z − distance` ?
    fn bridge_ahead(&self, car_z: f64, distance: f64, margin: f64) -> bool;
    /// Une structure d'événement occupe la route.
    fn busy(&self) -> bool;
    /// Suspend (ou reprend) les événements aléatoires du trafic.
    fn hold(&mut self, on: bool);
}

/// [`StuntWorld`] branché sur le trafic (flotte, événements) et la ville (toits, façades).
#[derive(Debug)]
pub struct TrafficStuntWorld<'a> {
    /// Le trafic : flotte, événements, suspension.
    pub traffic: &'a mut TrafficDirector,
    /// La ville ; `None` : une ville sans immeubles (tests, route seule).
    pub city: Option<&'a NeonWorld>,
    /// Le cycle de teintes prismatiques de la palette.
    pub cycle: [LinearRgba; 6],
    /// Le hasard (graine injectée) : seulement le déphasage de brillance des véhicules posés.
    pub rng: &'a mut Rand,
}

impl StuntWorld for TrafficStuntWorld<'_> {
    fn spawn(&mut self, variant: u8, at: DVec3, scale: f32, tint: i64) -> Option<CraftId> {
        let tint = self.cycle[tint.unsigned_abs() as usize % self.cycle.len()];
        let id = self
            .traffic
            .fleet
            .spawn(variant, at, Vec3::ZERO, tint, scale, self.rng)?;
        // La flotte ne l'avance pas : la cascade écrit sa position à chaque image.
        self.traffic.fleet.get_mut(id)?.motion = Motion::Scripted;
        Some(id)
    }

    fn release(&mut self, id: CraftId) {
        self.traffic.fleet.release(id);
    }

    fn craft_mut(&mut self, id: CraftId) -> Option<&mut Craft> {
        self.traffic.fleet.get_mut(id)
    }

    fn roof_at(&self, x: f64, z: f64) -> Option<f32> {
        self.city?.roof_at(x, z)
    }

    fn facade(&self, side: i8, z: f64, depth: f64) -> Option<f32> {
        self.city?.facade(side, z, depth)
    }

    fn overhead(&self, out: &mut Vec<Obstruction>, car_z: f64) {
        out.extend_from_slice(self.traffic.obstructions());
        bridge_obstructions(car_z, out);
    }

    fn bridge_ahead(&self, car_z: f64, distance: f64, margin: f64) -> bool {
        let mut z = car_z + 20.0;
        while z > car_z - distance {
            if is_near_bridge(z, margin) {
                return true;
            }
            z -= SCAN;
        }
        false
    }

    fn busy(&self) -> bool {
        self.traffic.structure_busy()
    }

    fn hold(&mut self, on: bool) {
        self.traffic.hold(on);
    }
}
