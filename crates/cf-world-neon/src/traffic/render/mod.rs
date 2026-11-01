//! Le rendu du trafic : des entités préallouées au démarrage, jamais de `spawn` ni de `despawn` ensuite.
//!
//! - **véhicules** : une entité par emplacement de chaque modèle (`CAPACITY`), qui partagent le maillage de leur modèle
//!   et un seul matériau (Bevy les regroupe en un lot par modèle, l'équivalent des `InstancedMesh` de Sillage). La
//!   teinte voyage dans le `MeshTag` ;
//! - **traînées** : une entité par véhicule du pool, un maillage et un matériau communs ;
//! - **points lumineux** et **murs de lumière** : une entité chacun, dont le maillage est réécrit sur place ;
//! - **panneaux des structures** (route croisée, tour, portiques, danseur, aurore) : deux entités, une par couche (plein et
//!   lumière), dont le maillage est réécrit sur place (voir `panels`).
//!
//! Toutes portent `WorldLayer`. Les `Aabb` sont ceux des maillages (véhicules, traînées) ou un volume centré sur la
//! voiture (points, murs : leurs sommets sont relatifs à l'origine de l'image) : jamais de `NoFrustumCulling`.

mod materials;
mod meshes;
mod panels;
mod sync;

use std::path::PathBuf;

use bevy::mesh::MeshTag;
use bevy::prelude::*;
use cf_render::{AbsolutePosition, EntityPool, LodBands, SimulationLod, WorldLayer, follow_camera_bounds};

pub(crate) use materials::embed_shaders;
pub use materials::{GlintMaterial, TrailMaterial, VehicleMaterial, VehicleParams, WallMaterial};
pub(crate) use panels::embed_shader as embed_panel_shader;
pub use panels::{HoloMaterial, PanelMaterial, PanelParams, TrafficPanelsLight, TrafficPanelsSolid};
pub(crate) use panels::{sync_panels, update_panel_materials};
pub(crate) use sync::{sync_dynamic, sync_fleet, update_materials};

use super::director::TrafficDirector;
use super::fleet::CraftId;
use super::model;
use super::variant::{BIKE, CAPACITY, POOL, VARIANT_COUNT};
use crate::car::CarModelAsset;
use crate::rider::RiderPaintMaterial;
use crate::rider::moto::{self, BODY_MATERIALS, MOTO_COUNT};
use crate::rider::spawn::{ModelAssets, Tint, spawn_model};

/// Distances de la cadence de simulation d'un véhicule (m de la caméra). Les véhicules vont vite (60 à 118 m/s) et la
/// ville s'arrête à la brume (380 m) : pleine cadence tant qu'ils sont à portée d'une coupe de caméra, demi-cadence puis
/// quart de cadence en vue au loin, un huitième hors champ. Jamais gelés : un véhicule gelé resterait en place, alors
/// que sa trajectoire continue (voir `Fleet`).
const LOD_BANDS: LodBands = LodBands {
    near_m: 100.0,
    far_m: 250.0,
    freeze_m: f32::INFINITY,
};
/// Demi-arête (m) du volume des points lumineux et des murs : la fenêtre du trafic autour de la voiture (720 m devant).
const DYNAMIC_REACH_M: f32 = 760.0;

/// D'où viennent les modèles (`assets/models/crafts`), lus une fois au démarrage.
#[derive(Resource, Debug, Clone)]
pub struct TrafficSource {
    /// Dossier des fichiers `craft_<nom>.glb`.
    pub models_dir: PathBuf,
}

/// Marque l'entité d'un emplacement de véhicule.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TrafficBody;

/// Marque l'entité de la traînée d'un véhicule.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TrafficTrail;

/// Marque l'entité du maillage des points lumineux.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TrafficGlints;

/// Marque l'entité du maillage des murs de lumière.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TrafficWalls;

/// Ce que le dernier envoi au rendu a montré d'un véhicule.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Shown {
    pub(crate) generation: u32,
    pub(crate) body: Option<Entity>,
    pub(crate) trail_visible: bool,
}

