//! La flotte (`fleet.ts`) : tous les véhicules volants de la ville, en logique pure. Les trajectoires sont décidées par
//! les événements et le trafic ; la flotte se contente de les animer, de les orienter dans le sens du mouvement (avec
//! un roulis qui suit les virages) et de les recycler.
//!
//! Sillage dessinait un `InstancedMesh` par modèle. Ici chaque emplacement est une entité préallouée (voir `render`) :
//! la flotte ne connaît pas Bevy, elle range le résultat dans [`CraftPose`] que le rendu recopie.
//!
//! # Cadence
//!
//! [`Fleet::update`] demande à l'appelant, pour chaque véhicule, si c'est son tour (`SimulationLod` du culling de
//! simulation). Un véhicule qui n'a pas son tour accumule le temps et l'intègre d'un coup au suivant : sa trajectoire
//! est une fonction de son âge, il n'en est pas ralenti.

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Mat3, Quat, Vec3};

use super::motion::Motion;
use super::variant::{CAPACITY, POOL, SLEEK, VARIANT_COUNT};
use crate::pilot::rand::Rand;

/// Identifiant d'un véhicule : son rang dans le pool, stable tant qu'il vit.
pub type CraftId = u16;

/// Derrière la voiture, un véhicule est recyclé au-delà de cette distance (m).
const RECYCLE_BEHIND_M: f64 = 280.0;
/// Devant la voiture, au-delà de cette distance (m).
const RECYCLE_AHEAD_M: f64 = 720.0;
/// Écart latéral (m), altitude haute et basse (m) au-delà desquels un véhicule est recyclé.
const RECYCLE_X_M: f64 = 520.0;
const RECYCLE_Y_MAX_M: f64 = 520.0;
const RECYCLE_Y_MIN_M: f64 = -90.0;

/// Ce que le rendu recopie d'un véhicule après son dernier pas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CraftPose {
    /// Orientation du modèle (il regarde vers −Z).
    pub rotation: Quat,
    /// Origine de la traînée, absolue (m).
    pub trail_position: DVec3,
    /// Largeur, hauteur et longueur de la traînée ; nul quand elle est éteinte.
    pub trail_scale: Vec3,
}

impl Default for CraftPose {
    fn default() -> Self {
        Self {
            rotation: Quat::IDENTITY,
            trail_position: DVec3::ZERO,
            trail_scale: Vec3::ZERO,
        }
    }
}

/// Un véhicule du pool.
#[derive(Debug, Clone)]
pub struct Craft {
    /// Vivant (sinon l'emplacement est libre).
    pub active: bool,
    /// Rang dans le pool.
    pub id: CraftId,
    /// Modèle (voir [`super::variant`]).
    pub variant: u8,
    /// Emplacement parmi ceux de son modèle.
    pub slot: u16,
    /// Position absolue, m.
    pub position: DVec3,
    /// Vitesse dans le monde, m/s. Le cap en découle, sauf si le mouvement la réécrit.
    pub velocity: Vec3,
    /// Le haut du véhicule.
    pub up: Vec3,
    /// Échelle (longueur réelle = échelle × [`super::variant::CRAFT_LENGTH`]).
    pub scale: f32,
    /// Teinte de la carrosserie et de la traînée (lumière linéaire).
    pub tint: LinearRgba,
    /// Longueur relative de la traînée (0 = aucune).
    pub trail: f32,
    /// Âge, s.
    pub age: f32,
    /// Roulis, rad.
    pub roll: f32,
    /// Cabrage en rad : positif, le nez se lève (wheeling), autour de l'origine du modèle.
    pub pitch: f32,
    /// Multiplicateur du roulis en virage : les motos se couchent bien plus que les vaisseaux.
    pub lean: f32,
    /// Déphasage tiré à la naissance, 0..1 (réservé aux effets par véhicule).
    pub flash: f32,
    /// Mouvement propre.
    pub motion: Motion,
    /// Cap lissé (unitaire).
    pub heading: Vec3,
    /// Monte à chaque naissance : le rendu sait qu'il faut réécrire teinte et visibilité.
    pub generation: u32,
    /// Pose calculée au dernier pas.
    pub pose: CraftPose,
    /// Temps écoulé depuis son dernier pas, s.
    pending_s: f32,
}

