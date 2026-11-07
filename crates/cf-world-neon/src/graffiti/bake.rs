//! La cuisson des graffitis à l'ouverture, comme celle des enseignes ([`crate::signs::bake`]) : lecture des fichiers,
//! des polices et dessin des deux atlas dans une tâche de fond (la fenêtre reste vivante), inscrite au carnet de
//! chargement (`LoadTracker`) : le show ne s'ouvre qu'une fois la cuisson finie (règle 8 de `CLAUDE.md`). Aucune texture
//! n'est dessinée ensuite.
//!
//! La bibliothèque des mots (seulement ceux qui ont été cuits) entre dans la ville au même moment que l'atlas : un tag
//! de façade ou un projecteur ne désigne jamais une cellule absente.

use std::path::PathBuf;
use std::time::Instant;

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler};
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};
use bevy::tasks::{AsyncComputeTaskPool, Task, TaskPool, futures_lite::future};
use cf_render::warmup::StepId;
use cf_render::{CfSet, ChunkStream, LoadTracker, WorldSet};

use super::atlas::{AtlasCells, BakedGraffiti, CellRect, HoloCell, LayerFormat, Layers, bake_assets};
use crate::NeonWorld;

/// Les atlas des graffitis, sur le GPU : deux textures en tableau de couches et l'index des cellules.
#[derive(Resource, Debug)]
pub struct GraffitiAtlas {
    /// Atlas mural : RGBA sRVB, alpha prémultiplié, tableau de couches.
    pub wall: Handle<Image>,
    /// Atlas holographique : un canal (masque net et halo flou de chaque mot), tableau de couches.
    pub holo: Handle<Image>,
    /// Taille de l'atlas mural : largeur, hauteur d'une couche, nombre de couches.
    pub wall_size: UVec3,
    /// Taille de l'atlas holographique.
    pub holo_size: UVec3,
    cells: AtlasCells,
    /// Durée de la cuisson (fichiers compris), en secondes.
    pub bake_s: f32,
    /// Mémoire des deux atlas, en octets.
    pub bytes: usize,
    /// Mots sautés, avec la raison.
    pub skipped: Vec<String>,
}

impl GraffitiAtlas {
    /// La cellule du motif mural `index` (celui du tag : `1..=31`) ; `None` pour 0 ou un motif inconnu.
    #[must_use]
    pub fn wall_cell(&self, index: u8) -> Option<CellRect> {
        self.cells.wall_cell(index)
    }

    /// Les cellules du mot holographique `word` (rang de [`Projector::word`](crate::scenery::graffiti_plan::Projector)).
    #[must_use]
    pub fn holo_cell(&self, word: u16) -> Option<HoloCell> {
        self.cells.holo_cell(word)
    }
}

/// La cuisson en cours.
#[derive(Resource)]
pub struct GraffitiBake {
    task: Task<(BakedGraffiti, f32)>,
    step: Option<StepId>,
}

impl std::fmt::Debug for GraffitiBake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraffitiBake").finish_non_exhaustive()
    }
}

/// Dossier `assets/` d'où lire mots, placements et polices.
#[derive(Resource, Debug, Clone)]
struct GraffitiAssets(PathBuf);

/// Installe la cuisson des graffitis. Ajouté par [`NeonWorldPlugin`](crate::NeonWorldPlugin).
///
/// Les fichiers sont lus dans `assets_dir` par un chemin relatif au dossier de travail, comme le modèle de la voiture
/// ([`CarPlugin`](crate::CarPlugin)). **Sans rendu (tests)** le plugin ne fait rien, comme les enseignes ;
/// [`GraffitiPlugin::always`] force la cuisson.
#[derive(Debug, Clone)]
pub struct GraffitiPlugin {
    /// Cuire aussi sans rendu (tests d'intégration).
    pub force: bool,
    /// Dossier `assets/` du dépôt.
    pub assets_dir: PathBuf,
}

impl Default for GraffitiPlugin {
    fn default() -> Self {
        Self {
            force: false,
            assets_dir: "assets".into(),
        }
    }
}

