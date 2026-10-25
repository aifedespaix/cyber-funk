//! La couche Bevy des immeubles-vitrines : les entités préallouées à l'ouverture et le système qui recopie la pose et les
//! paramètres de chaque pièce.
//!
//! # Pool
//!
//! Chaque immeuble a une **racine** (position absolue, origine flottante) et ses **pièces** (enfants, repère de l'immeuble).
//! Tout est créé une fois, dans `PostStartup`, caché au repos, et **jamais détruit ni recréé** : aucun `spawn` en régime
//! établi (règle 4). Un seul immeuble est visible à la fois ; les autres dorment cachés, aucun matériau n'est écrit pour eux.
//! Chaque entité porte `WorldLayer` : Bevy ne propage pas les `RenderLayers` aux enfants.
//!
//! # Textures (règle 8)
//!
//! L'atlas du mot ERROR est cuit ici, à l'ouverture, et inscrit au carnet de chargement ; la grille de l'arcade est créée
//! vide et réécrite en place par la partie qui joue (aucune allocation, aucune texture nouvelle).
//!
//! # Culling
//!
//! Chaque pièce a l'`Aabb` de son maillage, ou celle que sa description impose quand le vertex shader déplace les sommets.
//! Aucun `NoFrustumCulling`. Le corps fenêtré des immeubles est un pavé de la ville (même matériau, même plage de brume).

mod material;
mod meshes;

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::image::{Image, ImageSampler};
use bevy::mesh::MeshTag;
use bevy::pbr::MeshMaterial3d;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use cf_render::{AbsolutePosition, LoadTracker, WorldLayer};

pub use material::{LandmarkKey, LandmarkMaterial, LandmarkParams};
pub(crate) use material::{embed_shaders, warm};

use super::arcade_game::{GRID_H, GRID_W};
use super::event::LandmarkHandle;
use super::field::CityField;
use super::frame::Colors;
use super::landmark::{LandmarkKind, Placement};
use super::part::{MapId, PartPaint};
use super::stage::FrameUniform;
use crate::scenery::{BASE_DEPTH_M, SceneryAssets, Tier, fog_range};

/// Période, en s, au bout de laquelle le temps des shaders repart de zéro : la même que celle des façades.
const TIME_WRAP_S: f64 = 2400.0;

/// Les entités d'un immeuble.
#[derive(Debug)]
struct KindRig {
    root: Entity,
    parts: Vec<(Entity, Handle<LandmarkMaterial>)>,
    body: Option<Entity>,
}

/// Toutes les entités des immeubles-vitrines, créées à l'ouverture.
#[derive(Resource, Debug)]
pub struct LandmarkRig {
    kinds: Vec<KindRig>,
    atlas: Handle<Image>,
    grid: Handle<Image>,
    /// Ce que les entités montrent : l'immeuble et le numéro de pose.
    shown: Option<(LandmarkKind, u32)>,
}

impl LandmarkRig {
    /// Nombre d'entités préallouées (constant en régime établi).
    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.kinds
            .iter()
            .map(|k| 1 + k.parts.len() + usize::from(k.body.is_some()))
            .sum()
    }

    /// L'atlas du mot ERROR.
    #[must_use]
    pub fn atlas(&self) -> &Handle<Image> {
        &self.atlas
    }

    /// La racine d'un immeuble.
    #[must_use]
    pub fn root(&self, kind: LandmarkKind) -> Entity {
        self.kinds[kind.index()].root
    }

    /// Les entités des pièces d'un immeuble.
    pub fn parts(&self, kind: LandmarkKind) -> impl Iterator<Item = Entity> + '_ {
        self.kinds[kind.index()].parts.iter().map(|(entity, _)| *entity)
    }

    /// Le corps fenêtré d'un immeuble, s'il en a un.
    #[must_use]
    pub fn body(&self, kind: LandmarkKind) -> Option<Entity> {
        self.kinds[kind.index()].body
    }
}

fn texture(width: u32, height: u32, data: Vec<u8>, sampler: ImageSampler) -> Image {
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::R8Unorm,
        RenderAssetUsages::default(),
    );
    image.sampler = sampler;
    image
}

