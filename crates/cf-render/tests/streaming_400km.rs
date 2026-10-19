//! Critères d'acceptation de J4-03, sans écran ni GPU : 400 km parcourus en accéléré dans la ville de boîtes.
//!
//! - nombre d'entités constant d'un bout à l'autre ;
//! - aucune allocation dans les systèmes du monde (origine, portes, tronçons, sujets, synchronisation) après la mise
//!   en route, mesurée par un allocateur compteur ;
//! - précision du monde rendu près de la caméra à 400 km (le « tremblement », chiffré).

// Allocateur compteur de test : seule utilisation de `unsafe`, encadrée ci-dessous.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use bevy::ecs::schedule::SingleThreadedExecutor;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use cf_render::streaming::{ChunkRoot, StreamedPiece};
use cf_render::{
    AbsolutePosition, BoxWorld, CfSet, CorePlugin, CurrentGates, FloatingOrigin, StreamFocus, StreamStats, SubjectKind,
    Subjects, WorldPlugin, WorldSet,
};
use cf_show::model::{CutGrid, FxGates, ShowGates};

/// Compte les allocations : sur le fil courant quand `ARMED` est vrai (nos systèmes, exécutés en série sur ce fil),
/// et sur tous les fils quand `WHOLE_FRAME` est vrai (l'image entière, Bevy compris, pour information).
struct CountingAlloc;

static OURS: AtomicU64 = AtomicU64::new(0);
static WHOLE: AtomicU64 = AtomicU64::new(0);
static WHOLE_FRAME: AtomicBool = AtomicBool::new(false);

thread_local! {
    // Initialisation constante, sans destructeur : lire ce drapeau depuis l'allocateur n'alloue pas.
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

fn count() {
    if ARMED.try_with(Cell::get).unwrap_or(false) {
        OURS.fetch_add(1, Ordering::Relaxed);
    }
    if WHOLE_FRAME.load(Ordering::Relaxed) {
        WHOLE.fetch_add(1, Ordering::Relaxed);
    }
}

// SAFETY: chaque méthode délègue à `System` avec exactement les arguments reçus, donc les contrats de `GlobalAlloc`
// (taille non nulle, alignement, pointeur issu du même allocateur pour `dealloc`/`realloc`) sont ceux que
// l'appelant garantit déjà. `count` n'alloue pas (atomiques et drapeau thread-local à initialisation constante).
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: contrat de `GlobalAlloc::alloc` transmis tel quel.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: contrat de `GlobalAlloc::alloc_zeroed` transmis tel quel.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        // SAFETY: `ptr` vient de cet allocateur, donc de `System` ; contrat de `realloc` transmis tel quel.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` vient de cet allocateur, donc de `System`.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

fn arm() {
    ARMED.with(|a| a.set(true));
}

fn disarm() {
    ARMED.with(|a| a.set(false));
}

/// 400 km en pas de 37,3 m (~2 200 km/h à 60 i/s) : un ou deux tronçons recyclés par image, 14 recentrages.
const DISTANCE_M: f64 = 400_000.0;
const STEP_M: f64 = 37.3;
/// Images de mise en route (premier remplissage de la fenêtre, premiers passages des systèmes).
const WARMUP_FRAMES: u32 = 120;
/// Écart toléré entre rendu `f32` et exact `f64`, vu de la caméra (m). Borne théorique : à 15 400 m de l'origine
/// (pire cas avant recentrage), un `f32` est arrondi à 2⁻¹⁰ m près ; la différence de deux positions arrondies plus
/// la composition parent × enfant donnent ≈ 1,5 mm. Un pixel 1080p (60° de champ) vaut 5 mm à 5 m de l'objectif ;
/// sans origine flottante, l'écart à 400 km atteint 3 cm.
const TOLERANCE_M: f64 = 2e-3;
/// La caméra, derrière et au-dessus de la voiture (m).
const CAMERA_OFFSET: DVec3 = DVec3::new(1.5, 2.8, 7.5);

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

#[derive(Component)]
struct Camera3dStandIn;

struct Scene {
    app: App,
    car: Entity,
    camera: Entity,
}

fn scene() -> Scene {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        CorePlugin,
        WorldPlugin::new(BoxWorld::new(7)),
    ))
    .init_resource::<Assets<Mesh>>()
    .init_resource::<Assets<StandardMaterial>>()
    .insert_resource(CurrentGates(gates(1.0)));
    // En série sur ce fil : le drapeau thread-local de l'allocateur ne voit que les systèmes encadrés.
    app.edit_schedule(Update, |schedule| {
        schedule.set_executor(SingleThreadedExecutor::new());
    });
    app.edit_schedule(PostUpdate, |schedule| {
        schedule.set_executor(SingleThreadedExecutor::new());
    });
    app.add_systems(Update, (arm.before(WorldSet::Origin), disarm.after(CfSet::Present)));
    app.add_systems(
        PostUpdate,
        (
            arm.before(WorldSet::Sync),
            disarm.after(WorldSet::Sync).before(TransformSystems::Propagate),
        ),
    );
    let car = app
        .world_mut()
        .spawn((StreamFocus, AbsolutePosition(DVec3::new(0.0, 0.6, 0.0))))
        .id();
    let camera = app
        .world_mut()
        .spawn((Camera3dStandIn, AbsolutePosition(CAMERA_OFFSET)))
        .id();
    Scene { app, car, camera }
}

