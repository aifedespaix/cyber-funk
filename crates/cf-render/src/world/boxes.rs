//! La ville de boîtes : implémentation de test du trait [`World`].
//!
//! Une ville générique qui reprend la géométrie de `scenery.ts` (rangées de 26 m, six couronnes d'immeubles de
//! chaque côté, 15 rangées derrière et 19 devant), en simples boîtes. Elle sert aux tests sans rendu du streaming,
//! puis de scène de référence pour le culling (J4-04, densité réglable) et la surcouche de diagnostic (J4-05).

use bevy::ecs::world::World as EcsWorld;
use bevy::prelude::*;
use cf_show::model::ShowGates;

use super::{Replan, Subject, SubjectContext, SubjectKind, SubjectSink, World, WorldError};
use crate::culling::{FOG_FULL_M, until_fog};
use crate::streaming::{ArchetypeId, ChunkInfo, ChunkPlan, PieceRegistry, StreamLayout, splitmix64};

/// Longueur d'un tronçon : l'écart entre deux rangées d'immeubles (`SPACING` de `scenery.ts`), en m.
const CHUNK_LENGTH_M: f32 = 26.0;
/// Rangées gardées derrière la voiture (`BEHIND` de `scenery.ts`) : 390 m. La caméra ne recule pas à plus de 90 m
/// derrière la voiture (`MAX_BEHIND` de `cinema/director.ts`) : il lui reste 300 m de ville, noyés dans la brume.
const BEHIND: u32 = 15;
/// Rangées devant : `ROWS` (34) − `BEHIND` − la rangée courante. La ville s'étend à ~494 m devant ; la caméra
/// n'avance pas à plus de 190 m devant la voiture (`MAX_AHEAD`), il lui reste donc ~300 m de ville elle aussi.
const AHEAD: u32 = 18;
/// Immeubles par rangée : six couronnes, des deux côtés (`SLOTS` de `scenery.ts`).
const SLOTS_PER_ROW: u16 = 12;
/// Décalage aléatoire d'une rangée le long de la route, en m (`(hash − 0,5) × 6` dans `scenery.ts`).
const ROW_JITTER_M: f32 = 6.0;
/// Part des tronçons qui portent une tour-repère (sujet « landmark » pour la caméra).
const LANDMARK_CHANCE: f32 = 0.08;
/// Voitures garées par tronçon, portes grandes ouvertes (`ground_traffic` = 1).
const MAX_CARS: u16 = 4;
/// Axes des voies (m), chaussée |x| < 9 (`cinema/types.ts`).
const LANES: [f32; 4] = [-6.3, -2.1, 2.1, 6.3];

/// Une couronne d'immeubles (`LAYERS` de `scenery.ts`) : distance à l'axe, étalement, plages de taille, part sautée.
struct Layer {
    x: f32,
    spread: f32,
    width: [f32; 2],
    depth: [f32; 2],
    height: [f32; 2],
    skip: f32,
}

/// Premier plan trapu collé à la route, deux couronnes plus hautes, trois couronnes d'horizon (valeurs de Sillage).
const LAYERS: [Layer; 6] = [
    Layer {
        x: 13.5,
        spread: 3.0,
        width: [8.0, 13.0],
        depth: [8.0, 12.0],
        height: [9.0, 28.0],
        skip: 0.22,
    },
    Layer {
        x: 34.0,
        spread: 6.0,
        width: [10.0, 18.0],
        depth: [10.0, 16.0],
        height: [20.0, 50.0],
        skip: 0.25,
    },
    Layer {
        x: 58.0,
        spread: 10.0,
        width: [12.0, 22.0],
        depth: [12.0, 20.0],
        height: [38.0, 85.0],
        skip: 0.1,
    },
    Layer {
        x: 98.0,
        spread: 14.0,
        width: [16.0, 30.0],
        depth: [16.0, 24.0],
        height: [70.0, 140.0],
        skip: 0.12,
    },
    Layer {
        x: 150.0,
        spread: 20.0,
        width: [20.0, 36.0],
        depth: [18.0, 26.0],
        height: [100.0, 200.0],
        skip: 0.1,
    },
    Layer {
        x: 214.0,
        spread: 30.0,
        width: [26.0, 46.0],
        depth: [20.0, 28.0],
        height: [140.0, 280.0],
        skip: 0.1,
    },
];

