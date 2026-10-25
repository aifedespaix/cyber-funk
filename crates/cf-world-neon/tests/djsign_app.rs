//! L'enseigne du DJ dans une vraie `App` Bevy, sans rendu : entités préallouées dans la couche monde, enseigne qui apparaît
//! au changement de DJ (lettres, immeuble, zone dégagée, sujet `dj-sign`), recyclage, aucune entité créée en régime établi.

#![allow(clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::time::Duration;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::world::SubjectKind;
use cf_render::{
    AbsolutePosition, CorePlugin, CurrentShow, Layer, PalettePlugin, SheetSlot, ShowSheet, StreamFocus, Subjects,
    WorldPlugin,
};
use cf_show::director::ShowDirector;
use cf_show::drop::DropTitles;
use cf_show::load::{LoadedShow, LookLibrary};
use cf_world_neon::djsign::{DjSignGlyphs, DjSignIntro, DjSignPlugin, DjSignRes, DjSignRig};
use cf_world_neon::{FacadePlugin, NeonWorld, RoadLook};
use rand_pcg::Pcg32;

const FRAME: Duration = Duration::from_micros(16_667);

fn demo_show() -> CurrentShow {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let looks = LookLibrary::load_dir(&assets.join("looks")).expect("looks");
    let show = LoadedShow::load(&assets.join("shows/demo.ron"), &looks).expect("show");
    let director = ShowDirector::<Pcg32>::new(&show, DropTitles::empty(), 1).expect("chef");
    CurrentShow(director.state().clone())
}

fn sheet(djs: &[&str]) -> ShowSheet {
    ShowSheet {
        event_name: "Démesure".to_owned(),
        dj_sign: true,
        slots: djs
            .iter()
            .map(|dj| SheetSlot {
                dj: (*dj).to_owned(),
                schedule: None,
                cues: Vec::new(),
            })
            .collect(),
    }
}

fn app() -> App {
    app_with(true)
}

/// `with_show` : un spectacle tourne. Sans lui, personne ne demande l'enseigne.
fn app_with(with_show: bool) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        PalettePlugin,
        WorldPlugin::new(NeonWorld::default()),
        FacadePlugin,
        DjSignPlugin::always(),
    ))
    .init_asset::<Image>()
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>()
    .init_resource::<RoadLook>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME))
    .insert_resource(sheet(&["KRMN", "Nova Kaito"]));
    if with_show {
        app.insert_resource(demo_show());
    }
    // La voiture roule à 60 m/s vers −z.
    app.world_mut().spawn((StreamFocus, AbsolutePosition::default()));
    app
}

fn drive(app: &mut App, frames: usize) {
    for _ in 0..frames {
        let world = app.world_mut();
        let mut cars = world.query_filtered::<&mut AbsolutePosition, With<StreamFocus>>();
        for mut position in cars.iter_mut(world) {
            position.0.z -= 60.0 * FRAME.as_secs_f64();
        }
        app.update();
    }
}

fn car_z(app: &mut App) -> f64 {
    let mut q = app.world_mut().query_filtered::<&AbsolutePosition, With<StreamFocus>>();
    q.single(app.world()).expect("le point suivi").0.z
}

/// Passe au DJ suivant, comme le chef du spectacle le publie.
fn next_dj(app: &mut App) {
    let mut show = app.world_mut().resource_mut::<CurrentShow>();
    show.0.slot += 1;
    show.0.slot_seq += 1;
}

fn visible_letters(app: &mut App) -> usize {
    let letters: Vec<Entity> = app.world().resource::<DjSignRig>().letters().collect();
    letters
        .into_iter()
        .filter(|e| app.world().get::<Visibility>(*e) != Some(&Visibility::Hidden))
        .count()
}

#[test]
fn les_entites_sont_preallouees_dans_la_couche_monde_et_cachees_au_repos() {
    // Sans spectacle, rien n'appelle l'enseigne : tout dort.
    let mut app = app_with(false);
    drive(&mut app, 5);
    let rig = app.world().resource::<DjSignRig>();
    // La racine, vingt lettres, l'ossature et deux pavés d'immeuble.
    assert_eq!(rig.entity_count(), 24);
    let world_layers = Layer::World.render_layers();
    let drawn: Vec<Entity> = rig.letters().chain([rig.frame()]).chain(rig.site()).collect();
    for entity in drawn {
        assert_eq!(
            app.world().get::<RenderLayers>(entity),
            Some(&world_layers),
            "{entity:?}"
        );
        assert_eq!(
            app.world().get::<Visibility>(entity),
            Some(&Visibility::Hidden),
            "{:?}",
            app.world().get::<Name>(entity)
        );
    }
    assert!(app.world().resource::<DjSignGlyphs>().cells.contains_key(&'K'));
    assert!(!app.world().resource::<DjSignRes>().0.visible());
}

#[test]
fn le_premier_dj_appelle_l_enseigne_des_l_ouverture_du_show() {
    // Le chef du spectacle charge le premier créneau (`slot_seq` monte) : comme dans Sillage, son nom s'écrit tout de suite.
    let mut app = app();
    drive(&mut app, 5);
    let director = &app.world().resource::<DjSignRes>().0;
    assert!(director.visible());
    assert_eq!(director.sign().name(), "KRMN");
}

