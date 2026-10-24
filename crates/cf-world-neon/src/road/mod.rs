//! La route infinie de Neon City (`road.ts` de Sillage).
//!
//! « Un seul plan qui suit la voiture. Les motifs (bords, tirets, joints) sont calculés en coordonnées monde, donc ils
//! défilent sans recyclage. » Ici aussi : une seule entité, un plan de 18 m × 1 200 m, placé 300 m devant le point
//! suivi, et un shader ([`material`]) qui dessine tout. Ni tronçon, ni pool : rien à recycler.
//!
//! - **Positions** : le plan est une racine du monde portant une [`AbsolutePosition`] (`f64`) ; l'origine flottante de
//!   `cf-render` en fait la translation `f32` de son `Transform`. Le shader lit la position monde rendue, relative à
//!   l'origine : les motifs restent précis à 400 km et continus au recentrage (le pas divise toutes leurs périodes).
//! - **Culling** : le mesh est statique et centré sur son origine, son `Aabb` automatique est exact. Pas de
//!   `NoFrustumCulling`.
//! - **Musique** : pouls et basses viennent de `AudioFeatures`, la teinte d'ambiance de son intensité ; le reste (lueur,
//!   freinage, météo) de [`RoadLook`].
//! - **Coût** : une seule entité, quatre sommets, aucun spawn en régime établi. Le matériau est réécrit à chaque image
//!   (temps, pouls) ; Bevy le re-prépare (un petit buffer d'uniformes), c'est tout.

mod look;
mod material;
mod systems;

use bevy::prelude::*;
use bevy::shader::Shader;
use cf_render::{CfSet, WarmupAppExt, WorldLayer, WorldSet};

pub use look::{RoadLook, RoadPalette};
pub use material::{RoadMaterial, RoadParams};
pub use systems::{FrameInputs, build_params};

/// Longueur du plan de la route, en m (`LENGTH` de `road.ts`).
pub const LENGTH_M: f32 = 1200.0;
/// Avance, en m, du centre du plan sur le point suivi : le plan couvre 300 m derrière et 900 m devant
/// (`LENGTH * 0.25` de `Road.update`). La brume a tout avalé bien avant.
pub const AHEAD_M: f64 = LENGTH_M as f64 * 0.25;

/// Marqueur du plan de la route.
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(WorldLayer)]
pub struct Road;

/// Installe la route : matériau et shader, le plan, et les deux systèmes qui le suivent et le nourrissent.
///
/// À ajouter après les plugins de rendu de Bevy : le shader est embarqué dans `Assets<Shader>` à la construction. Sans
/// rendu (tests), seul le matériau est déclaré et la logique tourne.
#[derive(Debug, Default, Clone, Copy)]
pub struct RoadPlugin;

impl Plugin for RoadPlugin {
    fn build(&self, app: &mut App) {
        if app.is_plugin_added::<bevy::pbr::PbrPlugin>() {
            app.add_plugins(MaterialPlugin::<RoadMaterial>::default())
                .warm_material::<RoadMaterial>("route");
            if let Some(mut shaders) = app.world_mut().get_resource_mut::<Assets<Shader>>() {
                material::embed_shader(&mut shaders);
            }
        } else {
            app.init_asset::<RoadMaterial>();
        }
        app.init_resource::<RoadLook>()
            .add_systems(Startup, spawn_road)
            .add_systems(
                Update,
                systems::follow_focus.in_set(CfSet::Simulate).after(WorldSet::Chunks),
            )
            .add_systems(Update, systems::update_material.in_set(CfSet::Present));
    }
}

/// Crée le plan de la route, une fois, au démarrage.
fn spawn_road(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<RoadMaterial>>) {
    let plane = Plane3d::new(Vec3::Y, Vec2::new(crate::route::ROAD_HALF_WIDTH_M, LENGTH_M / 2.0));
    commands.spawn((
        Name::new("route"),
        Road,
        Mesh3d(meshes.add(plane.mesh().build())),
        MeshMaterial3d(materials.add(RoadMaterial::default())),
        cf_render::AbsolutePosition::default(),
    ));
}
