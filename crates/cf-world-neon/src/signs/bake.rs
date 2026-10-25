//! La cuisson de l'atlas au chargement : dans une tâche de fond (la fenêtre reste vivante), inscrite au carnet de
//! chargement (`LoadTracker`) pour que le show ne s'ouvre qu'une fois toutes les textures prêtes (règle 8 de `CLAUDE.md`).
//! Aucune texture n'est dessinée ensuite : pendant le show, il n'y a plus que des lectures.

use std::collections::HashMap;
use std::time::Instant;

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler};
use bevy::math::Vec4;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::{AsyncComputeTaskPool, Task, TaskPool, futures_lite::future};
use cf_render::warmup::StepId;
use cf_render::{LoadTracker, ShowSheet};

use super::atlas::{BakedAtlas, CellInfo, CellKey, PageFormat, StripInfo, StripKey};
use super::show_signs::SignShow;

/// L'atlas des enseignes, sur le GPU : l'index des cellules et des bandeaux, et les pages.
#[derive(Resource, Debug)]
pub struct SignAtlas {
    /// Les cellules.
    pub cells: HashMap<CellKey, CellInfo>,
    /// Les bandeaux.
    pub strips: HashMap<StripKey, StripInfo>,
    pages: Vec<(Handle<Image>, UVec2)>,
    /// Durée de la cuisson, en secondes.
    pub bake_s: f32,
    /// Mémoire des pages, en octets.
    pub bytes: usize,
}

impl SignAtlas {
    /// Taille (largeur, hauteur) de la page `index`, dans `x` et `y`.
    #[must_use]
    pub fn page_size(&self, index: usize) -> Vec4 {
        self.pages
            .get(index)
            .map_or(Vec4::ONE, |(_, size)| Vec4::new(size.x as f32, size.y as f32, 0.0, 0.0))
    }

    /// La texture de la page `index`.
    #[must_use]
    pub fn page_handle(&self, index: usize) -> Option<Handle<Image>> {
        self.pages.get(index).map(|(handle, _)| handle.clone())
    }
}

/// La cuisson en cours.
#[derive(Resource)]
pub struct SignBake {
    task: Task<(BakedAtlas, f32)>,
    step: Option<StepId>,
}

impl std::fmt::Debug for SignBake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignBake").finish_non_exhaustive()
    }
}

/// `Startup` : lit les noms du show, puis lance la cuisson de tout l'atlas.
pub fn start_bake(
    mut commands: Commands,
    sheet: Option<Res<ShowSheet>>,
    mut show: ResMut<SignShow>,
    tracker: Option<ResMut<LoadTracker>>,
) {
    if let Some(sheet) = sheet {
        show.slots = SignShow::names(&sheet.event_name, sheet.slots.iter().map(|s| s.dj.as_str()));
    }
    let slots = show.slots.clone();
    let threads = std::thread::available_parallelism().map_or(2, std::num::NonZero::get);
    let task = AsyncComputeTaskPool::get_or_init(TaskPool::default).spawn(async move {
        let start = Instant::now();
        let atlas = BakedAtlas::build(&slots, threads);
        (atlas, start.elapsed().as_secs_f32())
    });
    let step = tracker.map(|mut tracker| tracker.register("Enseignes (textures)"));
    commands.insert_resource(SignBake { task, step });
}

/// `Update` : quand la cuisson est finie, envoie les pages au GPU et ouvre l'atlas aux systèmes.
pub fn poll_bake(
    mut commands: Commands,
    bake: Option<ResMut<SignBake>>,
    mut images: ResMut<Assets<Image>>,
    tracker: Option<ResMut<LoadTracker>>,
) {
    let Some(mut bake) = bake else { return };
    let Some((atlas, bake_s)) = future::block_on(future::poll_once(&mut bake.task)) else {
        return;
    };
    let step = bake.step;
    commands.remove_resource::<SignBake>();
    let bytes = atlas.bytes();
    let BakedAtlas {
        pages,
        cells,
        strips,
        failed,
    } = atlas;
    let pages = pages
        .into_iter()
        .map(|page| {
            let format = match page.format {
                PageFormat::Rgba => TextureFormat::Rgba8Unorm,
                PageFormat::R8 => TextureFormat::R8Unorm,
            };
            let size = UVec2::new(page.w, page.h);
            // Rien à relire côté processeur : les pages ne vivent que sur le GPU.
            let mut image = Image::new(
                Extent3d {
                    width: page.w,
                    height: page.h,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                page.data,
                format,
                RenderAssetUsages::RENDER_WORLD,
            );
            image.sampler = ImageSampler::linear();
            (images.add(image), size)
        })
        .collect();
    tracing::info!(
        cells = cells.len(),
        strips = strips.len(),
        mb = bytes / 1_000_000,
        seconds = bake_s,
        failed,
        "enseignes : atlas cuit"
    );
    commands.insert_resource(SignAtlas {
        cells,
        strips,
        pages,
        bake_s,
        bytes,
    });
    if let (Some(step), Some(mut tracker)) = (step, tracker) {
        if failed == 0 {
            tracker.finish(step);
        } else {
            tracker.fail(step, format!("{failed} texture(s) d'enseigne non dessinée(s)"));
        }
    }
}