impl Craft {
    fn idle(id: CraftId) -> Self {
        Self {
            active: false,
            id,
            variant: 0,
            slot: 0,
            position: DVec3::ZERO,
            velocity: Vec3::ZERO,
            up: Vec3::Y,
            scale: 1.0,
            tint: LinearRgba::BLACK,
            trail: 1.0,
            age: 0.0,
            roll: 0.0,
            pitch: 0.0,
            lean: 1.0,
            flash: 0.0,
            motion: Motion::Linear,
            heading: Vec3::NEG_Z,
            generation: 0,
            pose: CraftPose::default(),
            pending_s: 0.0,
        }
    }
}

/// Dimensions d'un modèle dans son repère (avant mise à l'échelle) : de quoi poser la traînée.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ModelDims {
    /// Demi-largeur.
    pub half_width: f32,
    /// Abscisse z de l'arrière.
    pub rear: f32,
}

/// Compteurs de l'image : ce que la cadence réduite économise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FleetStats {
    /// Véhicules vivants.
    pub active: u32,
    /// Véhicules dont c'était le tour cette image.
    pub stepped: u32,
    /// Véhicules dont ce n'était pas le tour (animation différée, rattrapée au tour suivant).
    pub skipped: u32,
    /// Total des pas depuis la création.
    pub stepped_total: u64,
    /// Total des tours sautés depuis la création.
    pub skipped_total: u64,
}

/// Tous les véhicules volants de la ville.
#[derive(Debug, Clone)]
pub struct Fleet {
    crafts: Vec<Craft>,
    /// Emplacements libres de chaque modèle (le dernier rendu ressort le premier).
    free: Vec<Vec<u16>>,
    dims: [ModelDims; VARIANT_COUNT],
    stats: FleetStats,
    generations: u32,
}

impl Default for Fleet {
    fn default() -> Self {
        Self::new()
    }
}

impl Fleet {
    /// Une flotte vide aux capacités de Sillage.
    #[must_use]
    pub fn new() -> Self {
        let crafts = (0..POOL).map(|i| Craft::idle(i as CraftId)).collect();
        let free = CAPACITY
            .iter()
            .map(|&capacity| (0..capacity).rev().map(|i| i as u16).collect())
            .collect();
        Self {
            crafts,
            free,
            dims: [ModelDims::default(); VARIANT_COUNT],
            stats: FleetStats::default(),
            generations: 0,
        }
    }

    /// Les dimensions d'un modèle, connues une fois le modèle chargé.
    pub fn set_dims(&mut self, variant: u8, dims: ModelDims) {
        if let Some(slot) = self.dims.get_mut(usize::from(variant)) {
            *slot = dims;
        }
    }

    /// Choisit un modèle profilé (`r` dans [0, 1)).
    #[must_use]
    pub fn sleek(r: f64) -> u8 {
        SLEEK[((r * SLEEK.len() as f64) as usize).min(SLEEK.len() - 1)]
    }

    /// Pose un véhicule. `None` si le pool ou les emplacements du modèle sont pleins : l'appelant n'insiste pas.
    pub fn spawn(
        &mut self,
        variant: u8,
        position: DVec3,
        velocity: Vec3,
        tint: LinearRgba,
        scale: f32,
        rng: &mut Rand,
    ) -> Option<CraftId> {
        let slots = self.free.get_mut(usize::from(variant))?;
        let craft = self.crafts.iter_mut().find(|c| !c.active)?;
        let slot = slots.pop()?;
        self.generations = self.generations.wrapping_add(1);
        craft.active = true;
        craft.variant = variant;
        craft.slot = slot;
        craft.position = position;
        craft.velocity = velocity;
        craft.up = Vec3::Y;
        craft.scale = scale;
        craft.tint = tint;
        craft.trail = 1.0;
        craft.age = 0.0;
        craft.roll = 0.0;
        craft.pitch = 0.0;
        craft.lean = 1.0;
        craft.flash = rng.unit() as f32;
        craft.motion = Motion::Linear;
        craft.pending_s = 0.0;
        craft.generation = self.generations;
        craft.pose = CraftPose::default();
        if velocity.length_squared() > 1e-6 {
            craft.heading = velocity.normalize();
        }
        Some(craft.id)
    }

