//! Les projecteurs holographiques dans une vraie `App` Bevy, sans rendu : douze au plus, les plus proches gagnent, un
//! projecteur garde son emplacement quand la voiture avance, un emplacement libre est caché (jamais détruit).

#![allow(clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::warmup::LoadTracker;
use cf_render::{AbsolutePosition, CorePlugin, StreamFocus};
use cf_world_neon::graffiti::projector::{MAX_PROJECTORS, ProjectorPool, ProjectorRoot, ProjectorState};
use cf_world_neon::graffiti::{GraffitiAtlas, GraffitiPlugin};
use cf_world_neon::scenery::graffiti_plan::{Projector, ProjectorList, row_projectors};
use cf_world_neon::{ChasePlugin, NeonWorld, NeonWorldPlugin, RunnerConfig};

const FRAME: Duration = Duration::from_micros(16_667);
const ASSETS: &str = "../../assets";

fn app(start_z_m: f64, speed_mps: f64) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        GraffitiPlugin::always(ASSETS),
        NeonWorldPlugin::embedded(),
        ChasePlugin {
            config: RunnerConfig { start_z_m, speed_mps },
        },
    ))
    .init_asset::<Image>()
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>()
    .init_resource::<LoadTracker>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    let start = Instant::now();
    while app.world().get_resource::<GraffitiAtlas>().is_none() {
        assert!(start.elapsed() < Duration::from_secs(60), "atlas non cuit en 60 s");
        app.update();
        std::thread::sleep(Duration::from_millis(5));
    }
    app
}

fn active(app: &App) -> Vec<(usize, Projector)> {
    let state = app.world().resource::<ProjectorState>();
    state
        .active()
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.map(|p| (i, p)))
        .collect()
}

fn visible_roots(app: &mut App) -> usize {
    let mut q = app.world_mut().query_filtered::<&Visibility, With<ProjectorRoot>>();
    q.iter(app.world()).filter(|v| **v != Visibility::Hidden).count()
}

#[test]
fn douze_projecteurs_au_plus() {
    let mut app = app(0.0, 60.0);
    let mut most = 0;
    for _ in 0..1200 {
        app.update();
        most = most.max(active(&app).len());
        assert!(visible_roots(&mut app) <= MAX_PROJECTORS);
    }
    assert!(most > 0, "la ville livrée porte des projecteurs");
}

#[test]
fn les_plus_proches_gagnent() {
    // Voiture à l'arrêt près d'un placement : le résultat est exactement celui d'un tirage de force brute.
    let mut app = app(-380.0, 0.0);
    for _ in 0..5 {
        app.update();
    }
    let car = app
        .world_mut()
        .query_filtered::<&AbsolutePosition, With<StreamFocus>>()
        .single(app.world())
        .expect("la voiture")
        .0;
    let shown = active(&app);
    assert_ne!(shown.len(), 0, "des projecteurs sont affichés");
    let world = app.world().resource::<NeonWorld>();
    let mut all = Vec::new();
    let mut list = ProjectorList::default();
    for index in 0..40 {
        row_projectors(index + (-car.z / 26.0).round() as i64 - 20, world.scenery(), &mut list);
        all.extend_from_slice(list.as_slice());
    }
    let dist = |p: &Projector| (f64::from(p.x) - car.x).powi(2) + (p.z - car.z).powi(2);
    let farthest_shown = shown.iter().map(|(_, p)| dist(p)).fold(0.0, f64::max);
    // Les rangées examinées par le système vont de 40 m derrière à 420 m devant : seuls ceux-là comptent.
    let in_window = |p: &Projector| p.z < car.z + 40.0 + 13.0 && p.z > car.z - 420.0 - 13.0;
    let missing = all
        .iter()
        .filter(|p| in_window(p) && dist(p) < farthest_shown - 1e-6 && !shown.iter().any(|(_, s)| s == *p))
        .count();
    assert_eq!(
        missing, 0,
        "un projecteur plus proche que le plus lointain affiché est absent"
    );
}

