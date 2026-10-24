//! La voiture dans une vraie `App` Bevy, sans rendu : chargement du GLB par l'`AssetServer`, couches, boîtes englobantes,
//! lumières qui pulsent sur les kicks, clignotants, absence d'entités créées en régime établi, précision à 400 km.
//!
//! Le modèle (`assets/models/car/car-low.glb`, la variante légère) est décodé pour de vrai : meshopt, WebP, tout.

#![allow(clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::RenderLayers;
use bevy::color::LinearRgba;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::{AbsolutePosition, AudioFeatures, CorePlugin, Layer, aabb_encloses_mesh, mesh_aabb};
use cf_world_neon::car::effects::{CarSmoke, CarTrail};
use cf_world_neon::car::{Car, CarControls, CarPaintMaterial, CarPlugin, CarRig, CarState, ModelQuality};
use cf_world_neon::{ChasePlugin, NeonWorldPlugin, RunnerConfig};

const FRAME: Duration = Duration::from_micros(16_667);

fn app(start_z_m: f64, speed_mps: f64) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            // Les tests tournent dans `crates/cf-world-neon` : les assets sont à la racine du dépôt.
            file_path: "../../assets".into(),
            ..default()
        },
        TransformPlugin,
        CorePlugin,
        NeonWorldPlugin::embedded(),
        ChasePlugin {
            config: RunnerConfig { start_z_m, speed_mps },
        },
        CarPlugin {
            quality: ModelQuality::Low,
        },
    ))
    .init_asset::<Image>()
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>()
    .insert_resource(AudioFeatures(cf_analysis::AudioFeatures::NEUTRAL))
    .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    app
}