/// `PostStartup` : crée les racines, les pièces et les textures, tout caché ; cuit l'atlas du mot ERROR.
pub(crate) fn spawn_rig(
    mut commands: Commands,
    handle: Res<LandmarkHandle>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<LandmarkMaterial>>,
    mut images: ResMut<Assets<Image>>,
    scenery: Option<Res<SceneryAssets>>,
    tracker: Option<ResMut<LoadTracker>>,
    mut field: ResMut<CityField>,
) {
    let stage = handle.lock();
    // L'atlas : cuit maintenant, avant le show. Un échec (taille nulle, impossible) laisse une texture vide : le mot ne
    // s'affiche pas, mais le show ne s'arrête pas.
    let baked = super::error_atlas::bake(&mut crate::signs::paint::text::Fonts::new()).unwrap_or_else(|| {
        tracing::error!("landmarks : atlas du mot ERROR non cuit");
        vec![0; (super::error_atlas::WIDTH * super::error_atlas::ROW * 4) as usize]
    });
    let atlas = images.add(texture(
        super::error_atlas::WIDTH,
        super::error_atlas::ROW * 4,
        baked,
        ImageSampler::linear(),
    ));
    let grid = images.add(texture(
        GRID_W as u32,
        GRID_H as u32,
        vec![0; GRID_W * GRID_H],
        ImageSampler::nearest(),
    ));
    if let Some(mut tracker) = tracker {
        tracker.register_done("immeubles-vitrines : atlas du mot ERROR");
    }
    field.decal_map = Some(atlas.clone());

    let mut library = meshes::MeshLibrary::default();
    let mut kinds = Vec::with_capacity(LandmarkKind::ALL.len());
    for kind in LandmarkKind::ALL {
        let root = commands
            .spawn((
                Name::new(format!("immeuble-vitrine {}", kind.as_str())),
                WorldLayer,
                AbsolutePosition::default(),
                Transform::default(),
                Visibility::Hidden,
            ))
            .id();
        let mut parts = Vec::new();
        for spec in stage.parts(kind) {
            let map = spec.map.map(|m| match m {
                MapId::ErrorAtlas => atlas.clone(),
                MapId::ArcadeGrid => grid.clone(),
            });
            let material = materials.add(LandmarkMaterial::new(spec.look, map));
            let mut entity = commands.spawn((
                Name::new(format!("{} : {}", kind.as_str(), spec.name)),
                WorldLayer,
                Mesh3d(library.get(&spec.mesh, &mut meshes)),
                MeshMaterial3d(material.clone()),
                Transform::default(),
                Visibility::Inherited,
                ChildOf(root),
            ));
            if let Some((min, max)) = spec.bounds {
                entity.insert(Aabb::from_min_max(min, max));
            }
            parts.push((entity.id(), material));
        }
        let body = stage.landmark(kind).body().zip(scenery.as_deref()).map(|(_, scenery)| {
            commands
                .spawn((
                    Name::new(format!("immeuble-vitrine {} : corps", kind.as_str())),
                    WorldLayer,
                    Mesh3d(scenery.mesh.clone()),
                    MeshMaterial3d(scenery.material.clone()),
                    MeshTag::new(0),
                    Transform::default(),
                    Visibility::Inherited,
                    fog_range(),
                    ChildOf(root),
                ))
                .id()
        });
        kinds.push(KindRig { root, parts, body });
    }
    commands.insert_resource(LandmarkRig {
        kinds,
        atlas,
        grid,
        shown: None,
    });
}

/// Les uniformes d'une pièce : la palette, l'image, les paramètres propres au rendu.
#[must_use]
pub fn params_of(uniform: &FrameUniform, colors: &Colors, paint: &PartPaint) -> LandmarkParams {
    let mut params = LandmarkParams::default();
    for (slot, color) in params.palette.iter_mut().zip(colors.roles()) {
        *slot = color.extend(1.0);
    }
    params.mood = paint.mood.extend(1.0);
    params.frame = Vec4::new(
        (uniform.time % TIME_WRAP_S) as f32,
        uniform.pulse,
        uniform.bass,
        uniform.mid,
    );
    params.audio = Vec4::new(uniform.high, uniform.beat, uniform.intensity, uniform.phase);
    params.a = paint.a;
    params.b = paint.b;
    params.c = paint.c;
    params.d = paint.d;
    params
}