/// Sels des tirages, pour que chaque grandeur ait son propre hasard.
mod salt {
    pub const SKIP: u64 = 1;
    pub const WIDTH: u64 = 2;
    pub const DEPTH: u64 = 3;
    pub const HEIGHT: u64 = 4;
    pub const SPREAD: u64 = 5;
    pub const JITTER: u64 = 6;
    pub const TINT: u64 = 7;
    pub const LANDMARK: u64 = 20;
    pub const LANDMARK_SIDE: u64 = 21;
    pub const LANDMARK_X: u64 = 22;
    pub const LANDMARK_HEIGHT: u64 = 23;
    pub const CAR: u64 = 30;
}

/// La ville de boîtes. Configuration : graine et densité ; état : voitures garées par tronçon (porte
/// `ground_traffic`).
#[derive(Resource, Debug, Clone)]
pub struct BoxWorld {
    seed: u64,
    rows_per_chunk: u16,
    cars_per_chunk: u16,
    /// Distance caméra au-delà de laquelle les immeubles ne sont plus dessinés (`VisibilityRange`), `None` : jamais.
    fog_cull_m: Option<f32>,
    boxes: ArchetypeId,
    landmarks: ArchetypeId,
    cars: ArchetypeId,
}

impl Default for BoxWorld {
    fn default() -> Self {
        Self::new(0x00C1_7EB0)
    }
}

impl BoxWorld {
    /// Ville à la densité de Sillage : une rangée de 12 immeubles par tronçon.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            rows_per_chunk: 1,
            cars_per_chunk: MAX_CARS,
            fog_cull_m: Some(FOG_FULL_M),
            boxes: ArchetypeId::default(),
            landmarks: ArchetypeId::default(),
            cars: ArchetypeId::default(),
        }
    }

    /// Ville plus dense : `rows` rangées de 12 immeubles par tronçon (13 rangées ≈ 5 300 boîtes vivantes, J4-04).
    #[must_use]
    pub fn with_rows_per_chunk(mut self, rows: u16) -> Self {
        self.rows_per_chunk = rows.max(1);
        self
    }

    /// Distance (m) au-delà de laquelle les immeubles ne sont plus dessinés, `None` pour tout dessiner jusqu'au bout
    /// de la fenêtre. Par défaut [`FOG_FULL_M`] : la brume de Sillage a alors tout couvert. Les tours-repères et les
    /// voitures n'ont pas de limite (ce sont des sujets de caméra). Lue au démarrage du monde (`setup`).
    #[must_use]
    pub fn with_fog_cull(mut self, end_m: Option<f32>) -> Self {
        self.fog_cull_m = end_m;
        self
    }

    /// Distance de coupe des immeubles, si elle est réglée.
    #[must_use]
    pub fn fog_cull_m(&self) -> Option<f32> {
        self.fog_cull_m
    }

    /// Capacité d'immeubles par tronçon.
    #[must_use]
    pub fn boxes_per_chunk(&self) -> u16 {
        self.rows_per_chunk.saturating_mul(SLOTS_PER_ROW)
    }

    /// Voitures garées dans chaque tronçon tiré à partir de maintenant.
    #[must_use]
    pub fn cars_per_chunk(&self) -> u16 {
        self.cars_per_chunk
    }

    /// La tour-repère du tronçon, s'il en a une : centre (repère local) et hauteur (m).
    fn landmark(chunk: &ChunkInfo) -> Option<(Vec3, f32)> {
        if hash01(chunk.seed, salt::LANDMARK) >= LANDMARK_CHANCE {
            return None;
        }
        let side = if hash01(chunk.seed, salt::LANDMARK_SIDE) < 0.5 {
            -1.0
        } else {
            1.0
        };
        let x = side * (70.0 + 20.0 * hash01(chunk.seed, salt::LANDMARK_X));
        let height = 300.0 + 120.0 * hash01(chunk.seed, salt::LANDMARK_HEIGHT);
        Some((Vec3::new(x, height / 2.0, -chunk.length_m / 2.0), height))
    }

    fn plan_buildings(&self, chunk: &ChunkInfo, plan: &mut ChunkPlan) {
        let rows = self.rows_per_chunk.max(1);
        let row_length = chunk.length_m / f32::from(rows);
        for row in 0..rows {
            for slot in 0..SLOTS_PER_ROW {
                let seed = chunk.seed ^ splitmix64((u64::from(row) << 8) | u64::from(slot));
                let layer = &LAYERS[usize::from(slot / 2) % LAYERS.len()];
                if hash01(seed, salt::SKIP) < layer.skip {
                    continue;
                }
                let side = if slot % 2 == 0 { -1.0 } else { 1.0 };
                let size = Vec3::new(
                    lerp(layer.width, hash01(seed, salt::WIDTH)),
                    lerp(layer.height, hash01(seed, salt::HEIGHT).powf(1.4)),
                    lerp(layer.depth, hash01(seed, salt::DEPTH)),
                );
                let x = side * (layer.x + hash01(seed, salt::SPREAD) * layer.spread + size.x / 2.0);
                let z = -(f32::from(row) + 0.5) * row_length + (hash01(seed, salt::JITTER) - 0.5) * ROW_JITTER_M;
                // Étiquette : graine 0..999 et couronne, comme les attributs d'instance de `scenery.ts`.
                let tag = (hash01(seed, salt::TINT) * 1000.0) as u32 | (u32::from(slot / 2) << 16);
                let transform = Transform::from_xyz(x, size.y / 2.0, z).with_scale(size);
                plan.push(self.boxes, transform, tag);
            }
        }
    }
}