impl Scene {
    /// Avance la voiture (et la caméra qui la suit) puis joue une image.
    fn drive_to(&mut self, z: f64) {
        let world = self.app.world_mut();
        let car = DVec3::new(0.0, 0.6, z);
        if let Some(mut position) = world.get_mut::<AbsolutePosition>(self.car) {
            position.0 = car;
        }
        if let Some(mut position) = world.get_mut::<AbsolutePosition>(self.camera) {
            position.0 = car + CAMERA_OFFSET;
        }
        self.app.update();
    }
}

#[test]
fn quatre_cents_km_entites_constantes_sans_allocation() {
    let mut scene = scene();
    let mut z = 0.0;
    for _ in 0..WARMUP_FRAMES {
        z -= STEP_M;
        scene.drive_to(z);
    }
    let entities = scene.app.world().entities().count_spawned();
    let filled_at_warmup = scene.app.world().resource::<StreamStats>().chunks_filled_total;
    OURS.store(0, Ordering::Relaxed);
    WHOLE.store(0, Ordering::Relaxed);

    let mut frames = 0_u64;
    let mut landmarks_seen = 0_usize;
    while z > -DISTANCE_M {
        z -= STEP_M;
        WHOLE_FRAME.store(true, Ordering::Relaxed);
        scene.drive_to(z);
        WHOLE_FRAME.store(false, Ordering::Relaxed);
        frames += 1;
        assert_eq!(
            scene.app.world().entities().count_spawned(),
            entities,
            "entité créée ou détruite à z = {z}"
        );
        landmarks_seen += scene
            .app
            .world()
            .resource::<Subjects>()
            .of_kind(SubjectKind::Landmark)
            .count();
    }

    let ours = OURS.load(Ordering::Relaxed);
    let whole = WHOLE.load(Ordering::Relaxed);
    let stats = *scene.app.world().resource::<StreamStats>();
    let origin = *scene.app.world().resource::<FloatingOrigin>();
    println!(
        "{frames} images, {} km : {entities} entités constantes ; allocations : {ours} dans nos systèmes, {whole} pour \
         l'image entière (Bevy compris) ; {} tronçons recyclés, {} recentrages, {landmarks_seen} sujets vus ; \
         {stats:?}",
        DISTANCE_M / 1000.0,
        stats.chunks_filled_total - filled_at_warmup,
        origin.shifts()
    );
    assert_eq!(ours, 0, "allocations dans les systèmes du monde en régime établi");
    assert_eq!(stats.chunks_live, 34);
    assert_eq!(stats.pieces_refused, 0);
    // Un tronçon de 26 m par tronçon parcouru : ~15 000 recyclages, tous servis par le pool.
    let recycled = stats.chunks_filled_total - filled_at_warmup;
    assert!((15_000..15_600).contains(&recycled), "{recycled} tronçons recyclés");
    assert_eq!(origin.shifts(), 14);
    assert!(landmarks_seen > 0, "les tours-repères doivent apparaître comme sujets");
}