#[test]
fn un_projecteur_garde_son_slot_en_avancant() {
    let mut app = app(0.0, 40.0);
    let mut seen: HashMap<(i64, i8), usize> = HashMap::new();
    let mut checked = 0;
    for _ in 0..2400 {
        app.update();
        for (slot, p) in active(&app) {
            let key = ((p.z * 100.0) as i64, p.side);
            match seen.get(&key) {
                Some(&before) => {
                    assert_eq!(before, slot, "le projecteur {key:?} a changé d'emplacement");
                    checked += 1;
                }
                None => {
                    seen.insert(key, slot);
                }
            }
        }
    }
    assert!(checked > 100, "peu de projecteurs suivis : {checked}");
}

#[test]
fn un_slot_libere_est_cache_pas_detruit() {
    let mut app = app(0.0, 80.0);
    for _ in 0..30 {
        app.update();
    }
    let count = app.world().iter_entities().count();
    let pool: Vec<Entity> = app
        .world()
        .resource::<ProjectorPool>()
        .slots()
        .iter()
        .flat_map(|s| [s.root, s.bar, s.text])
        .collect();
    for _ in 0..1500 {
        app.update();
    }
    for entity in pool {
        assert!(app.world().get_entity(entity).is_ok(), "{entity:?} a disparu");
    }
    assert_eq!(
        app.world().iter_entities().count(),
        count,
        "des entités ont été créées ou détruites"
    );
    assert!(visible_roots(&mut app) <= MAX_PROJECTORS);
}

fn set_quality(app: &mut App, from: usize, to: usize) {
    app.world_mut().write_message(cf_render::QualityChanged { from, to });
    app.update();
}

#[test]
fn en_qualite_minimale_seuls_les_proches_restent() {
    use cf_world_neon::graffiti::quality::MIN_QUALITY_RANGE_M;
    let mut app = app(0.0, 60.0);
    for _ in 0..600 {
        app.update();
    }
    set_quality(&mut app, 0, 3);
    for _ in 0..600 {
        app.update();
        let car = app
            .world_mut()
            .query_filtered::<&AbsolutePosition, With<StreamFocus>>()
            .single(app.world())
            .expect("la voiture")
            .0;
        // Le choix est refait à chaque nouvelle rangée : on laisse une rangée de marge.
        for (_, p) in active(&app) {
            let d = ((f64::from(p.x) - car.x).powi(2) + (p.z - car.z).powi(2)).sqrt();
            assert!(
                d <= MIN_QUALITY_RANGE_M + 60.0,
                "projecteur à {d:.0} m en qualité minimale"
            );
        }
    }
}

#[test]
fn changer_de_qualite_ne_detruit_aucune_entite() {
    let mut app = app(0.0, 60.0);
    for _ in 0..120 {
        app.update();
    }
    let count = app.world().iter_entities().count();
    for (from, to) in [(0, 1), (1, 2), (2, 3), (3, 0)] {
        set_quality(&mut app, from, to);
        for _ in 0..120 {
            app.update();
        }
        assert!(visible_roots(&mut app) <= MAX_PROJECTORS);
    }
    assert_eq!(
        app.world().iter_entities().count(),
        count,
        "des entités ont été créées ou détruites"
    );
}

#[test]
fn le_mot_impose_s_affiche_sur_tous_les_projecteurs_proches() {
    let mut app = app(0.0, 60.0);
    for _ in 0..300 {
        app.update();
    }
    let words = app.world().resource::<cf_render::GraffitiWords>().0.clone();
    assert!(words.len() > 1, "la liste livrée a plusieurs mots holographiques");
    let forced = words.len() - 1;
    app.world_mut().resource_mut::<cf_render::GraffitiTuning>().forced_word = Some(words[forced].clone());
    for _ in 0..5 {
        app.update();
    }
    let shown = active(&app);
    assert_ne!(shown.len(), 0, "des projecteurs sont affichés");
    assert!(shown.iter().all(|(_, p)| usize::from(p.word) == forced), "{shown:?}");
}

#[test]
fn projecteurs_coupes_aucun_n_est_affiche_et_la_ville_se_retire() {
    let mut app = app(0.0, 60.0);
    for _ in 0..300 {
        app.update();
    }
    assert_ne!(active(&app).len(), 0);
    app.world_mut().resource_mut::<cf_render::GraffitiTuning>().holo_enabled = false;
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(active(&app).len(), 0);
    assert_eq!(visible_roots(&mut app), 0);
}