impl World for BoxWorld {
    const ID: &'static str = "boxes";
    type SubjectParam = ();

    fn layout(&self) -> StreamLayout {
        StreamLayout {
            chunk_length_m: CHUNK_LENGTH_M,
            behind: BEHIND,
            ahead: AHEAD,
            seed: self.seed,
        }
    }

    fn setup(&mut self, ecs: &mut EcsWorld, pieces: &mut PieceRegistry) -> Result<(), WorldError> {
        let mesh = ecs
            .get_resource_mut::<Assets<Mesh>>()
            .ok_or(WorldError::MissingResource("Assets<Mesh>"))?
            .add(Cuboid::new(1.0, 1.0, 1.0));
        let mut materials = ecs
            .get_resource_mut::<Assets<StandardMaterial>>()
            .ok_or(WorldError::MissingResource("Assets<StandardMaterial>"))?;
        let facade = materials.add(StandardMaterial {
            base_color: Color::srgb(0.08, 0.09, 0.14),
            perceptual_roughness: 0.8,
            ..default()
        });
        let tower = materials.add(StandardMaterial {
            base_color: Color::srgb(0.10, 0.05, 0.20),
            emissive: LinearRgba::rgb(0.6, 0.1, 1.2),
            ..default()
        });
        let car = materials.add(StandardMaterial {
            emissive: LinearRgba::rgb(2.0, 0.3, 0.6),
            ..default()
        });
        self.boxes = match self.fog_cull_m {
            // Au-delà de la brume l'immeuble est de la couleur du fond : Bevy le saute avant même le frustum.
            Some(end_m) => pieces.add(
                "box",
                self.boxes_per_chunk(),
                (Mesh3d(mesh.clone()), MeshMaterial3d(facade), until_fog(end_m)),
            ),
            None => pieces.add(
                "box",
                self.boxes_per_chunk(),
                (Mesh3d(mesh.clone()), MeshMaterial3d(facade)),
            ),
        };
        self.landmarks = pieces.add("landmark", 1, (Mesh3d(mesh.clone()), MeshMaterial3d(tower)));
        self.cars = pieces.add("car", MAX_CARS, (Mesh3d(mesh), MeshMaterial3d(car)));
        Ok(())
    }

    fn plan_chunk(&self, chunk: &ChunkInfo, plan: &mut ChunkPlan) {
        self.plan_buildings(chunk, plan);
        if let Some((center, height)) = Self::landmark(chunk) {
            let transform = Transform::from_translation(center).with_scale(Vec3::new(24.0, height, 24.0));
            plan.push(self.landmarks, transform, 0);
        }
        for (k, lane) in LANES.iter().take(usize::from(self.cars_per_chunk)).enumerate() {
            let z = -hash01(chunk.seed, salt::CAR + k as u64) * chunk.length_m;
            let transform = Transform::from_xyz(*lane, 0.65, z).with_scale(Vec3::new(1.9, 1.3, 4.6));
            plan.push(self.cars, transform, k as u32);
        }
    }

    fn apply_gates(&mut self, gates: &ShowGates) -> Replan {
        // Quantifié : un fondu de portes appelle cette méthode à chaque image, la densité ne change que par voiture
        // entière. Les tronçons déjà là gardent leurs voitures ; les nouveaux arrivent au loin avec la bonne densité.
        self.cars_per_chunk = (gates.ground_traffic.clamp(0.0, 1.0) * f32::from(MAX_CARS)).round() as u16;
        Replan::Keep
    }