/// Le tag du corps fenêtré (voir `Part::tag` de la ville) : graine, teinte, type « corps », sans ceinture lumineuse.
#[must_use]
pub fn body_tag(seed: u32, tint: u8) -> u32 {
    seed & 0x3FF | (u32::from(tint) & 0xF) << 10 | (Tier::Body as u32) << 14
}

/// Pose le corps d'un immeuble : un pavé qui plonge sous la route comme les gratte-ciel (`body.set` de `LandmarkBase.show`).
fn place_body(transform: &mut Transform, tag: &mut MeshTag, span: f32, height: f32, depth: f32, seed: u32, tint: u8) {
    transform.translation = Vec3::new(0.0, -BASE_DEPTH_M, -depth / 2.0);
    transform.scale = Vec3::new(span, BASE_DEPTH_M + height, depth);
    tag.value = body_tag(seed, tint);
}

/// `CfSet::Present` : recopie la scène des immeubles dans leurs entités. Rien n'est créé ; seules les poses et les paramètres
/// de l'immeuble visible sont écrits.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(crate) fn apply_stage(
    handle: Res<LandmarkHandle>,
    mut rig: ResMut<LandmarkRig>,
    mut materials: ResMut<Assets<LandmarkMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut roots: Query<(&mut AbsolutePosition, &mut Transform, &mut Visibility), Without<ChildOf>>,
    mut parts: Query<(&mut Transform, &mut Visibility), With<ChildOf>>,
    mut bodies: Query<&mut MeshTag>,
) {
    let mut stage = handle.lock();
    let active = stage.active().copied();
    let seq = stage.seq();
    let rig = &mut *rig;
    let current = active.map(|a| (a.kind, seq));
    if rig.shown != current {
        // La pose a changé : on range ce qui était visible et on pose la racine de l'immeuble qui l'est maintenant.
        for (index, kind_rig) in rig.kinds.iter().enumerate() {
            let is_active = active.is_some_and(|a| a.kind.index() == index);
            let Ok((mut position, mut transform, mut visibility)) = roots.get_mut(kind_rig.root) else {
                continue;
            };
            visibility.set_if_neq(if is_active {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
            if let (true, Some(active)) = (is_active, active) {
                place_root(&mut position, &mut transform, &active.place);
                let landmark = stage.landmark(active.kind);
                if let (Some(body), Some(entity)) = (landmark.body(), kind_rig.body)
                    && let (Ok((mut body_transform, _)), Ok(mut tag)) = (parts.get_mut(entity), bodies.get_mut(entity))
                {
                    place_body(
                        &mut body_transform,
                        &mut tag,
                        landmark.span(),
                        body.height,
                        body.depth,
                        body.seed,
                        active.place.tint,
                    );
                }
            }
        }
        rig.shown = current;
    }
    let Some(active) = active else { return };
    let kind_rig = &rig.kinds[active.kind.index()];
    let uniform = *stage.uniform();
    let colors = stage.inputs.colors;
    for ((entity, material), paint) in kind_rig.parts.iter().zip(stage.paint(active.kind)) {
        if let Ok((mut transform, mut visibility)) = parts.get_mut(*entity) {
            transform.set_if_neq(paint.transform);
            visibility.set_if_neq(if paint.visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
        if let Some(mut asset) = materials.get_mut(material) {
            asset.params = params_of(&uniform, &colors, paint);
        }
    }
    // La grille de l'arcade : réécrite en place, sans allocation, quand la partie a changé.
    if active.kind == LandmarkKind::Arcade {
        let landmark = stage.landmark_mut(active.kind);
        if landmark.take_grid_dirty()
            && let Some(cells) = landmark.grid()
            && let Some(mut image) = images.get_mut(&rig.grid)
            && let Some(data) = image.data.as_mut()
            && data.len() == cells.len()
        {
            data.copy_from_slice(cells);
        }
    }
}

/// Pose la racine : position absolue, orientation, échelle du passage.
fn place_root(position: &mut AbsolutePosition, transform: &mut Transform, place: &Placement) {
    let target = place.root_position();
    if position.0 != target {
        position.0 = target;
    }
    transform.rotation = place.root_rotation();
    transform.scale = Vec3::splat(place.scale);
}

#[cfg(test)]
mod tests;