impl GraffitiPlugin {
    /// Cuisson même sans moteur de rendu, depuis `assets_dir`.
    #[must_use]
    pub fn always(assets_dir: impl Into<PathBuf>) -> Self {
        Self {
            force: true,
            assets_dir: assets_dir.into(),
        }
    }
}

impl Plugin for GraffitiPlugin {
    fn build(&self, app: &mut App) {
        if !self.force && !app.is_plugin_added::<bevy::pbr::PbrPlugin>() {
            return;
        }
        app.insert_resource(GraffitiAssets(self.assets_dir.clone()))
            .add_systems(Startup, start_bake)
            .add_systems(
                Update,
                poll_bake
                    .run_if(resource_exists::<GraffitiBake>)
                    .in_set(CfSet::Simulate)
                    .before(WorldSet::Chunks),
            );
    }
}

/// `Startup` : lance la cuisson et l'inscrit au carnet.
fn start_bake(mut commands: Commands, assets: Res<GraffitiAssets>, tracker: Option<ResMut<LoadTracker>>) {
    let dir = assets.0.clone();
    let task = AsyncComputeTaskPool::get_or_init(TaskPool::default).spawn(async move {
        let start = Instant::now();
        let baked = bake_assets(&dir);
        (baked, start.elapsed().as_secs_f32())
    });
    let step = tracker.map(|mut tracker| tracker.register("Graffitis (textures)"));
    commands.insert_resource(GraffitiBake { task, step });
}

/// `Update` : quand la cuisson est finie, envoie les atlas au GPU, donne la bibliothèque à la ville (et re-tire les
/// tronçons vivants si elle change, comme un changement de qualité) et clôt l'étape du carnet.
fn poll_bake(
    mut commands: Commands,
    mut bake: ResMut<GraffitiBake>,
    mut images: ResMut<Assets<Image>>,
    tracker: Option<ResMut<LoadTracker>>,
    world: Option<ResMut<NeonWorld>>,
    stream: Option<ResMut<ChunkStream<NeonWorld>>>,
) {
    let Some((baked, bake_s)) = future::block_on(future::poll_once(&mut bake.task)) else {
        return;
    };
    let step = bake.step;
    commands.remove_resource::<GraffitiBake>();
    let bytes = baked.bytes();
    let BakedGraffiti {
        library,
        wall,
        holo,
        cells,
        skipped,
        warnings,
    } = baked;
    // Un mot sauté ou une entrée ignorée n'est pas une panne : la scène reste complète, on prévient seulement.
    for why in skipped.iter().chain(&warnings) {
        tracing::warn!("graffitis : {why}");
    }
    tracing::info!(
        wall = library.wall_words().len(),
        holo = library.holo_words().len(),
        manual = library.manual().len(),
        skipped = skipped.len(),
        kb = bytes / 1000,
        seconds = bake_s,
        "graffitis : atlas cuits"
    );
    let wall_size = UVec3::new(wall.w, wall.h, wall.count);
    let holo_size = UVec3::new(holo.w, holo.h, holo.count);
    commands.insert_resource(GraffitiAtlas {
        wall: images.add(to_image(wall)),
        holo: images.add(to_image(holo)),
        wall_size,
        holo_size,
        cells,
        bake_s,
        bytes,
        skipped,
    });
    if let Some(mut world) = world
        && world.set_graffiti(library)
        && let Some(mut stream) = stream
    {
        stream.replan_all();
    }
    if let (Some(step), Some(mut tracker)) = (step, tracker) {
        tracker.finish(step);
    }
}

/// Une texture en tableau de couches (même avec une seule couche : le shader lit toujours un `texture_2d_array`).
fn to_image(layers: Layers) -> Image {
    let format = match layers.format {
        LayerFormat::Rgba => TextureFormat::Rgba8UnormSrgb,
        LayerFormat::R8 => TextureFormat::R8Unorm,
    };
    // Rien à relire côté processeur : les atlas ne vivent que sur le GPU.
    let mut image = Image::new(
        Extent3d {
            width: layers.w,
            height: layers.h,
            depth_or_array_layers: layers.count,
        },
        TextureDimension::D2,
        layers.data,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2Array),
        ..Default::default()
    });
    image.sampler = ImageSampler::linear();
    image
}
