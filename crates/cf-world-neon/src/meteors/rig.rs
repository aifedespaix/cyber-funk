//! Les entités des astres, créées une fois au démarrage (pool : rien n'est créé ni détruit en régime établi).
//!
//! Par emplacement d'astre : le corps, la queue, le halo. Par anneau de brume : un plan couché sur la nappe. Par
//! explosion : l'éclair, l'onde de choc et la gerbe de débris. En tout [`POOL`]` × 4 + `[`BLASTS`]` × 3` entités, toutes
//! racines (une [`AbsolutePosition`] chacune), toutes cachées au repos.
//!
//! # Culling (règle 6 de `CLAUDE.md`)
//!
//! Aucune pièce ne porte `NoFrustumCulling`. Les plans (queue, halo, anneau, éclair, onde) ont l'`Aabb` exact du
//! rectangle unité que Bevy calcule : leur `Transform` (échelle, rotation) fait le reste. Le corps porte un `Aabb`
//! explicite de demi-côté 1,25 : le vertex shader bosselle la sphère jusqu'à 17 % au-delà de son rayon, l'`Aabb` calculé
//! du maillage (1,0) serait trop petit. La gerbe de débris a un `Aabb` **recalculé à chaque image** d'après ses débris
//! vivants (voir `sync`), marge d'un demi-côté de carré comprise.

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use cf_render::{AbsolutePosition, WorldLayer};

use super::blast::{BLASTS, DEBRIS};
use super::field::POOL;
use super::material::{MeteorDebrisMaterial, MeteorFxMaterial, MeteorSurfaceMaterial};

/// Demi-côté de l'`Aabb` du corps : 1,17 (bosselage) arrondi à 1,25.
pub(super) const BODY_HALF_EXTENT: f32 = 1.25;

/// Tout ce qui fait partie des astres : à interroger pour le compter ou le cacher en bloc.
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(WorldLayer, AbsolutePosition)]
pub struct MeteorPart;

/// Une pièce du pool et son indice (emplacement d'astre, anneau ou explosion selon la variante).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(MeteorPart)]
pub enum Piece {
    /// Le corps de l'astre.
    Body(usize),
    /// Sa queue.
    Trail(usize),
    /// Son halo de chaleur.
    Glow(usize),
    /// Un anneau de brume.
    Splash(usize),
    /// L'éclair d'une explosion.
    Flash(usize),
    /// L'onde de choc d'une explosion.
    Shock(usize),
    /// La gerbe de débris d'une explosion.
    Debris(usize),
}

/// Les poignées des matériaux et des maillages dynamiques, pour les réécrire sans rien créer.
#[derive(Resource, Debug)]
pub struct MeteorRig {
    pub(super) body: [Handle<MeteorSurfaceMaterial>; POOL],
    pub(super) trail: [Handle<MeteorFxMaterial>; POOL],
    pub(super) glow: [Handle<MeteorFxMaterial>; POOL],
    pub(super) splash: [Handle<MeteorFxMaterial>; POOL],
    pub(super) flash: [Handle<MeteorFxMaterial>; BLASTS],
    pub(super) shock: [Handle<MeteorFxMaterial>; BLASTS],
    pub(super) debris_mesh: [Handle<Mesh>; BLASTS],
}

/// Le maillage d'une gerbe : [`DEBRIS`] carrés dont les quatre sommets sont au centre du débris (le vertex shader les
/// écarte). Positions et couleurs sont réécrites à chaque image tant que la gerbe vit.
#[must_use]
pub fn debris_mesh() -> Mesh {
    let corners = [[0.0_f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mut uv = Vec::with_capacity(DEBRIS * 4);
    let mut indices = Vec::with_capacity(DEBRIS * 6);
    for i in 0..DEBRIS {
        uv.extend(corners);
        let base = (i * 4) as u32;
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; DEBRIS * 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0_f32; 4]; DEBRIS * 4])
        .with_inserted_indices(Indices::U32(indices))
}

/// Les matériaux, rangés par type.
#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct MeteorMaterials<'w> {
    pub surface: ResMut<'w, Assets<MeteorSurfaceMaterial>>,
    pub fx: ResMut<'w, Assets<MeteorFxMaterial>>,
    pub debris: ResMut<'w, Assets<MeteorDebrisMaterial>>,
}

/// Crée le pool d'entités, une fois, au démarrage. Tout est caché ; `sync` montre ce qui vit.
pub(super) fn spawn_meteors(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: MeteorMaterials,
) {
    let sphere = meshes.add(Sphere::new(1.0).mesh().uv(28, 18));
    let quad = meshes.add(Rectangle::new(1.0, 1.0).mesh().build());
    let hidden = Visibility::Hidden;

    let body = std::array::from_fn::<_, POOL, _>(|i| {
        let handle = materials.surface.add(MeteorSurfaceMaterial::default());
        commands.spawn((
            Name::new("astre · corps"),
            Piece::Body(i),
            Mesh3d(sphere.clone()),
            MeshMaterial3d(handle.clone()),
            Aabb::from_min_max(Vec3::splat(-BODY_HALF_EXTENT), Vec3::splat(BODY_HALF_EXTENT)),
            hidden,
        ));
        handle
    });
    let mut fx_quad = |name: &'static str, piece: Piece| {
        let handle = materials.fx.add(MeteorFxMaterial::default());
        commands.spawn((
            Name::new(name),
            piece,
            Mesh3d(quad.clone()),
            MeshMaterial3d(handle.clone()),
            hidden,
        ));
        handle
    };
    let trail = std::array::from_fn::<_, POOL, _>(|i| fx_quad("astre · queue", Piece::Trail(i)));
    let glow = std::array::from_fn::<_, POOL, _>(|i| fx_quad("astre · halo", Piece::Glow(i)));
    let splash = std::array::from_fn::<_, POOL, _>(|i| fx_quad("astre · anneau de brume", Piece::Splash(i)));
    let flash = std::array::from_fn::<_, BLASTS, _>(|i| fx_quad("explosion · éclair", Piece::Flash(i)));
    let shock = std::array::from_fn::<_, BLASTS, _>(|i| fx_quad("explosion · onde de choc", Piece::Shock(i)));

    let debris_material = materials.debris.add(MeteorDebrisMaterial::default());
    let debris_mesh = std::array::from_fn::<_, BLASTS, _>(|i| {
        let handle = meshes.add(debris_mesh());
        commands.spawn((
            Name::new("explosion · débris"),
            Piece::Debris(i),
            Mesh3d(handle.clone()),
            MeshMaterial3d(debris_material.clone()),
            // Recalculé à chaque image par `sync` ; cette boîte de départ n'est jamais vue (la gerbe est cachée).
            Aabb::from_min_max(Vec3::splat(-2.0), Vec3::splat(2.0)),
            hidden,
        ));
        handle
    });

    commands.insert_resource(MeteorRig {
        body,
        trail,
        glow,
        splash,
        flash,
        shock,
        debris_mesh,
    });
}
