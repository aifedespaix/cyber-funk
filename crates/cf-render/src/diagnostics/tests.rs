//! Tests de la surcouche sans rendu : `App` avec `MinimalPlugins`, horloge pilotée à la main.

use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::diagnostic::{Diagnostic, DiagnosticMeasurement, DiagnosticPath, DiagnosticsStore};
use bevy::platform::time::Instant;
use bevy::time::TimeUpdateStrategy;

use super::*;
use crate::StreamStats;

fn app(frame: Duration) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), DiagnosticsPlugin::default()))
        .init_asset::<Mesh>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(frame));
    // La première image n'a pas de delta : on la consomme avant de mesurer.
    app.update();
    app
}

fn spawn_boxes(app: &mut App, kinds: &[Handle<Mesh>], count: usize, visible: bool) -> Vec<Entity> {
    (0..count)
        .map(|i| {
            let mesh = kinds[i % kinds.len()].clone();
            let view = if visible {
                ViewVisibility::VISIBLE
            } else {
                ViewVisibility::HIDDEN
            };
            app.world_mut().spawn((Mesh3d(mesh), view)).id()
        })
        .collect()
}

fn meshes(app: &mut App, n: usize) -> Vec<Handle<Mesh>> {
    let mut assets = app.world_mut().resource_mut::<Assets<Mesh>>();
    (0..n)
        .map(|i| assets.add(Mesh::from(Cuboid::new(1.0 + i as f32, 1.0, 1.0))))
        .collect()
}

fn run(app: &mut App, frames: usize) {
    for _ in 0..frames {
        app.update();
    }
}

fn snapshot(app: &App) -> DiagnosticsSnapshot {
    app.world().resource::<DiagnosticsSnapshot>().clone()
}

#[test]
fn les_temps_d_image_suivent_la_cadence() {
    let mut app = app(Duration::from_millis(20));
    run(&mut app, 30);
    let s = snapshot(&app);
    assert!((s.frame_ms - 20.0).abs() < 0.01, "{}", s.frame_ms);
    assert!((s.frame_mean_ms - 20.0).abs() < 0.01);
    assert!((s.frame_p99_ms - 20.0).abs() < 0.01);
    assert!((s.fps - 50.0).abs() < 0.1, "{}", s.fps);
}

#[test]
fn le_p99_voit_les_accrocs_que_la_moyenne_lisse() {
    let mut app = app(Duration::from_millis(10));
    run(&mut app, 95);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(100)));
    run(&mut app, 5);
    let s = snapshot(&app);
    assert!(s.frame_p99_ms > 90.0, "p99 {}", s.frame_p99_ms);
    assert!(s.frame_mean_ms < 25.0, "moyenne {}", s.frame_mean_ms);
}

#[test]
fn les_compteurs_bougent_quand_on_ajoute_et_retire_des_objets() {
    let mut app = app(Duration::from_millis(16));
    let kinds = meshes(&mut app, 3);
    run(&mut app, 1);
    let vide = snapshot(&app);

    let visibles = spawn_boxes(&mut app, &kinds, 5000, true);
    let caches = spawn_boxes(&mut app, &kinds, 2000, false);
    run(&mut app, MESH_KIND_PERIOD);
    let plein = snapshot(&app);
    assert_eq!(plein.renderable_entities, vide.renderable_entities + 7000);
    assert_eq!(plein.visible_entities, vide.visible_entities + 5000);
    assert_eq!(plein.visible_mesh_kinds, 3);
    assert!(
        plein.world_entities >= vide.world_entities + 7000,
        "le total du monde suit : {} -> {}",
        vide.world_entities,
        plein.world_entities
    );

    for entity in visibles {
        app.world_mut().despawn(entity);
    }
    run(&mut app, MESH_KIND_PERIOD);
    let cullees = snapshot(&app);
    assert_eq!(cullees.visible_entities, vide.visible_entities);
    assert_eq!(cullees.renderable_entities, vide.renderable_entities + 2000);
    assert_eq!(cullees.visible_mesh_kinds, 0);

    for entity in caches {
        app.world_mut().despawn(entity);
    }
    run(&mut app, 1);
    assert_eq!(snapshot(&app).renderable_entities, vide.renderable_entities);
}

/// Période de recomptage des maillages distincts (en images), plus une image de marge.
const MESH_KIND_PERIOD: usize = 11;