#[test]
fn changer_de_dj_fait_apparaitre_son_nom_et_publie_le_sujet() {
    let mut app = app();
    drive(&mut app, 5);
    next_dj(&mut app);
    drive(&mut app, 5);

    let director = &app.world().resource::<DjSignRes>().0;
    assert!(director.visible());
    assert_eq!(director.sign().name(), "NOVA KAITO");
    let site = *director.site().expect("immeuble posé");
    let z = car_z(&mut app);
    // 520 m devant la voiture, sur le côté de la route.
    assert!(site.sign_z < z - 400.0, "{} / {z}", site.sign_z);
    assert!(site.x.abs() > 18.0);

    // Neuf lettres (l'espace ne compte pas), l'immeuble et l'ossature sont montrés.
    assert_eq!(visible_letters(&mut app), 9);
    let rig = app.world().resource::<DjSignRig>();
    assert_eq!(app.world().get::<Visibility>(rig.root()), Some(&Visibility::Inherited));
    assert_eq!(app.world().get::<Visibility>(rig.frame()), Some(&Visibility::Inherited));
    for block in rig.site() {
        assert_eq!(app.world().get::<Visibility>(block), Some(&Visibility::Inherited));
    }
    let root_at = app.world().get::<AbsolutePosition>(rig.root()).expect("position").0;
    assert!((root_at.z - site.sign_z).abs() < 1e-6 && (root_at.y - f64::from(site.roof_y)).abs() < 1e-6);

    // Le sujet `dj-sign` : centre des lettres, largeur totale, face vers +z.
    let subjects = app.world().resource::<Subjects>();
    let subject = subjects.of_kind(SubjectKind::DjSign).next().expect("sujet dj-sign");
    assert_eq!(subject.facing, Some(Vec3::Z));
    assert!(subject.size > 40.0 && subject.size <= 132.0 + 1e-3, "{}", subject.size);
    assert!((subject.position.z - site.sign_z).abs() < 1e-6);
    assert!(subject.position.y > f64::from(site.roof_y));
    assert_eq!(subjects.of_kind(SubjectKind::DjSign).count(), 1);
}

#[test]
fn la_ville_s_ecarte_devant_l_enseigne() {
    let mut app = app();
    drive(&mut app, 5);
    next_dj(&mut app);
    drive(&mut app, 5);
    let site = *app.world().resource::<DjSignRes>().0.site().expect("immeuble posé");
    let side = if site.x < 0.0 { -1 } else { 1 };
    let cleared = app.world().resource::<NeonWorld>();
    let free = NeonWorld::default();
    // Dans la zone, la ville seule borde la route par endroits ; avec l'enseigne, plus un immeuble de ce côté.
    let zs: Vec<f64> = (0..64)
        .map(|i| site.clearing.z0 + f64::from(i) * 2.0)
        .take_while(|z| *z < site.clearing.z1)
        .collect();
    let bordered = zs.iter().filter(|z| free.facade(side, **z, 2.0).is_some()).count();
    assert!(
        bordered > 5,
        "la ville de référence doit border la route dans la zone ({bordered})"
    );
    for z in &zs {
        assert!(
            cleared.facade(side, *z, 2.0).is_none(),
            "immeuble dans la zone dégagée à z = {z}"
        );
    }
    // Hors de la zone, la ville est intacte.
    let outside = site.clearing.z1 + 300.0;
    assert_eq!(cleared.facade(side, outside, 2.0), free.facade(side, outside, 2.0));
}

#[test]
fn l_enseigne_est_rangee_quand_la_voiture_l_a_depassee_et_rien_n_est_cree() {
    let mut app = app();
    drive(&mut app, 30);
    let entities = app.world().entities().len();
    next_dj(&mut app);
    drive(&mut app, 5);
    assert!(app.world().resource::<DjSignRes>().0.visible());
    let sign_z = app.world().resource::<DjSignRes>().0.site().expect("posé").sign_z;
    // La voiture roule à 60 m/s : 520 m + 240 m de dépassement, soit un peu plus de 12 s.
    drive(&mut app, 60 * 14);
    assert!(car_z(&mut app) < sign_z - 200.0);
    let director = &app.world().resource::<DjSignRes>().0;
    assert!(director.site().is_none(), "enseigne rangée");
    assert_eq!(visible_letters(&mut app), 0);
    let rig = app.world().resource::<DjSignRig>();
    assert_eq!(app.world().get::<Visibility>(rig.root()), Some(&Visibility::Hidden));
    assert_eq!(
        app.world().entities().len(),
        entities,
        "aucune entité créée ni détruite"
    );
}

#[test]
fn la_pre_intro_libere_les_lettres_peu_a_peu() {
    let mut app = app();
    drive(&mut app, 5);
    app.world_mut().resource_mut::<DjSignIntro>().0 = Some(0.0);
    drive(&mut app, 120);
    let director = &app.world().resource::<DjSignRes>().0;
    assert!(director.visible(), "la pré-intro réclame l'enseigne");
    assert!(director.sign().lit() < 1e-6, "aucune lettre n'est encore libérée");
    app.world_mut().resource_mut::<DjSignIntro>().0 = Some(1.0);
    drive(&mut app, 60 * 8);
    assert!(app.world().resource::<DjSignRes>().0.sign().lit() > 0.3);
}

#[test]
fn le_reglage_dj_sign_de_l_evenement_coupe_l_enseigne() {
    let mut app = app();
    app.world_mut().resource_mut::<ShowSheet>().dj_sign = false;
    drive(&mut app, 5);
    next_dj(&mut app);
    drive(&mut app, 30);
    assert!(!app.world().resource::<DjSignRes>().0.visible());
    assert!(
        app.world()
            .resource::<Subjects>()
            .of_kind(SubjectKind::DjSign)
            .next()
            .is_none()
    );
}
