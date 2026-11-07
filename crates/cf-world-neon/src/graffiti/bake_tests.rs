//! Tests de la cuisson des atlas de graffitis : mots sautés sans erreur, une cellule par mot gardé et alignée sur la
//! bibliothèque, masque holographique monochrome et halo flou, budget de temps, aucune texture dessinée après le
//! démarrage.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use cf_render::warmup::LoadTracker;

use std::fmt::Write as _;

use super::atlas::{BakedGraffiti, CellRect, LayerFormat, bake, bake_assets};
use super::bake::{GraffitiAtlas, GraffitiBake, GraffitiPlugin};
use super::{GraffitiStyle, GraffitiWord, Placement, ProjectorSize};
use crate::NeonWorld;
use crate::scenery::graffiti_plan::GraffitiLibrary;

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn fonts() -> PathBuf {
    assets().join("fonts")
}

fn word(text: &str, font: &str, style: GraffitiStyle, wall: bool, holo: bool) -> GraffitiWord {
    GraffitiWord {
        text: text.to_owned(),
        font: font.to_owned(),
        style,
        wall,
        holo,
        hues: vec![300.0],
    }
}

/// Une table de licences qui couvre les polices `names`.
fn licences(names: &[&str]) -> String {
    let mut md = String::from("| Nom | Fichier | Source | Licence | Date |\n| --- | --- | --- | --- | --- |\n");
    for name in names {
        let _ = writeln!(md, "| {name} | `{name}.ttf` | test | CC0 | 2026-11-03 |");
    }
    md
}

fn bake_with(words: &[GraffitiWord], placements: &[Placement], md: &str, dir: &Path) -> BakedGraffiti {
    bake(words, placements, md, dir)
}

fn inked(baked: &BakedGraffiti, rect: CellRect, rgba: bool) -> usize {
    let layers = if rgba { &baked.wall } else { &baked.holo };
    let bpp = if rgba { 4 } else { 1 };
    layers.crop(rect).chunks(bpp).filter(|px| px[bpp - 1] > 32).count()
}

#[test]
fn une_police_absente_saute_ses_mots_avec_avertissement() {
    let words = [
        word("FANTOME", "Absente", GraffitiStyle::Fill, true, true),
        word("NEON", "DejaVuSans-Bold", GraffitiStyle::Fill, true, true),
    ];
    let baked = bake_with(&words, &[], &licences(&["Absente", "DejaVuSans-Bold"]), &fonts());
    assert_eq!(baked.library.wall_words(), ["NEON".to_owned()]);
    assert_eq!(baked.library.holo_words(), ["NEON".to_owned()]);
    assert_eq!(baked.skipped.len(), 1, "{:?}", baked.skipped);
    assert!(
        baked.skipped[0].contains("FANTOME") && baked.skipped[0].contains("absente"),
        "{:?}",
        baked.skipped
    );
    assert!(baked.cells.wall_cell(1).is_some() && baked.cells.wall_cell(2).is_none());

    // Sans aucune police (dossier vide), rien n'est cuit et rien ne casse.
    let empty = tempfile::tempdir().expect("dossier temporaire");
    let baked = bake_with(&words, &[], &licences(&["Absente", "DejaVuSans-Bold"]), empty.path());
    assert!(baked.library.is_empty());
    assert_eq!(baked.skipped.len(), 2);
    assert!(baked.cells.wall_cell(1).is_none() && baked.cells.holo_cell(0).is_none());
}

#[test]
fn une_police_sans_ligne_de_licence_saute_ses_mots() {
    let words = [
        word("MONO", "DejaVuSansMono-Bold", GraffitiStyle::Fill, true, false),
        word("SANS", "DejaVuSans-Bold", GraffitiStyle::Outline, true, false),
    ];
    // Le fichier `DejaVuSansMono-Bold.ttf` existe, mais sa ligne est à compléter : il ne compte pas.
    let md = format!(
        "{}| Mono | `DejaVuSansMono-Bold.ttf` | ? | À COMPLÉTER | |\n",
        licences(&["DejaVuSans-Bold"])
    );
    let baked = bake_with(&words, &[], &md, &fonts());
    assert_eq!(baked.library.wall_words(), ["SANS".to_owned()]);
    assert_eq!(baked.skipped.len(), 1);
    assert!(
        baked.skipped[0].contains("MONO") && baked.skipped[0].contains("licence"),
        "{:?}",
        baked.skipped
    );
}