    fn subjects(&self, ctx: &SubjectContext<'_>, &(): &(), out: &mut SubjectSink<'_>) {
        for chunk in ctx.chunks.iter() {
            if let Some((center, height)) = Self::landmark(&chunk) {
                out.push(Subject {
                    id: chunk.index as u64,
                    kind: SubjectKind::Landmark,
                    position: chunk.anchor + center.as_dvec3(),
                    velocity: Vec3::ZERO,
                    size: height,
                    facing: None,
                });
            }
        }
    }
}

/// Tirage déterministe dans [0, 1) pour (graine, sel) : l'équivalent de `hash(seed, salt)` de `random.ts`, en entier
/// (stable d'une plateforme à l'autre, contrairement au `sin` de Sillage).
fn hash01(seed: u64, salt: u64) -> f32 {
    let bits = splitmix64(seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15)) >> 40;
    bits as f32 / (1_u64 << 24) as f32
}

fn lerp(range: [f32; 2], t: f32) -> f32 {
    range[0] + (range[1] - range[0]) * t
}

#[cfg(test)]
mod tests {
    use cf_show::model::{CutGrid, FxGates};

    use super::*;

    fn prepared(world: &mut BoxWorld) -> ChunkPlan {
        let mut ecs = EcsWorld::new();
        ecs.init_resource::<Assets<Mesh>>();
        ecs.init_resource::<Assets<StandardMaterial>>();
        let mut registry = PieceRegistry::default();
        world.setup(&mut ecs, &mut registry).expect("assets présents");
        ChunkPlan::new(&registry)
    }

    fn gates(ground_traffic: f32) -> ShowGates {
        ShowGates {
            max_floor: 6,
            flight_rate: 1.0,
            flight_on_drop_only: false,
            tricks: true,
            stunt_tiers: Vec::new(),
            stunt_kinds: Vec::new(),
            stunt_gap_scale: 1.0,
            shot_tiers: Vec::new(),
            shot_block: Vec::new(),
            cut_grid: CutGrid {
                calm: 8,
                groove: 4,
                peak: 2,
            },
            salvo: true,
            events: Vec::new(),
            event_rate: 1.0,
            meteor_rate: 14.0,
            meteor_shower: true,
            cruise: 1.0,
            ground_traffic,
            fx: FxGates {
                rgb_split: 0.0,
                flash: 0.0,
                city_pulse: 0.0,
            },
            ascension: true,
        }
    }

    #[test]
    fn meme_graine_meme_troncon() {
        let mut world = BoxWorld::new(42);
        let mut a = prepared(&mut world);
        let mut b = a.clone();
        let layout = world.layout();
        world.plan_chunk(&layout.chunk(15_384), &mut a);
        world.plan_chunk(&layout.chunk(15_384), &mut b);
        assert_eq!(a, b);
        b.clear();
        world.plan_chunk(&layout.chunk(15_385), &mut b);
        assert_ne!(a, b);
        assert_eq!(a.refused(), 0);
    }

    #[test]
    fn les_pieces_restent_dans_leur_troncon() {
        let mut world = BoxWorld::new(3).with_rows_per_chunk(13);
        let mut plan = prepared(&mut world);
        let layout = world.layout();
        for index in [-40, 0, 7, 15_000] {
            plan.clear();
            world.plan_chunk(&layout.chunk(index), &mut plan);
            assert_eq!(plan.refused(), 0);
            for piece in plan.pieces(world.boxes) {
                let z = piece.transform.translation.z;
                assert!((-CHUNK_LENGTH_M - ROW_JITTER_M..=ROW_JITTER_M).contains(&z), "z = {z}");
                assert!(piece.transform.translation.x.abs() > 11.0, "immeuble sur la chaussée");
            }
        }
    }

    #[test]
    fn la_densite_suit_la_porte_ground_traffic() {
        let mut world = BoxWorld::new(9);
        let mut plan = prepared(&mut world);
        assert_eq!(world.apply_gates(&gates(0.0)), Replan::Keep);
        world.plan_chunk(&world.layout().chunk(3), &mut plan);
        assert_eq!(plan.pieces(world.cars).len(), 0);
        world.apply_gates(&gates(0.5));
        assert_eq!(world.cars_per_chunk(), 2);
        plan.clear();
        world.plan_chunk(&world.layout().chunk(3), &mut plan);
        assert_eq!(plan.pieces(world.cars).len(), 2);
    }

    #[test]
    fn environ_un_troncon_sur_douze_porte_une_tour_repere() {
        let layout = BoxWorld::new(1).layout();
        let towers = (0..10_000)
            .filter(|&i| BoxWorld::landmark(&layout.chunk(i)).is_some())
            .count();
        assert!((600..1_000).contains(&towers), "{towers} tours sur 10 000 tronçons");
    }
}
