//! Le conducteur de la voiture (X-10) dans une vraie `App` Bevy, sans rendu : la dirty car porte le manul sur sa caisse,
//! la `CyberCar` n'en a pas, et un changement de voiture ne laisse ni doublon ni conducteur orphelin.

#![allow(clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::{AudioFeatures, CorePlugin, CurrentShowFx, ShowFxInput};
use cf_world_neon::car::{ActiveCar, CarChoiceRequest, CarPlugin, CarRig, CarVariant, ModelQuality};
use cf_world_neon::rider::driver::{
    Driver, DriverRig, GRIP_SETBACK_M, Gearshift, LEVER_PIVOT, WHEEL_AXIS, WHEEL_CENTRE, knob_position, lever_rotation,
};
use cf_world_neon::rider::gearbox::{Gear, SHIFT_S, Shift};
use cf_world_neon::{ChasePlugin, NeonWorldPlugin, RunnerConfig};

const FRAME: Duration = Duration::from_micros(16_667);

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        NeonWorldPlugin::embedded(),
        ChasePlugin {
            config: RunnerConfig {
                start_z_m: 0.0,
                speed_mps: 0.0,
            },
        },
        CarPlugin {
            quality: ModelQuality::Low,
            variant: CarVariant::Dirty,
            // Les tests tournent dans `crates/cf-world-neon` : les assets sont à la racine du dépôt.
            assets_dir: "../../assets".into(),
        },
    ))
    .init_asset::<Image>()
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>()
    .insert_resource(AudioFeatures(cf_analysis::AudioFeatures::NEUTRAL))
    .insert_resource(CurrentShowFx(ShowFxInput {
        flashes: true,
        breathing: false,
        heat: 0.0,
        time_scale: 1.0,
        cue_seq: 0,
        fx_rgb_split: 0.0,
        fx_flash: 0.0,
        drop: None,
    }))
    .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    app
}