#[test]
fn la_cuisson_donne_une_cellule_par_mot_garde() {
    let baked = bake_assets(&assets());
    assert!(baked.skipped.is_empty(), "{:?}", baked.skipped);
    assert!(baked.warnings.is_empty(), "{:?}", baked.warnings);
    let walls = baked.library.wall_words().len();
    let holos = baked.library.holo_words().len();
    assert!(walls >= 20 && holos >= 15, "{walls} muraux, {holos} holo");
    assert!(baked.cells.wall_cell(0).is_none(), "0 : aucun graffiti");
    assert!(baked.cells.wall_cell(walls as u8 + 1).is_none());
    assert!(baked.cells.holo_cell(holos as u16).is_none());

    let mut rects: Vec<(bool, CellRect)> = Vec::new();
    for k in 1..=walls as u8 {
        let rect = baked.cells.wall_cell(k).expect("cellule murale");
        assert!(inked(&baked, rect, true) > 200, "motif {k} vide");
        rects.push((true, rect));
    }
    for i in 0..holos as u16 {
        let cell = baked.cells.holo_cell(i).expect("cellule holo");
        assert_eq!((cell.sharp.w, cell.sharp.h), (cell.halo.w, cell.halo.h));
        assert!(inked(&baked, cell.sharp, false) > 200, "mot holo {i} vide");
        rects.push((false, cell.sharp));
        rects.push((false, cell.halo));
    }
    for (rgba, r) in &rects {
        let layers = if *rgba { &baked.wall } else { &baked.holo };
        assert!(u32::from(r.layer) < layers.count);
        assert!(u32::from(r.x) + u32::from(r.w) <= layers.w && u32::from(r.y) + u32::from(r.h) <= layers.h);
        assert!(r.letters > 0.3 && r.letters <= 1.0, "{r:?}");
    }
    // Aucune cellule n'en recouvre une autre.
    for (i, (ra, a)) in rects.iter().enumerate() {
        for (rb, b) in &rects[i + 1..] {
            let apart = ra != rb
                || a.layer != b.layer
                || a.x + a.w <= b.x
                || b.x + b.w <= a.x
                || a.y + a.h <= b.y
                || b.y + b.h <= a.y;
            assert!(apart, "{a:?} recouvre {b:?}");
        }
    }
    assert!(baked.wall.w <= 4096 && baked.wall.h <= 4096 && baked.holo.w <= 4096 && baked.holo.h <= 4096);
}

#[test]
fn l_atlas_reste_aligne_sur_la_bibliotheque_quand_un_mot_est_saute() {
    let md = licences(&["DejaVuSans-Bold", "DejaVuSansMono-Bold", "Absente"]);
    let lost = word("PERDU", "Absente", GraffitiStyle::Fill, true, true);
    let b = word("BETA", "DejaVuSans-Bold", GraffitiStyle::Extrude, true, false);
    let c = word("GAMMA", "DejaVuSansMono-Bold", GraffitiStyle::Outline, true, true);
    let placement = Placement {
        z_m: 100.0,
        side: 1,
        height_m: 3.0,
        size: ProjectorSize::Small,
        word: "GAMMA".to_owned(),
        yaw_deg: 0.0,
    };
    let all = bake_with(
        &[lost, b.clone(), c.clone()],
        std::slice::from_ref(&placement),
        &md,
        &fonts(),
    );
    let (expected, _) = GraffitiLibrary::new(&[b.clone(), c.clone()], std::slice::from_ref(&placement));
    assert_eq!(all.library, expected, "la bibliothèque ne compte que les mots cuits");
    assert_eq!(
        all.library.manual()[0].word,
        0,
        "le placement désigne GAMMA, rang holo 0"
    );

    // Chaque cellule est exactement celle du mot cuit seul : l'index du tag désigne bien ce mot.
    let only_b = bake_with(std::slice::from_ref(&b), &[], &md, &fonts());
    let only_c = bake_with(std::slice::from_ref(&c), &[], &md, &fonts());
    let cell = |x: &BakedGraffiti, k: u8| x.wall.crop(x.cells.wall_cell(k).expect("cellule"));
    assert_eq!(cell(&all, 1), cell(&only_b, 1), "motif 1 = BETA");
    assert_eq!(cell(&all, 2), cell(&only_c, 1), "motif 2 = GAMMA");
    let holo = |x: &BakedGraffiti| x.holo.crop(x.cells.holo_cell(0).expect("holo").sharp);
    assert_eq!(holo(&all), holo(&only_c), "mot holo 0 = GAMMA");
    assert!(all.cells.holo_cell(1).is_none());
}

/// Énergie des différences entre pixels voisins : forte sur un bord net, faible sur un flou.
fn local_variance(pixels: &[u8], w: usize) -> f64 {
    let mut sum = 0.0;
    for (i, v) in pixels.iter().enumerate() {
        if i % w + 1 < w {
            let d = f64::from(*v) - f64::from(pixels[i + 1]);
            sum += d * d;
        }
        if let Some(below) = pixels.get(i + w) {
            let d = f64::from(*v) - f64::from(*below);
            sum += d * d;
        }
    }
    sum / pixels.len() as f64
}

