//! Les graffitis muraux côté matériau : la table des motifs que lit le shader suit l'atlas cuit, et le shader de façade
//! est vide sans atlas. Le texte du shader est vérifié dans `facade/wall_tests.rs`.

#![allow(clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use bevy::prelude::*;
use cf_render::CorePlugin;
use cf_render::warmup::LoadTracker;
use cf_world_neon::FacadeMaterial;
use cf_world_neon::facade::{WALL_SLOTS, WallCells};
use cf_world_neon::graffiti::{GraffitiAtlas, GraffitiPlugin, GraffitiQuality};

const ASSETS: &str = "../../assets";

fn baked_cells() -> (WallCells, usize) {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        GraffitiPlugin::always(ASSETS),
    ))
    .init_asset::<Image>()
    .init_resource::<LoadTracker>();
    let start = Instant::now();
    while app.world().get_resource::<GraffitiAtlas>().is_none() {
        assert!(start.elapsed() < Duration::from_secs(60), "atlas non cuit en 60 s");
        app.update();
        std::thread::sleep(Duration::from_millis(5));
    }
    let atlas = app.world().resource::<GraffitiAtlas>();
    let walls = (1..=31u8).filter(|k| atlas.wall_cell(*k).is_some()).count();
    (WallCells::from_atlas(atlas, &GraffitiQuality::default()), walls)
}

#[test]
fn la_table_suit_l_atlas_et_laisse_vide_ce_qui_n_existe_pas() {
    let (cells, walls) = baked_cells();
    assert!(walls > 0, "la liste livrée a au moins un mot mural");
    assert!(cells.atlas.x >= 1.0 && cells.atlas.z >= 1.0);
    assert!(cells.info[0].w < 0.5, "l'entrée 0 (aucun motif) n'existe jamais");
    let existing = (1..WALL_SLOTS).filter(|k| cells.info[*k].w > 0.5).count();
    assert_eq!(existing, walls);
    for k in (1..WALL_SLOTS).filter(|k| cells.info[*k].w > 0.5) {
        let r = cells.rects[k];
        assert!(r.z > 0.0 && r.w > 0.0, "motif {k} sans surface : {r:?}");
        assert!(
            r.x + r.z <= cells.atlas.x && r.y + r.w <= cells.atlas.y,
            "motif {k} hors de la couche"
        );
    }
}

#[test]
fn sans_atlas_aucun_motif_n_existe() {
    let m = FacadeMaterial::default();
    assert!(m.wall.is_none());
    assert!(
        m.wall_cells.info.iter().all(|i| i.w < 0.5),
        "la façade d'avant n'a aucun motif"
    );
}