    /// Le véhicule `id`, s'il est vivant.
    #[must_use]
    pub fn get(&self, id: CraftId) -> Option<&Craft> {
        self.crafts.get(usize::from(id)).filter(|c| c.active)
    }

    /// Le véhicule `id`, s'il est vivant, pour que son événement le règle.
    pub fn get_mut(&mut self, id: CraftId) -> Option<&mut Craft> {
        self.crafts.get_mut(usize::from(id)).filter(|c| c.active)
    }

    /// Tous les emplacements du pool, vivants ou non (le rendu les parcourt).
    #[must_use]
    pub fn crafts(&self) -> &[Craft] {
        &self.crafts
    }

    /// Parcourt les véhicules vivants (lecture seule) : l'espace aérien et les sujets en ont besoin.
    pub fn for_each_active(&self, mut visit: impl FnMut(&Craft)) {
        for craft in self.crafts.iter().filter(|c| c.active) {
            visit(craft);
        }
    }

    /// Rend un véhicule. Sans effet s'il est déjà libre.
    pub fn release(&mut self, id: CraftId) {
        let Some(craft) = self.crafts.get_mut(usize::from(id)).filter(|c| c.active) else {
            return;
        };
        craft.active = false;
        craft.motion = Motion::Linear;
        if let Some(slots) = self.free.get_mut(usize::from(craft.variant)) {
            slots.push(craft.slot);
        }
    }

    /// Rend tout (arrêt du spectacle).
    pub fn clear(&mut self) {
        for id in 0..self.crafts.len() {
            self.release(id as CraftId);
        }
    }

    /// Compteurs de la dernière image.
    #[must_use]
    pub fn stats(&self) -> FleetStats {
        self.stats
    }

    /// Avance les véhicules de `dt` secondes. `due(craft)` dit si c'est le tour du véhicule (cadence réduite hors
    /// champ) ; `car_z` sert à recycler ce qui est trop loin.
    pub fn update(&mut self, dt: f32, car_z: f64, mut due: impl FnMut(&Craft) -> bool) {
        let mut stats = FleetStats {
            stepped_total: self.stats.stepped_total,
            skipped_total: self.stats.skipped_total,
            ..FleetStats::default()
        };
        for index in 0..self.crafts.len() {
            if !self.crafts[index].active {
                continue;
            }
            stats.active += 1;
            self.crafts[index].pending_s += dt;
            if !due(&self.crafts[index]) {
                stats.skipped += 1;
                continue;
            }
            stats.stepped += 1;
            if !self.step(index, car_z) {
                self.release(index as CraftId);
            }
        }
        stats.stepped_total += u64::from(stats.stepped);
        stats.skipped_total += u64::from(stats.skipped);
        self.stats = stats;
    }