/// Les entités préallouées et ce qu'elles montrent.
#[derive(Resource, Debug)]
pub struct TrafficEntities {
    /// Corps : par modèle, par emplacement.
    bodies: Vec<EntityPool>,
    /// Traînées : par véhicule du pool.
    trails: Vec<Entity>,
    /// Matériaux teintés de chaque moto texturée, par emplacement de la variante moto (vide : moto procédurale).
    pub(crate) moto_tints: Vec<Vec<Handle<RiderPaintMaterial>>>,
    pub(crate) shown: Vec<Shown>,
    pub(crate) vehicle_material: Handle<VehicleMaterial>,
    pub(crate) glint_mesh: Handle<Mesh>,
    pub(crate) wall_mesh: Handle<Mesh>,
    pub(crate) glints: Entity,
    pub(crate) walls: Entity,
    /// Points écrits à l'image précédente (pour éteindre ceux qui disparaissent).
    pub(crate) last_glints: usize,
    /// Les deux couches de panneaux : entités, maillages, matériaux, et panneaux écrits à l'image précédente.
    pub(crate) panel_solid: Entity,
    pub(crate) panel_light: Entity,
    pub(crate) panel_meshes: [Handle<Mesh>; 2],
    pub(crate) panel_solid_material: Handle<PanelMaterial>,
    pub(crate) panel_light_material: Handle<HoloMaterial>,
    pub(crate) last_panels: [usize; 2],
}

impl TrafficEntities {
    /// Le corps du véhicule de ce modèle et de cet emplacement.
    #[must_use]
    pub fn body(&self, variant: u8, slot: u16) -> Option<Entity> {
        self.bodies
            .get(usize::from(variant))?
            .entities()
            .get(usize::from(slot))
            .copied()
    }

    /// La traînée du véhicule `id`.
    #[must_use]
    pub fn trail(&self, id: CraftId) -> Option<Entity> {
        self.trails.get(usize::from(id)).copied()
    }

    /// Nombre d'entités préallouées par le trafic.
    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.bodies.iter().map(EntityPool::capacity).sum::<usize>() + self.trails.len() + 4
    }
}

/// Le parc des motos texturées : les entités du pool de la variante moto, chacune portant son modèle (les trois en
/// alternance) et sa moto posée à la place de la procédurale. Renvoie le pool et, par emplacement, les matériaux à teinter.
fn spawn_moto_pool(
    commands: &mut Commands,
    variant: usize,
    motos: &[CarModelAsset],
    refs: &mut ModelAssets<'_>,
) -> (EntityPool, Vec<Vec<Handle<RiderPaintMaterial>>>) {
    let bundle = (
        Name::new(format!("trafic : moto {variant}")),
        TrafficBody,
        WorldLayer,
        Transform::default(),
        AbsolutePosition::default(),
        MeshTag::new(0),
        SimulationLod::new(LOD_BANDS),
    );
    let pool = EntityPool::spawn(commands, CAPACITY[variant], bundle);
    let mut tints = Vec::with_capacity(pool.capacity());
    for (slot, &entity) in pool.entities().iter().enumerate() {
        let which = slot % MOTO_COUNT;
        let tint = Tint {
            color: LinearRgba::rgb(0.6, 0.6, 0.6),
            body_materials: BODY_MATERIALS[which],
        };
        let (root, spawned) = spawn_model(commands, entity, motos[which].clone(), refs, Some(tint));
        commands.entity(root).insert(moto::fit(which));
        tints.push(spawned.tinted);
    }
    (pool, tints)
}