/// Fait tourner l'application jusqu'à ce que `done` soit vrai (le décodage des modèles se fait dans un fil à part).
fn run_until(app: &mut App, what: &str, done: impl Fn(&mut App) -> bool) {
    for _ in 0..4000 {
        app.update();
        if done(app) {
            for _ in 0..5 {
                app.update();
            }
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("{what} : jamais atteint");
}

fn drivers(app: &mut App) -> Vec<Entity> {
    app.world_mut()
        .query_filtered::<Entity, With<Driver>>()
        .iter(app.world())
        .collect()
}

/// Vrai si `entity` descend de `ancestor`.
fn descends_from(app: &App, entity: Entity, ancestor: Entity) -> bool {
    let mut current = entity;
    while let Some(parent) = app.world().get::<ChildOf>(current) {
        current = parent.parent();
        if current == ancestor {
            return true;
        }
    }
    false
}

fn chassis(app: &mut App) -> Entity {
    app.world_mut()
        .query::<&CarRig>()
        .single(app.world())
        .expect("une voiture")
        .chassis
}

#[test]
fn la_dirty_car_porte_son_conducteur_sur_la_caisse() {
    let mut app = app();
    run_until(&mut app, "voiture et conducteur posés", |app| !drivers(app).is_empty());
    let found = drivers(&mut app);
    assert_eq!(found.len(), 1, "un seul conducteur");
    let chassis = chassis(&mut app);
    assert!(
        descends_from(&app, found[0], chassis),
        "le conducteur suit la caisse (suspension)"
    );
    // Il a un corps : des maillages dans son arbre.
    let mut meshes = app.world_mut().query::<(Entity, &Mesh3d)>();
    let count = meshes
        .iter(app.world())
        .filter(|(entity, _)| descends_from(&app, *entity, found[0]))
        .count();
    assert!(count > 0, "le conducteur a des maillages");
}

#[test]
fn la_cybercar_n_a_pas_de_conducteur_et_la_dirty_car_le_retrouve() {
    let mut app = app();
    run_until(&mut app, "voiture et conducteur posés", |app| !drivers(app).is_empty());
    // Attend que la seconde voiture soit décodée, puis passe à la CyberCar.
    app.world_mut().insert_resource(CarChoiceRequest(CarVariant::Cyber));
    run_until(&mut app, "CyberCar posée", |app| {
        app.world().resource::<ActiveCar>().0 == CarVariant::Cyber
    });
    assert!(
        drivers(&mut app).is_empty(),
        "pas de conducteur dans la CyberCar : la pose est celle de la dirty car"
    );

    app.world_mut().insert_resource(CarChoiceRequest(CarVariant::Dirty));
    run_until(&mut app, "dirty car reposée avec son conducteur", |app| {
        app.world().resource::<ActiveCar>().0 == CarVariant::Dirty && !drivers(app).is_empty()
    });
    assert_eq!(drivers(&mut app).len(), 1, "un seul conducteur après le retour");
    let found = drivers(&mut app)[0];
    let chassis = chassis(&mut app);
    assert!(descends_from(&app, found, chassis), "posé sur la nouvelle caisse");
}

/// Boîte englobante d'un modèle décodé, dans son repère (nœuds compris).
fn model_bounds(model: &cf_world_neon::car::CarModelAsset) -> (Vec3, Vec3) {
    use bevy::mesh::VertexAttributeValues;
    let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let mut stack: Vec<(usize, Mat4)> = model.roots.iter().map(|&r| (r, Mat4::IDENTITY)).collect();
    while let Some((index, parent)) = stack.pop() {
        let node = &model.nodes[index];
        let world = parent * node.transform.to_matrix();
        for &(mesh, _) in &node.pieces {
            let Some(VertexAttributeValues::Float32x3(positions)) =
                model.meshes[mesh].attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                continue;
            };
            for p in positions {
                let p = world.transform_point3(Vec3::from(*p));
                min = min.min(p);
                max = max.max(p);
            }
        }
        stack.extend(node.children.iter().map(|&c| (c, world)));
    }
    (min, max)
}

#[test]
fn le_conducteur_est_dans_la_dirty_car_du_cote_du_volant() {
    let read = |path: &str| {
        let bytes = std::fs::read(format!("../../assets/models/{path}")).expect("lecture");
        cf_world_neon::car::CarModelAsset::from_glb(&bytes).expect("décodage")
    };
    let (car_min, car_max) = model_bounds(&read("car/car-dirty-low.glb"));
    let (min, max) = model_bounds(&read("characters/driver-car.glb"));
    let margin = Vec3::splat(0.05);
    assert!(
        min.cmpge(car_min - margin).all() && max.cmple(car_max + margin).all(),
        "le conducteur sort de la voiture : {min} {max} dans {car_min} {car_max}"
    );
    // Volant à gauche : le manul est du côté +x, pas au milieu (le siège est à x = 0,49).
    let center = (min + max) / 2.0;
    assert!(center.x > 0.3 && center.x < 0.7, "centre du conducteur : {center}");
}

/// Le volant a un pivot dans la voiture et le conducteur a ses deux bras à pivots : sans eux, rien ne suit le volant.
#[test]
fn le_conducteur_a_ses_deux_bras_et_la_voiture_son_volant() {
    let mut app = app();
    run_until(&mut app, "voiture et conducteur posés", |app| !drivers(app).is_empty());
    let rig = app.world_mut().query::<&CarRig>().single(app.world()).expect("voiture");
    assert!(rig.steering.is_some(), "pivot du volant (steering_pivot)");
    let driver = app.world_mut().query::<&DriverRig>().single(app.world()).expect("bras");
    let (left, right) = (driver.0.left.expect("bras gauche"), driver.0.right.expect("bras droit"));
    for arm in [left, right] {
        assert!(
            arm.geometry.upper() > 0.2 && arm.geometry.upper() < 0.5,
            "bras : {:?}",
            arm.geometry
        );
        assert!(
            arm.geometry.lower() > 0.2 && arm.geometry.lower() < 0.5,
            "avant-bras : {:?}",
            arm.geometry
        );
        // La main est sur le volant : à un rayon de jante de son centre.
        let d = arm.geometry.wrist - WHEEL_CENTRE;
        let radial = (d - WHEEL_AXIS.normalize() * d.dot(WHEEL_AXIS.normalize())).length();
        assert!(radial > 0.1 && radial < 0.3, "poignet à {radial} m de l'axe du volant");
    }
}

/// Les constantes du centre et de l'axe du volant sont celles de la colonne dans le GLB de la voiture.
#[test]
fn l_axe_du_volant_est_celui_de_la_colonne_du_glb() {
    let bytes = std::fs::read("../../assets/models/car/car-dirty-low.glb").expect("lecture");
    let model = cf_world_neon::car::CarModelAsset::from_glb(&bytes).expect("décodage");
    let column = model
        .nodes
        .iter()
        .find(|n| n.name == "steering_column")
        .expect("colonne de direction");
    let t = column.transform;
    assert!(t.translation.distance(WHEEL_CENTRE) < 0.01, "{:?}", t.translation);
    let axis = t.rotation * Vec3::Y;
    assert!(axis.distance(WHEEL_AXIS.normalize()) < 0.01, "{axis:?}");
}

/// Le pivot du levier dans le GLB est celui de la constante : la grille et la main en dépendent.
#[test]
fn le_pivot_du_levier_est_celui_du_glb() {
    let bytes = std::fs::read("../../assets/models/car/car-dirty-low.glb").expect("lecture");
    let model = cf_world_neon::car::CarModelAsset::from_glb(&bytes).expect("décodage");
    let node = &model.nodes[model.node("pivot_gear_lever").expect("pivot_gear_lever")];
    assert!(
        node.transform.translation.distance(LEVER_PIVOT) < 1e-3,
        "{:?}",
        node.transform.translation
    );
}

/// Au changement de rapport, la main droite quitte le volant et atteint le levier (à la portée du bras près), puis revient.
#[test]
fn la_main_droite_va_au_levier_puis_revient_au_volant() {
    let mut app = app();
    run_until(&mut app, "voiture et conducteur posés", |app| !drivers(app).is_empty());
    let arm = app
        .world_mut()
        .query::<&DriverRig>()
        .single(app.world())
        .expect("bras")
        .0
        .right
        .expect("bras droit");
    let reach = arm.geometry.upper() + arm.geometry.lower();
    let mut closest = f32::MAX;
    let mut at_wheel_after = f32::MAX;
    for gear in [Gear::Forward(2), Gear::Forward(3)] {
        let shift = Shift::new(Gear::Forward(2), gear);
        for step in 0..=60 {
            let mut shift = shift;
            shift.elapsed_s = SHIFT_S * step as f32 / 60.0;
            let knob = knob_position(shift.knob());
            let grip = knob + (arm.geometry.shoulder - knob).normalize() * GRIP_SETBACK_M;
            let target = arm.geometry.wrist.lerp(grip, shift.hand_on_lever());
            let reached = arm.geometry.wrist_for(arm.geometry.solve(target));
            if (shift.hand_on_lever() - 1.0).abs() < 1e-3 {
                closest = closest.min(reached.distance(knob));
            }
            if step == 60 {
                at_wheel_after = at_wheel_after.min(reached.distance(arm.geometry.wrist));
            }
        }
    }
    // Le bras est trop court pour poser le poignet à 11 cm du pommeau : il reste, tendu, dans la direction du levier.
    assert!(
        closest < GRIP_SETBACK_M + 0.12,
        "poignet à {closest} m du pommeau (portée {reach})"
    );
    assert!(at_wheel_after < 1e-3, "la main n'est pas revenue : {at_wheel_after}");
}

/// Dans une vraie `App`, le levier suit le rapport : à vitesse de croisière, la sixième est engagée et le levier y repose.
#[test]
fn le_levier_de_la_voiture_suit_le_rapport_engage() {
    let mut app = app();
    run_until(&mut app, "voiture et conducteur posés", |app| !drivers(app).is_empty());
    run_until(&mut app, "rapport engagé", |app| {
        app.world_mut()
            .query::<&Gearshift>()
            .single(app.world())
            .is_ok_and(|g| g.gear.is_some())
    });
    for _ in 0..120 {
        app.update();
    }
    let gearshift = *app
        .world_mut()
        .query::<&Gearshift>()
        .single(app.world())
        .expect("boîte");
    let gear = gearshift.gear.expect("rapport");
    assert!(gearshift.shift.is_none(), "geste en cours : {gearshift:?}");
    let lever = app
        .world_mut()
        .query::<&CarRig>()
        .single(app.world())
        .expect("voiture")
        .gear_lever
        .expect("pivot_gear_lever");
    let rotation = app.world().get::<Transform>(lever).expect("levier").rotation;
    assert!(
        rotation.angle_between(lever_rotation(gear.gate())) < 1e-3,
        "{gear:?} {rotation:?}"
    );
}