    /// Un pas du véhicule `index` sur le temps accumulé. Faux quand il sort de la fenêtre : il est à recycler.
    fn step(&mut self, index: usize, car_z: f64) -> bool {
        let dims = self.dims[usize::from(self.crafts[index].variant)];
        let craft = &mut self.crafts[index];
        let dt = std::mem::take(&mut craft.pending_s);
        craft.age += dt;
        let motion = craft.motion;
        motion.step(&mut craft.position, &mut craft.velocity, craft.age, dt);

        let p = craft.position;
        if p.z > car_z + RECYCLE_BEHIND_M
            || p.z < car_z - RECYCLE_AHEAD_M
            || p.x.abs() > RECYCLE_X_M
            || p.y > RECYCLE_Y_MAX_M
            || p.y < RECYCLE_Y_MIN_M
        {
            return false;
        }

        let speed = craft.velocity.length();
        let forward = if speed > 0.01 {
            craft.velocity / speed
        } else {
            craft.heading
        };

        // Le roulis suit le taux de virage : le véhicule se couche dans ses courbes.
        if dt > 0.0 {
            let yaw_rate = craft.heading.cross(forward).y / dt;
            let target = (-yaw_rate * 0.35 * craft.lean).clamp(-0.7, 0.7);
            craft.roll += (target - craft.roll) * (1.0 - (-dt * 6.0).exp());
        }
        craft.heading = forward;

        let [x, y, z] = basis(forward, craft.up, craft.roll, craft.pitch);
        craft.pose.rotation = Quat::from_mat3(&Mat3::from_cols(x, y, z));

        // Traînée : démarre au cul du véhicule, s'allonge avec la vitesse.
        if craft.trail <= 0.0 {
            craft.pose.trail_scale = Vec3::ZERO;
        } else {
            let length = (1.1 + speed * 0.05) * craft.trail * craft.scale;
            let width = dims.half_width * 0.8 * craft.scale;
            craft.pose.trail_scale = Vec3::new(width, width, length);
            craft.pose.trail_position = craft.position + (z * (dims.rear * craft.scale)).as_dvec3();
        }
        true
    }
}