/// `Startup` : construit les maillages et les matériaux, crée toutes les entités du trafic (cachées).
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources d'assets.
pub(crate) fn setup_traffic(
    mut commands: Commands,
    source: Res<TrafficSource>,
    mut director: ResMut<TrafficDirector>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut vehicles: ResMut<Assets<VehicleMaterial>>,
    mut trails: ResMut<Assets<TrailMaterial>>,
    mut glints: ResMut<Assets<GlintMaterial>>,
    mut walls: ResMut<Assets<WallMaterial>>,
    mut panel_solid: ResMut<Assets<PanelMaterial>>,
    mut panel_light: ResMut<Assets<HoloMaterial>>,
    images: Option<ResMut<Assets<Image>>>,
    standard: Option<ResMut<Assets<StandardMaterial>>>,
    mut tinted: ResMut<Assets<RiderPaintMaterial>>,
) {
    let (mut images, mut standard) = (images, standard);
    let models = model::load_models(&source.models_dir);
    // Les motos texturées (X-10) : lues avec les autres modèles, à l'ouverture. Sans elles, la moto procédurale reste.
    let motos = if images.is_some() && standard.is_some() {
        source
            .models_dir
            .parent()
            .and_then(|models| moto::load_motos(&models.join("motos")))
    } else {
        None
    };
    let mut moto_tints = Vec::new();
    let vehicle_material = vehicles.add(VehicleMaterial::default());

    let mut bodies = Vec::with_capacity(VARIANT_COUNT);
    for (variant, model) in models.into_iter().enumerate() {
        director.fleet.set_dims(variant as u8, model.dims());
        if variant == usize::from(BIKE)
            && let Some(motos) = &motos
            && let (Some(images), Some(standard)) = (images.as_mut(), standard.as_mut())
        {
            let mut refs = ModelAssets {
                meshes: &mut meshes,
                images,
                standard,
                tinted: &mut tinted,
            };
            let (pool, tints) = spawn_moto_pool(&mut commands, variant, motos, &mut refs);
            moto_tints = tints;
            bodies.push(pool);
            continue;
        }
        let mesh = meshes.add(model.mesh);
        let bundle = (
            Name::new(format!("trafic : véhicule {variant}")),
            TrafficBody,
            WorldLayer,
            Mesh3d(mesh),
            MeshMaterial3d(vehicle_material.clone()),
            AbsolutePosition::default(),
            MeshTag::new(0),
            SimulationLod::new(LOD_BANDS),
        );
        bodies.push(EntityPool::spawn(&mut commands, CAPACITY[variant], bundle));
    }

    let trail_mesh = meshes.add(meshes::trail_mesh());
    let trail_material = trails.add(TrailMaterial::default());
    let trail_entities = (0..POOL)
        .map(|i| {
            commands
                .spawn((
                    Name::new(format!("trafic : traînée {i}")),
                    TrafficTrail,
                    WorldLayer,
                    Mesh3d(trail_mesh.clone()),
                    MeshMaterial3d(trail_material.clone()),
                    AbsolutePosition::default(),
                    MeshTag::new(0),
                    cf_render::streaming::Pooled,
                    Visibility::Hidden,
                ))
                .id()
        })
        .collect();

    let glint_mesh = meshes.add(meshes::glint_mesh());
    let glint_entity = spawn_dynamic(
        &mut commands,
        "trafic : points lumineux",
        TrafficGlints,
        glint_mesh.clone(),
        glints.add(GlintMaterial::default()),
    );
    let wall_mesh = meshes.add(meshes::wall_mesh());
    let wall_entity = spawn_dynamic(
        &mut commands,
        "trafic : murs de lumière",
        TrafficWalls,
        wall_mesh.clone(),
        walls.add(WallMaterial::default()),
    );

    let panel_solid_mesh = meshes.add(panels::panel_mesh());
    let panel_light_mesh = meshes.add(panels::panel_mesh());
    let panel_solid_material = panel_solid.add(PanelMaterial::default());
    let panel_light_material = panel_light.add(HoloMaterial::default());
    let panel_solid_entity = spawn_dynamic(
        &mut commands,
        "trafic : panneaux pleins",
        TrafficPanelsSolid,
        panel_solid_mesh.clone(),
        panel_solid_material.clone(),
    );
    let panel_light_entity = spawn_dynamic(
        &mut commands,
        "trafic : panneaux de lumière",
        TrafficPanelsLight,
        panel_light_mesh.clone(),
        panel_light_material.clone(),
    );

    commands.insert_resource(TrafficEntities {
        bodies,
        trails: trail_entities,
        moto_tints,
        shown: vec![Shown::default(); POOL],
        vehicle_material,
        glint_mesh,
        wall_mesh,
        glints: glint_entity,
        walls: wall_entity,
        last_glints: 0,
        panel_solid: panel_solid_entity,
        panel_light: panel_light_entity,
        panel_meshes: [panel_solid_mesh, panel_light_mesh],
        panel_solid_material,
        panel_light_material,
        last_panels: [0; 2],
    });
}

/// Une entité à maillage dynamique : ses sommets sont relatifs à l'origine de l'image (la voiture), l'entité suit cette
/// origine, et son volume est un cube centré dessus (`follow_camera_bounds`).
fn spawn_dynamic<M: Material, C: Component>(
    commands: &mut Commands,
    name: &'static str,
    marker: C,
    mesh: Handle<Mesh>,
    material: Handle<M>,
) -> Entity {
    let (aabb, no_auto) = follow_camera_bounds(DYNAMIC_REACH_M);
    commands
        .spawn((
            Name::new(name),
            marker,
            WorldLayer,
            Mesh3d(mesh),
            MeshMaterial3d(material),
            AbsolutePosition::default(),
            aabb,
            no_auto,
            cf_render::streaming::Pooled,
            Visibility::Hidden,
        ))
        .id()
}
