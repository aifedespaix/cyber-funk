//! Le contrat du trait `World` dans une vraie `App` Bevy (sans rendu) : portes, re-tirage, sujets, point suivi absent,
//! monde mal réglé.

use bevy::ecs::system::SystemParamItem;
use bevy::ecs::world::World as EcsWorld;
use bevy::math::DVec3;
use bevy::prelude::*;
use cf_render::streaming::StreamedPiece;
use cf_render::world::{SubjectContext, SubjectSink};
use cf_render::{
    AbsolutePosition, BoxWorld, ChunkInfo, ChunkPlan, ChunkStream, CorePlugin, CurrentGates, PieceRegistry, Replan,
    StreamFocus, StreamLayout, StreamStats, SubjectKind, Subjects, World, WorldError, WorldPlugin,
};
use cf_show::model::{CutGrid, FxGates, ShowGates};

/// Rang de la famille « voiture » dans la ville de boîtes (déclarée après « box » et « landmark »).
const CAR_ARCHETYPE: usize = 2;

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

fn app<W: World + Clone>(world: W) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, CorePlugin, WorldPlugin::new(world)))
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>();
    app
}

fn drive(app: &mut App, car: Entity, z: f64) {
    if let Some(mut position) = app.world_mut().get_mut::<AbsolutePosition>(car) {
        position.0 = DVec3::new(0.0, 0.6, z);
    }
    app.update();
}

fn visible_cars(app: &mut App) -> usize {
    let world = app.world_mut();
    let mut query = world.query::<(&StreamedPiece, &Visibility)>();
    query
        .iter(world)
        .filter(|(piece, visibility)| piece.archetype.index() == CAR_ARCHETYPE && **visibility != Visibility::Hidden)
        .count()
}

#[test]
fn les_portes_ne_changent_que_les_troncons_a_venir() {
    let mut app = app(BoxWorld::new(5));
    app.insert_resource(CurrentGates(gates(1.0)));
    let car = app.world_mut().spawn((StreamFocus, AbsolutePosition::default())).id();
    drive(&mut app, car, 0.0);
    assert_eq!(visible_cars(&mut app), 34 * 4);

    // Plus de trafic : les tronçons déjà là gardent leurs voitures (pas de saut à l'image)…
    app.insert_resource(CurrentGates(gates(0.0)));
    drive(&mut app, car, 0.0);
    assert_eq!(app.world().resource::<BoxWorld>().cars_per_chunk(), 0);
    assert_eq!(visible_cars(&mut app), 34 * 4);
    // … les nouveaux arrivent sans : après une fenêtre entière parcourue, plus aucune voiture.
    let mut z = 0.0;
    for _ in 0..40 {
        z -= 26.0;
        drive(&mut app, car, z);
    }
    assert_eq!(visible_cars(&mut app), 0);
}

#[test]
fn replan_all_retire_toute_la_fenetre() {
    let mut app = app(BoxWorld::new(5));
    let car = app.world_mut().spawn((StreamFocus, AbsolutePosition::default())).id();
    drive(&mut app, car, 0.0);
    drive(&mut app, car, 0.0);
    assert_eq!(app.world().resource::<StreamStats>().chunks_filled, 0);
    app.world_mut().resource_mut::<ChunkStream<BoxWorld>>().replan_all();
    drive(&mut app, car, 0.0);
    assert_eq!(app.world().resource::<StreamStats>().chunks_filled, 34);
    // Re-tirage d'un seul tronçon vivant, refus pour un tronçon hors fenêtre.
    let mut stream = app.world_mut().resource_mut::<ChunkStream<BoxWorld>>();
    assert!(stream.replan(3));
    assert!(!stream.replan(10_000));
    drive(&mut app, car, 0.0);
    let stream = app.world().resource::<ChunkStream<BoxWorld>>();
    assert_eq!(stream.filled().map(|c| c.index).collect::<Vec<_>>(), vec![3]);
}

#[test]
fn les_sujets_suivent_la_fenetre() {
    let mut app = app(BoxWorld::new(11));
    let car = app.world_mut().spawn((StreamFocus, AbsolutePosition::default())).id();
    let mut z = 0.0;
    let mut seen = 0;
    for _ in 0..2_000 {
        z -= 13.0;
        drive(&mut app, car, z);
        let stream = app.world().resource::<ChunkStream<BoxWorld>>();
        let live: Vec<i64> = stream.live().iter().map(|c| c.index).collect();
        let subjects = app.world().resource::<Subjects>();
        for subject in subjects.of_kind(SubjectKind::Landmark) {
            // Chaque tour-repère listée appartient à un tronçon vivant, à sa place absolue.
            let index = subject.id.cast_signed();
            assert!(live.contains(&index), "sujet d'un tronçon recyclé");
            let anchor_z = -(index as f64) * 26.0;
            assert!((subject.position.z - (anchor_z - 13.0)).abs() < 1e-6);
            seen += 1;
        }
        assert!(
            subjects.as_slice().len() <= live.len(),
            "la liste est vidée à chaque image"
        );
    }
    assert!(seen > 0);
}

#[test]
fn sans_point_suivi_la_fenetre_reste_au_depart() {
    let mut app = app(BoxWorld::new(5));
    app.update();
    assert_eq!(app.world().resource::<StreamStats>().chunks_filled, 34);
    for _ in 0..10 {
        app.update();
    }
    let stats = app.world().resource::<StreamStats>();
    assert_eq!((stats.chunks_filled, stats.chunks_filled_total), (0, 34));
    let first = app
        .world()
        .resource::<ChunkStream<BoxWorld>>()
        .live()
        .iter()
        .map(|c| c.index)
        .min();
    assert_eq!(first, Some(-15));
}

/// Un monde dont la géométrie est absurde : il doit rester vide, journaliser, et ne jamais faire paniquer l'app.
#[derive(Resource, Clone, Debug)]
struct BrokenWorld;

impl World for BrokenWorld {
    const ID: &'static str = "broken";
    type SubjectParam = ();

    fn layout(&self) -> StreamLayout {
        StreamLayout {
            chunk_length_m: 0.0,
            behind: 1,
            ahead: 1,
            seed: 0,
        }
    }

    fn setup(&mut self, _: &mut EcsWorld, _: &mut PieceRegistry) -> Result<(), WorldError> {
        Ok(())
    }

    fn plan_chunk(&self, _: &ChunkInfo, _: &mut ChunkPlan) {}

    fn apply_gates(&mut self, _: &ShowGates) -> Replan {
        Replan::Keep
    }

    fn subjects(&self, _: &SubjectContext<'_>, &(): &SystemParamItem<'_, '_, ()>, _: &mut SubjectSink<'_>) {}
}

#[test]
fn un_monde_mal_regle_reste_vide_sans_paniquer() {
    let mut app = app(BrokenWorld);
    app.insert_resource(CurrentGates(gates(1.0)));
    app.world_mut().spawn((StreamFocus, AbsolutePosition::default()));
    for _ in 0..3 {
        app.update();
    }
    assert!(app.world().get_resource::<ChunkStream<BrokenWorld>>().is_none());
    assert_eq!(app.world().resource::<Subjects>().as_slice().len(), 0);
}