#[test]
fn les_compteurs_du_streaming_sont_relus_s_ils_existent() {
    let mut app = app(Duration::from_millis(16));
    run(&mut app, 1);
    assert_eq!(snapshot(&app).stream, None);
    app.insert_resource(StreamStats {
        chunks_live: 7,
        chunks_capacity: 9,
        pieces_used: 120,
        ..default()
    });
    run(&mut app, 1);
    let stream = snapshot(&app).stream;
    assert_eq!(stream.map(|s| (s.chunks_live, s.pieces_used)), Some((7, 120)));
}

fn publish(app: &mut App, path: &'static str, value: f64) {
    let path = DiagnosticPath::new(path);
    let mut store = app.world_mut().resource_mut::<DiagnosticsStore>();
    if store.get(&path).is_none() {
        store.add(Diagnostic::new(path.clone()));
    }
    if let Some(diagnostic) = store.get_mut(&path) {
        diagnostic.add_measurement(DiagnosticMeasurement {
            time: Instant::now(),
            value,
        });
    }
}

#[test]
fn sans_timestamps_gpu_le_total_gpu_reste_indisponible() {
    let mut app = app(Duration::from_millis(16));
    publish(&mut app, "render/main_opaque_pass_3d/elapsed_cpu", 0.8);
    run(&mut app, 1);
    let s = snapshot(&app);
    assert_eq!(s.gpu_total_ms, None, "aucun elapsed_gpu : champ indisponible, pas zéro");
    assert_eq!(s.passes.len(), 1);
    assert_eq!(s.passes[0].cpu_ms, Some(0.8));
    assert_eq!(s.passes[0].gpu_ms, None);
}

#[test]
fn le_total_gpu_additionne_les_passes_de_premier_niveau_seulement() {
    let mut app = app(Duration::from_millis(16));
    publish(&mut app, "render/shadows/elapsed_gpu", 1.5);
    publish(&mut app, "render/shadows/cascade_0/elapsed_gpu", 0.7);
    publish(&mut app, "render/main_pass/elapsed_gpu", 3.0);
    publish(&mut app, "render/main_pass/elapsed_cpu", 0.4);
    publish(&mut app, "render/main_pass/vertex_shader_invocations", 1e6);
    run(&mut app, 1);
    let s = snapshot(&app);
    assert_eq!(s.passes.len(), 3, "les statistiques de pipeline ne sont pas des passes");
    assert!((s.gpu_total_ms.unwrap_or_default() - 4.5).abs() < 1e-9);
    // Une mesure plus récente remplace l'ancienne, sans dupliquer la passe.
    publish(&mut app, "render/main_pass/elapsed_gpu", 5.0);
    run(&mut app, 1);
    let s = snapshot(&app);
    assert_eq!(s.passes.len(), 3);
    assert!((s.gpu_total_ms.unwrap_or_default() - 6.5).abs() < 1e-9);
}

#[test]
fn la_session_enregistre_une_ligne_par_image_et_s_exporte_sans_fenetre() {
    let mut app = app(Duration::from_millis(16));
    let kinds = meshes(&mut app, 2);
    spawn_boxes(&mut app, &kinds, 40, true);
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    app.world_mut().resource_mut::<MeasurementSession>().start(now);
    run(&mut app, 20);
    app.world_mut().resource_mut::<MeasurementSession>().stop();
    run(&mut app, 5);

    let session = app.world().resource::<MeasurementSession>();
    assert_eq!(session.rows().len(), 20, "rien n'est ajouté après l'arrêt");
    assert!(
        session.rows().windows(2).all(|w| w[1].t_s > w[0].t_s),
        "le temps avance"
    );
    assert!(session.rows().iter().all(|r| r.visible_entities == 40));

    let mut csv = Vec::new();
    assert!(session.write_csv(&mut csv).is_ok());
    let text = String::from_utf8_lossy(&csv);
    assert_eq!(text.lines().count(), 21);
    assert!(text.starts_with(CSV_HEADER));
}

#[test]
fn l_echelle_de_rendu_vaut_un_tant_que_la_qualite_ne_l_a_pas_changee() {
    let mut app = app(Duration::from_millis(16));
    run(&mut app, 1);
    assert!((snapshot(&app).render_scale - 1.0).abs() < f32::EPSILON);
    app.insert_resource(RenderScale(0.8));
    run(&mut app, 1);
    assert!((snapshot(&app).render_scale - 0.8).abs() < f32::EPSILON);
}