/// Écart maximal (m) entre la position rendue (`f32`, telle que le GPU la reçoit) et la position exacte (`f64`) des
/// pièces affichées à moins de 60 m de la caméra, vues depuis la caméra ; et le nombre de pièces mesurées.
fn render_error_near_camera(scene: &mut Scene) -> (f64, usize) {
    let world = scene.app.world_mut();
    let camera_render = world
        .get::<GlobalTransform>(scene.camera)
        .map(GlobalTransform::translation);
    let camera_absolute = world.get::<AbsolutePosition>(scene.camera).map(|p| p.0);
    let (Some(camera_render), Some(camera_absolute)) = (camera_render, camera_absolute) else {
        return (f64::INFINITY, 0);
    };
    // Toute racine de tronçon reste près de l'origine : précision ≤ 1 mm partout dans la fenêtre.
    let mut roots = world.query_filtered::<&GlobalTransform, With<ChunkRoot>>();
    for root in roots.iter(world) {
        assert!(
            root.translation().abs().max_element() < 16_384.0,
            "racine à {:?}",
            root.translation()
        );
    }
    let mut pieces =
        world.query_filtered::<(&GlobalTransform, &Transform, &ChildOf, &Visibility), With<StreamedPiece>>();
    let mut anchors = world.query::<&AbsolutePosition>();
    let mut worst = 0.0_f64;
    let mut checked = 0;
    for (global, local, parent, visibility) in pieces.iter(world) {
        let Ok(anchor) = anchors.get(world, parent.parent()) else {
            continue;
        };
        let exact = anchor.0 + local.translation.as_dvec3() - camera_absolute;
        // Seules comptent les pièces affichées près de l'objectif : c'est là qu'un millimètre se voit.
        if *visibility == Visibility::Hidden || exact.length() > 60.0 {
            continue;
        }
        let rendered = (global.translation() - camera_render).as_dvec3();
        worst = worst.max((rendered - exact).length());
        checked += 1;
    }
    (worst, checked)
}

/// Le « tremblement » à 400 km, mesuré dans la vraie chaîne ECS (origine, synchronisation, propagation des
/// transforms). La scène y arrive d'un saut (comme une reprise après plantage), puis roule une seconde.
#[test]
fn a_400_km_le_monde_rendu_reste_au_millimetre() {
    let mut scene = scene();
    let mut z = -DISTANCE_M + 2_000.0;
    let mut worst = 0.0_f64;
    for _ in 0..60 {
        z -= STEP_M;
        scene.drive_to(z);
        let (error, checked) = render_error_near_camera(&mut scene);
        assert!(checked > 0);
        worst = worst.max(error);
    }
    println!("à 400 km : écart maximal {worst:e} m sur 60 images");
    assert!(worst < TOLERANCE_M, "écart de {worst} m à 400 km");
}

/// Le recentrage ne se voit pas : sur l'image où l'origine saute de 28,8 km, chaque pièce reste à un
/// ou deux millimètres de sa place exacte vue depuis la caméra, comme sur les images qui l'encadrent.
#[test]
fn le_recentrage_est_invisible() {
    let mut scene = scene();
    let mut z = -14_000.0;
    let mut worst = 0.0_f64;
    let mut shift_seen = false;
    while z > -17_000.0 {
        z -= STEP_M;
        let before = scene.app.world().resource::<FloatingOrigin>().shifts();
        scene.drive_to(z);
        let shifted = scene.app.world().resource::<FloatingOrigin>().shifts() > before;
        let (error, checked) = render_error_near_camera(&mut scene);
        assert!(checked > 0);
        if shifted {
            shift_seen = true;
            println!("recentrage à z = {z:.1} m : écart {error:e} m");
        }
        worst = worst.max(error);
    }
    assert!(shift_seen, "le parcours doit franchir la marge de recentrage");
    assert!(worst < TOLERANCE_M, "écart de {worst} m autour du recentrage");
}