#[test]
fn le_masque_holo_est_monochrome_et_le_halo_est_plus_flou() {
    let words = [word("ECHO", "DejaVuSans-Bold", GraffitiStyle::Fill, false, true)];
    let baked = bake_with(&words, &[], &licences(&["DejaVuSans-Bold"]), &fonts());
    assert_eq!(
        baked.holo.format,
        LayerFormat::R8,
        "un seul canal : la couleur vient de la palette"
    );
    assert_eq!(
        baked.holo.data.len(),
        (baked.holo.w * baked.holo.h * baked.holo.count) as usize
    );
    assert_eq!(baked.wall.format, LayerFormat::Rgba);
    let cell = baked.cells.holo_cell(0).expect("cellule");
    let w = usize::from(cell.sharp.w);
    let sharp = baked.holo.crop(cell.sharp);
    let halo = baked.holo.crop(cell.halo);
    let (vs, vh) = (local_variance(&sharp, w), local_variance(&halo, w));
    assert!(
        vh * 4.0 < vs,
        "halo {vh:.1} pas assez flou devant le masque net {vs:.1}"
    );
    // Le halo déborde de la forme nette : des pixels vides dans le masque s'allument dans le halo.
    let spill = sharp.iter().zip(&halo).filter(|(s, h)| **s == 0 && **h > 8).count();
    assert!(spill > 500, "{spill} pixels de halo hors des lettres");
}

/// Critère de la spec : moins d'une seconde pour la liste livrée. Le temps mural dépend de la machine et de sa charge
/// (tests en parallèle, compilations) : lancé à part, en `--release` (`cargo test --release -p cf-world-neon
/// la_cuisson_tient -- --ignored --nocapture`).
#[test]
#[ignore = "dépend de la machine : à lancer en --release"]
fn la_cuisson_tient_sous_une_seconde_sur_la_liste_livree() {
    let start = Instant::now();
    let baked = bake_assets(&assets());
    let seconds = start.elapsed().as_secs_f64();
    eprintln!(
        "graffitis : {} muraux, {} holo, mural {}×{}×{}, holo {}×{}×{}, {:.1} Mo, {seconds:.3} s",
        baked.library.wall_words().len(),
        baked.library.holo_words().len(),
        baked.wall.w,
        baked.wall.h,
        baked.wall.count,
        baked.holo.w,
        baked.holo.h,
        baked.holo.count,
        baked.bytes() as f64 / 1e6,
    );
    assert!(seconds < 1.0, "{seconds:.3} s");
}

/// Compte les événements de texture (création, modification) vus par l'application.
#[derive(Resource, Default)]
struct ImageEvents(usize);

fn count_image_events(mut events: MessageReader<AssetEvent<Image>>, mut count: ResMut<ImageEvents>) {
    count.0 += events.read().count();
}

#[test]
fn aucune_texture_n_est_dessinee_apres_le_startup() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Image>()
        .init_resource::<LoadTracker>()
        .init_resource::<ImageEvents>()
        .insert_resource(NeonWorld::default())
        .add_plugins(GraffitiPlugin::always(assets()))
        .add_systems(Last, count_image_events);
    app.update();
    {
        let tracker = app.world().resource::<LoadTracker>();
        assert!(!tracker.is_complete(), "le show attend la cuisson des graffitis");
        assert!(tracker.steps().iter().any(|s| s.label.contains("Graffitis")));
    }
    let start = Instant::now();
    while app.world().get_resource::<GraffitiAtlas>().is_none() {
        assert!(start.elapsed() < Duration::from_secs(60), "pas cuit en 60 s");
        app.update();
        std::thread::sleep(Duration::from_millis(5));
    }
    app.update();
    assert!(app.world().resource::<LoadTracker>().is_complete());
    assert!(
        app.world().get_resource::<GraffitiBake>().is_none(),
        "la tâche est rendue"
    );
    let atlas = app.world().resource::<GraffitiAtlas>();
    assert!(atlas.bake_s > 0.0 && atlas.bytes > 0 && atlas.skipped.is_empty());
    assert!(atlas.wall_cell(1).is_some() && atlas.holo_cell(0).is_some());
    // La bibliothèque est arrivée dans la ville en même temps que l'atlas, alignée sur lui.
    let lib = app.world().resource::<NeonWorld>().scenery().graffiti().clone();
    assert!(atlas.wall_cell(lib.wall_words().len() as u8).is_some());
    assert!(atlas.wall_cell(lib.wall_words().len() as u8 + 1).is_none());
    assert!(atlas.holo_cell(lib.holo_words().len() as u16 - 1).is_some());

    let images = app.world().resource::<Assets<Image>>().len();
    app.world_mut().resource_mut::<ImageEvents>().0 = 0;
    for _ in 0..30 {
        app.update();
    }
    assert_eq!(
        app.world().resource::<ImageEvents>().0,
        0,
        "une texture a été dessinée en régime établi"
    );
    assert_eq!(app.world().resource::<Assets<Image>>().len(), images);
}