#[allow(clippy::many_single_char_names)] // x, y, z, s, c : les axes et le sinus-cosinus, comme dans `fleet.ts`.
/// Les trois axes du véhicule (x à droite, y en haut, z vers l'arrière) pour un cap, un haut, un roulis et un cabrage.
#[must_use]
pub fn basis(forward: Vec3, up: Vec3, roll: f32, pitch: f32) -> [Vec3; 3] {
    let mut z = -forward;
    let mut x = up.cross(z);
    if x.length_squared() < 1e-4 {
        x = Vec3::X;
    }
    x = x.normalize();
    let mut y = z.cross(x);
    let (s, c) = roll.sin_cos();
    let rolled_x = x * c + y * s;
    y = y * c - x * s;
    x = rolled_x;
    if pitch != 0.0 {
        // Nez levé : la proue (−Z) monte vers Y, autour de l'axe X.
        let (sp, cp) = pitch.sin_cos();
        let pitched_z = z * cp - y * sp;
        y = y * cp + z * sp;
        z = pitched_z;
    }
    [x, y, z]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn_at(fleet: &mut Fleet, rng: &mut Rand, variant: u8, z: f64, vz: f32) -> CraftId {
        fleet
            .spawn(
                variant,
                DVec3::new(0.0, 15.0, z),
                Vec3::new(0.0, 0.0, vz),
                LinearRgba::WHITE,
                1.9,
                rng,
            )
            .expect("place libre")
    }

    #[test]
    fn les_emplacements_d_un_modele_s_epuisent_puis_reviennent() {
        let mut fleet = Fleet::new();
        let mut rng = Rand::new(1);
        let ids: Vec<_> = (0..6)
            .map(|_| spawn_at(&mut fleet, &mut rng, 6, -100.0, -50.0))
            .collect();
        // Le septième cargo n'a plus d'emplacement.
        assert!(
            fleet
                .spawn(6, DVec3::ZERO, Vec3::ZERO, LinearRgba::WHITE, 1.0, &mut rng)
                .is_none()
        );
        fleet.release(ids[2]);
        fleet.release(ids[2]);
        assert!(
            fleet
                .spawn(6, DVec3::ZERO, Vec3::ZERO, LinearRgba::WHITE, 1.0, &mut rng)
                .is_some()
        );
    }

    #[test]
    fn le_pool_total_est_plafonne_a_pool() {
        let mut fleet = Fleet::new();
        let mut rng = Rand::new(1);
        let mut n = 0;
        'fill: for variant in 0..VARIANT_COUNT as u8 {
            for _ in 0..CAPACITY[usize::from(variant)] {
                if fleet
                    .spawn(variant, DVec3::ZERO, Vec3::ZERO, LinearRgba::WHITE, 1.0, &mut rng)
                    .is_none()
                {
                    break 'fill;
                }
                n += 1;
            }
        }
        assert_eq!(n, CAPACITY.iter().sum::<usize>());
        assert!(n <= POOL);
    }

    #[test]
    fn un_vehicule_avance_et_sort_de_la_fenetre() {
        let mut fleet = Fleet::new();
        let mut rng = Rand::new(2);
        let id = spawn_at(&mut fleet, &mut rng, 0, 100.0, 100.0);
        // Il fuit vers l'arrière de la voiture (z croissant) : à 100 m/s il dépasse 280 m derrière en ~2 s.
        for _ in 0..180 {
            fleet.update(1.0 / 60.0, 0.0, |_| true);
        }
        assert!(fleet.get(id).is_none(), "recyclé");
        assert_eq!(fleet.stats().active, 0);
    }

    #[test]
    fn la_cadence_reduite_ne_change_pas_la_position_finale() {
        let mut rng = Rand::new(4);
        let (mut a, mut b) = (Fleet::new(), Fleet::new());
        let ia = spawn_at(&mut a, &mut rng, 0, -50.0, -60.0);
        let ib = spawn_at(&mut b, &mut rng, 0, -50.0, -60.0);
        for frame in 0..240_u32 {
            a.update(1.0 / 60.0, 0.0, |_| true);
            b.update(1.0 / 60.0, 0.0, |_| frame % 8 == 7);
        }
        let (pa, pb) = (a.get(ia).map(|c| c.position.z), b.get(ib).map(|c| c.position.z));
        assert!((pa.unwrap_or(0.0) - pb.unwrap_or(1e9)).abs() < 1e-3, "{pa:?} {pb:?}");
        assert_eq!(b.stats().stepped_total, 30);
        assert_eq!(b.stats().skipped_total, 210);
    }

    #[test]
    fn le_vehicule_regarde_vers_son_cap() {
        let mut fleet = Fleet::new();
        let mut rng = Rand::new(6);
        let id = spawn_at(&mut fleet, &mut rng, 0, -50.0, -60.0);
        fleet.update(1.0 / 60.0, 0.0, |_| true);
        let craft = fleet.get(id).expect("vivant");
        // Le modèle regarde vers −Z : tourné par la pose, il regarde vers −Z ici aussi.
        let nose = craft.pose.rotation * Vec3::NEG_Z;
        assert!((nose - Vec3::NEG_Z).length() < 1e-4, "{nose}");
    }

    #[test]
    fn la_base_est_orthonormee_avec_roulis_et_cabrage() {
        let [x, y, z] = basis(Vec3::new(0.3, 0.1, -1.0).normalize(), Vec3::Y, 0.4, 0.3);
        for (a, b) in [(x, y), (y, z), (x, z)] {
            assert!(a.dot(b).abs() < 1e-4);
        }
        for v in [x, y, z] {
            assert!((v.length() - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn une_traine_eteinte_est_de_taille_nulle() {
        let mut fleet = Fleet::new();
        fleet.set_dims(
            0,
            ModelDims {
                half_width: 0.5,
                rear: 1.2,
            },
        );
        let mut rng = Rand::new(8);
        let id = spawn_at(&mut fleet, &mut rng, 0, -50.0, -60.0);
        fleet.update(1.0 / 60.0, 0.0, |_| true);
        assert!(fleet.get(id).is_some_and(|c| c.pose.trail_scale.z > 0.0));
        if let Some(craft) = fleet.get_mut(id) {
            craft.trail = 0.0;
        }
        fleet.update(1.0 / 60.0, 0.0, |_| true);
        assert_eq!(fleet.get(id).map(|c| c.pose.trail_scale), Some(Vec3::ZERO));
    }
}