/// Fait tourner l'application jusqu'à ce que la voiture soit posée (le décodage se fait dans un fil à part).
fn built(start_z_m: f64, speed_mps: f64) -> App {
    let mut app = app(start_z_m, speed_mps);
    for _ in 0..4000 {
        app.update();
        let mut rigs = app.world_mut().query_filtered::<Entity, With<CarRig>>();
        if rigs.iter(app.world()).next().is_some() {
            // Quelques images de plus : traînée, lumières, un régime établi.
            for _ in 0..5 {
                app.update();
            }
            return app;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("la voiture n'a pas été construite : le modèle ne s'est pas chargé");
}

fn car(app: &mut App) -> Entity {
    let mut query = app.world_mut().query_filtered::<Entity, With<Car>>();
    query.single(app.world()).expect("une seule voiture")
}

/// Descendants d'une entité (elle comprise).
fn tree(app: &App, root: Entity) -> Vec<Entity> {
    let mut out = vec![root];
    let mut i = 0;
    while i < out.len() {
        if let Some(children) = app.world().get::<Children>(out[i]) {
            out.extend(children.iter());
        }
        i += 1;
    }
    out
}

#[test]
fn la_voiture_remplace_le_pave_provisoire_et_pose_ses_pneus_au_sol() {
    let mut app = built(0.0, 0.0);
    let root = car(&mut app);
    assert!(app.world().get::<Mesh3d>(root).is_none(), "le pavé est retiré");
    let position = app.world().get::<AbsolutePosition>(root).expect("position").0;
    assert!(position.y.abs() < 1e-12, "pneus à y = 0 : {}", position.y);
    // Un seul point suivi : la voiture, pas deux.
    let mut focus = app.world_mut().query_filtered::<Entity, With<cf_render::StreamFocus>>();
    assert_eq!(focus.iter(app.world()).count(), 1);
}

#[test]
fn tout_ce_que_la_voiture_dessine_est_dans_la_couche_monde_sans_no_frustum_culling() {
    let mut app = built(0.0, 40.0);
    let world_layers = Layer::World.render_layers();
    let root = car(&mut app);
    let mut checked = 0;
    for entity in tree(&app, root) {
        let has_mesh = app.world().get::<Mesh3d>(entity).is_some();
        let is_light = app.world().get::<SpotLight>(entity).is_some() || app.world().get::<PointLight>(entity).is_some();
        if has_mesh || is_light {
            let layers = app.world().get::<RenderLayers>(entity).expect("RenderLayers");
            assert_eq!(*layers, world_layers, "{:?}", app.world().get::<Name>(entity));
            checked += 1;
        }
        assert!(
            app.world().get::<bevy::camera::visibility::NoFrustumCulling>(entity).is_none(),
            "pas de NoFrustumCulling : {:?}",
            app.world().get::<Name>(entity)
        );
    }
    // 62 primitives du GLB (moins la pluie sur le pare-brise), ~20 pièces de lumière, 2 projecteurs, 2 lumières néon.
    assert!(checked > 60, "{checked} entités dessinées ou éclairantes");
    // Traînée et fumée : roots à part, dans la couche monde aussi.
    for entity in app.world_mut().query_filtered::<Entity, Or<(With<CarTrail>, With<CarSmoke>)>>().iter(app.world()).collect::<Vec<_>>() {
        assert_eq!(*app.world().get::<RenderLayers>(entity).expect("couche"), world_layers);
    }
    // Aucune lumière n'est restée à la couche 0 (le fond) : la caméra du monde ne les verrait pas.
    let mut bare = app
        .world_mut()
        .query_filtered::<Entity, (Or<(With<SpotLight>, With<PointLight>)>, Without<RenderLayers>)>();
    assert_eq!(bare.iter(app.world()).count(), 0);
}

#[test]
fn la_traine_et_la_fumee_ont_un_aabb_qui_enveloppe_leur_maillage() {
    let mut app = built(-400_000.0, 40.0);
    for _ in 0..60 {
        app.update();
    }
    let mut lines = app.world_mut().query::<(&Mesh3d, &Aabb, &CarTrail)>();
    let (mesh, aabb, _) = lines.single(app.world()).expect("traînée");
    let meshes = app.world().resource::<Assets<Mesh>>();
    let mesh = meshes.get(&mesh.0).expect("maillage");
    let exact = mesh_aabb(mesh).expect("positions");
    // Les sommets restants (non posés) sont repliés sur le plus ancien : l'Aabb pose enveloppe tout.
    assert!(aabb_encloses_mesh(aabb, mesh, 1e-3), "{aabb:?} / {exact:?}");
    // À 400 km, la bande est rendue près de l'origine flottante (précision du millimètre), pas à 400 000 m.
    assert!(exact.center.length() < 16_000.0, "rendue à {} m de l'origine", exact.center.length());
}

fn clone_material<A: Asset + Clone>(app: &App, handle: &Handle<A>) -> A {
    app.world().resource::<Assets<A>>().get(handle).expect("matériau").clone()
}

/// Critère de la tâche : le glow pulse sur les kicks. Au pic du kick, les halos des phares, les volumes de lumière, la
/// lueur sous la caisse, le logo arrière et le liseré de la peinture sont plus forts qu'au creux.
#[test]
fn le_glow_pulse_sur_les_kicks() {
    let mut app = built(0.0, 40.0);
    let root = car(&mut app);
    let rig_of = |app: &App| {
        let rig = app.world().get::<CarRig>(root).expect("rig");
        (
            rig.lights.heads[0].material.clone(),
            rig.lights.beam.clone(),
            rig.lights.under.material.clone(),
            rig.materials.logo.clone().expect("logo").handle,
            rig.materials.paint.clone().expect("peinture"),
        )
    };
    let (head, beam, under, logo, paint) = rig_of(&app);
    let sample = |app: &mut App, pulse: f32, bass: f32| {
        {
            let mut features = app.world_mut().resource_mut::<AudioFeatures>();
            features.0.live = true;
            features.0.pulse = pulse;
            features.0.bass = bass;
        }
        // Quelques images : le logo a une attaque instantanée mais un relâchement de 0,25 s.
        for _ in 0..40 {
            app.update();
        }
        let sum = |c: Vec4| c.x + c.y + c.z;
        (
            sum(clone_material(app, &head).params.color),
            clone_material(app, &beam).params.color.w,
            sum(clone_material(app, &under).params.color),
            {
                let m = clone_material(app, &logo);
                m.emissive.red + m.emissive.green + m.emissive.blue
            },
            clone_material::<CarPaintMaterial>(app, &paint).extension.params.finish.w,
        )
    };
    let calm = sample(&mut app, 0.0, 0.0);
    let kick = sample(&mut app, 1.0, 0.9);
    assert!(kick.0 > calm.0 * 1.2, "halo des phares : {} → {}", calm.0, kick.0);
    assert!(kick.1 > calm.1 * 1.2, "volume des phares : {} → {}", calm.1, kick.1);
    assert!(kick.2 > calm.2 * 1.5, "lueur sous la caisse : {} → {}", calm.2, kick.2);
    assert!(kick.3 > calm.3 * 2.0, "logo arrière : {} → {}", calm.3, kick.3);
    assert!(kick.4 > calm.4 * 2.0, "liseré de la peinture : {} → {}", calm.4, kick.4);
}

#[test]
fn le_freinage_allume_les_feux_stop_et_la_route() {
    let mut app = built(0.0, 40.0);
    let root = car(&mut app);
    let brake = |app: &App| {
        let rig = app.world().get::<CarRig>(root).expect("rig");
        let material = rig.materials.brake.clone().expect("feux stop");
        let m = clone_material(app, &material.handle);
        m.emissive.red
    };
    let rolling = brake(&app);
    app.world_mut().resource_mut::<CarControls>().brake = 1.0;
    for _ in 0..3 {
        app.update();
    }
    assert!(brake(&app) > rolling * 4.0, "{rolling} → {}", brake(&app));
    // La route reflète les feux stop (`RoadLook.brake`).
    assert!((app.world().resource::<cf_world_neon::RoadLook>().brake - 1.0).abs() < 1e-6);
}

#[test]
fn le_volant_allume_le_clignotant_de_son_cote_et_jamais_les_deux() {
    let mut app = built(0.0, 40.0);
    let root = car(&mut app);
    app.world_mut().resource_mut::<CarControls>().steer = 0.8;
    // Le premier battement est immédiat ; on regarde pendant deux secondes.
    let mut seen = [false; 2];
    for _ in 0..120 {
        app.update();
        let rig = app.world().get::<CarRig>(root).expect("rig");
        let mut lit = [false, false];
        for signal in &rig.signals {
            if app.world().get::<Visibility>(signal.halo_entity) != Some(&Visibility::Hidden) {
                lit[usize::from(signal.side == cf_world_neon::car::model::Side::Right)] = true;
            }
        }
        assert!(!(lit[0] && lit[1]), "jamais les deux côtés à la fois : {:?}", rig.signals.iter().map(|s| (s.side, app.world().get::<Visibility>(s.halo_entity).copied(), app.world().get::<Name>(s.halo_entity).map(ToString::to_string))).collect::<Vec<_>>());
        seen[0] |= lit[0];
        seen[1] |= lit[1];
    }
    assert_eq!(seen, [false, true], "seul le côté droit a clignoté");
    // Il bat sous 3 Hz : le volant redressé, il s'éteint après 0,8 s.
    app.world_mut().resource_mut::<CarControls>().steer = 0.0;
    for _ in 0..90 {
        app.update();
    }
    let state = app.world().get::<CarState>(root).expect("état");
    assert_eq!(state.blinker.side, 0);
}

#[test]
fn les_roues_tournent_et_la_caisse_plonge_au_freinage() {
    let mut app = built(0.0, 40.0);
    let root = car(&mut app);
    let spin = app.world().get::<CarRig>(root).expect("rig").spin[0];
    let before = app.world().get::<Transform>(spin).expect("roue").rotation;
    app.update();
    app.update();
    let after = app.world().get::<Transform>(spin).expect("roue").rotation;
    assert!(before.angle_between(after) > 1e-3, "la roue a tourné");
    let state = app.world().get::<CarState>(root).expect("état");
    assert!((state.speed_mps - 40.0).abs() < 0.5, "vitesse mesurée : {}", state.speed_mps);
}

#[test]
fn aucune_entite_creee_en_regime_etabli_et_la_vitesse_se_mesure() {
    let mut app = built(0.0, 60.0);
    for _ in 0..30 {
        app.update();
    }
    let entities = app.world().entities().len();
    for _ in 0..600 {
        app.update();
    }
    assert_eq!(
        app.world().entities().len(),
        entities,
        "aucune entité créée ou détruite en 10 s de route"
    );
}

#[test]
fn la_lueur_de_la_voiture_suit_le_faisceau_traverse() {
    let mut app = built(0.0, 40.0);
    let violet = app.world().resource::<cf_world_neon::RoadLook>().glow;
    let mint = LinearRgba::rgb(0.0, 0.78, 1.0);
    app.world_mut().resource_mut::<CarControls>().lit = Some(mint);
    // 0,5 s de fondu.
    for _ in 0..45 {
        app.update();
    }
    let glow = app.world().resource::<cf_world_neon::RoadLook>().glow;
    assert!((glow.blue - mint.blue).abs() < 1e-3 && glow.green > violet.green + 0.3, "{violet:?} → {glow:?}");
}

#[test]
fn la_voiture_n_est_pas_necessaire_au_monde_sans_modele() {
    // Sans fichier à charger, rien ne plante : le point suivi reste le pavé provisoire.
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: "inexistant".into(),
            ..default()
        },
        TransformPlugin,
        CorePlugin,
        NeonWorldPlugin::embedded(),
        ChasePlugin::default(),
        CarPlugin::default(),
    ))
    .init_asset::<Image>()
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    for _ in 0..30 {
        app.update();
    }
    let mut runner = app.world_mut().query_filtered::<&Mesh3d, With<cf_world_neon::Runner>>();
    assert_eq!(runner.iter(app.world()).count(), 1, "le pavé reste");
}
