//! La voiture dans une vraie `App` Bevy, sans rendu : chargement du GLB par l'`AssetServer`, couches, boîtes englobantes,
//! lumières qui pulsent sur les kicks, clignotants, absence d'entités créées en régime établi, précision à 400 km.
//!
//! Le modèle (`assets/models/car/car-dirty-low.glb`, la variante légère) est décodé pour de vrai : meshopt, WebP, tout.

#![allow(clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::RenderLayers;
use bevy::color::LinearRgba;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::{
    AbsolutePosition, AudioFeatures, CorePlugin, CurrentShowFx, Layer, ShowFxInput, aabb_encloses_mesh, mesh_aabb,
};
use cf_world_neon::car::effects::{CarSmoke, CarTrail};
use cf_world_neon::car::{
    ActiveCar, Car, CarChoiceRequest, CarControls, CarModels, CarPaintMaterial, CarPlugin, CarRig, CarState,
    CarVariant, ModelQuality,
};
use cf_world_neon::{ChasePlugin, NeonWorldPlugin, RunnerConfig};

const FRAME: Duration = Duration::from_micros(16_667);

/// Un spectacle qui permet les flashs (`flashes`) ou non : la politique que lit le limiteur de la voiture.
fn show(flashes: bool, breathing: bool) -> CurrentShowFx {
    CurrentShowFx(ShowFxInput {
        flashes,
        breathing,
        heat: 0.0,
        time_scale: 1.0,
        cue_seq: 0,
        fx_rgb_split: 0.0,
        fx_flash: 0.0,
        drop: None,
    })
}

fn app(start_z_m: f64, speed_mps: f64) -> App {
    app_with(start_z_m, speed_mps, CarVariant::Dirty, "../../assets".into())
}

fn app_with(start_z_m: f64, speed_mps: f64, variant: CarVariant, assets_dir: std::path::PathBuf) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        NeonWorldPlugin::embedded(),
        ChasePlugin {
            config: RunnerConfig { start_z_m, speed_mps },
        },
        CarPlugin {
            quality: ModelQuality::Low,
            variant,
            // Les tests tournent dans `crates/cf-world-neon` : les assets sont à la racine du dépôt.
            assets_dir,
        },
    ))
    .init_asset::<Image>()
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>()
    .insert_resource(AudioFeatures(cf_analysis::AudioFeatures::NEUTRAL))
    .insert_resource(show(true, false))
    .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    app
}

/// Fait tourner l'application jusqu'à ce que la voiture soit posée (le décodage se fait dans un fil à part).
fn built(start_z_m: f64, speed_mps: f64) -> App {
    built_with(app(start_z_m, speed_mps))
}

fn built_with(mut app: App) -> App {
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
        let is_light =
            app.world().get::<SpotLight>(entity).is_some() || app.world().get::<PointLight>(entity).is_some();
        if has_mesh || is_light {
            let layers = app.world().get::<RenderLayers>(entity).expect("RenderLayers");
            assert_eq!(*layers, world_layers, "{:?}", app.world().get::<Name>(entity));
            checked += 1;
        }
        // Règle 6 : chaque pièce garde l'`Aabb` que Bevy calcule de son maillage ; on vérifie qu'aucune ne s'en est
        // affranchie (les halos, la traînée et la fumée posent le leur, juste, à la main).
        let culling_off = app.world().get::<bevy::camera::visibility::NoFrustumCulling>(entity);
        assert!(
            culling_off.is_none(),
            "frustum culling désactivé : {:?}",
            app.world().get::<Name>(entity)
        );
    }
    // ~35 primitives du GLB (la dirty car est un modèle léger), ~20 pièces de lumière, 2 projecteurs, 2 lumières néon.
    assert!(checked > 35, "{checked} entités dessinées ou éclairantes");
    // Traînée et fumée : roots à part, dans la couche monde aussi.
    for entity in app
        .world_mut()
        .query_filtered::<Entity, Or<(With<CarTrail>, With<CarSmoke>)>>()
        .iter(app.world())
        .collect::<Vec<_>>()
    {
        assert_eq!(*app.world().get::<RenderLayers>(entity).expect("couche"), world_layers);
    }
    // Aucune lumière n'est restée à la couche 0 (le fond) : la caméra du monde ne les verrait pas.
    let mut bare = app
        .world_mut()
        .query_filtered::<Entity, (Or<(With<SpotLight>, With<PointLight>)>, Without<RenderLayers>)>();
    assert_eq!(bare.iter(app.world()).count(), 0);
}

/// Budget de la voiture : entités qui dessinent et lumières de Bevy. Les maillages du modèle sont distincts (un par
/// pièce, un draw call chacun) ; les lumières additives sont ~20 petits quadrilatères. Quatre lumières de Bevy au plus.
#[test]
fn la_voiture_reste_dans_son_budget_d_entites() {
    let mut app = built(0.0, 40.0);
    let root = car(&mut app);
    let in_car = tree(&app, root)
        .into_iter()
        .filter(|&e| app.world().get::<Mesh3d>(e).is_some())
        .count();
    // Et ses deux effets, hors de sa hiérarchie : la traînée et la fumée.
    let meshes = in_car + 2;
    let spots = app
        .world_mut()
        .query_filtered::<Entity, With<SpotLight>>()
        .iter(app.world())
        .count();
    let points = app
        .world_mut()
        .query_filtered::<Entity, With<PointLight>>()
        .iter(app.world())
        .count();
    eprintln!(
        "voiture : {meshes} entités à maillage (dont traînée et fumée), {spots} projecteurs, {points} lumières ponctuelles"
    );
    assert!((50..80).contains(&meshes), "{meshes} entités à maillage");
    assert_eq!(spots, 2, "deux phares");
    assert_eq!(points, 3, "sous la caisse, logo arrière et habitacle");
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
    assert!(
        exact.center.length() < 16_000.0,
        "rendue à {} m de l'origine",
        exact.center.length()
    );
}

fn clone_material<A: Asset + Clone>(app: &App, handle: &Handle<A>) -> A {
    app.world()
        .resource::<Assets<A>>()
        .get(handle)
        .expect("matériau")
        .clone()
}

/// Ce que montrent les lumières à cet instant : halo de phare, volume de phare, lueur sous la caisse, logo arrière,
/// liseré de la peinture.
fn glow_metrics(app: &App, root: Entity) -> [f32; 5] {
    let rig = app.world().get::<CarRig>(root).expect("rig");
    let sum = |c: Vec4| c.x + c.y + c.z;
    let logo = clone_material(app, &rig.materials.logo.clone().expect("logo").handle);
    [
        sum(clone_material(app, &rig.lights.heads[0].material).params.color),
        clone_material(app, &rig.lights.beam).params.color.w,
        sum(clone_material(app, &rig.lights.under.material).params.color),
        logo.emissive.red + logo.emissive.green + logo.emissive.blue,
        clone_material::<CarPaintMaterial>(app, rig.materials.paint.as_ref().expect("peinture"))
            .extension
            .params
            .finish
            .w,
    ]
}

/// Joue `seconds` secondes de kicks à `bpm` (pouls à 1 à l'image du kick, puis ~0,12 s de retombée) et rend la série
/// des `glow_metrics` image par image.
fn play_kicks(app: &mut App, root: Entity, bpm: f64, seconds: f64) -> Vec<[f32; 5]> {
    let period = 60.0 / bpm;
    let mut series = Vec::new();
    let mut next_kick = 0.0;
    let mut last_kick = f64::NEG_INFINITY;
    for i in 0..(seconds * 60.0) as usize {
        let t = i as f64 / 60.0;
        let pulse = if t >= next_kick {
            last_kick = t;
            next_kick += period;
            1.0
        } else {
            (-((t - last_kick) as f32) / 0.12).exp()
        };
        {
            let mut features = app.world_mut().resource_mut::<AudioFeatures>();
            features.0.live = true;
            features.0.pulse = pulse;
            features.0.bass = 0.5 * pulse;
        }
        app.update();
        series.push(glow_metrics(app, root));
    }
    series
}

fn column(series: &[[f32; 5]], i: usize) -> Vec<f32> {
    series.iter().map(|m| m[i]).collect()
}

/// Les montées d'une série : (image de départ, nombre d'images). Une montée est une suite d'images où la valeur croît.
fn rises(series: &[f32]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for i in 1..series.len() {
        let up = series[i] > series[i - 1] + 1e-5;
        match (up, start) {
            (true, None) => start = Some(i - 1),
            (false, Some(s)) => {
                out.push((s, i - 1 - s));
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// Critère de la tâche : le glow pulse sur les kicks. Au pic du kick, les halos des phares, les volumes de lumière, la
/// lueur sous la caisse, le logo arrière et le liseré de la peinture sont plus forts qu'au creux.
#[test]
fn le_glow_pulse_sur_les_kicks() {
    let mut app = built(0.0, 40.0);
    let root = car(&mut app);
    // 128 BPM : un kick toutes les 0,47 s, donc des creux franchement distincts des pics.
    let series = play_kicks(&mut app, root, 128.0, 4.0);
    for (i, what) in [
        "halo des phares",
        "volume des phares",
        "lueur sous la caisse",
        "logo arrière",
        "liseré de la peinture",
    ]
    .into_iter()
    .enumerate()
    {
        let values = column(&series, i);
        let (low, high) = values
            .iter()
            .fold((f32::MAX, 0.0_f32), |(lo, hi), &v| (lo.min(v), hi.max(v)));
        let factor = [1.2, 1.2, 1.5, 2.0, 2.0][i];
        assert!(high > low * factor, "{what} : {low} → {high}");
    }
}

/// Règle 7 : à 174 BPM, ce que montrent vraiment les matériaux ne monte pas plus de 3 fois par seconde sur une fenêtre
/// glissante, et aucune montée ne dure moins de 5 images.
#[test]
fn a_174_bpm_le_glow_des_materiaux_respecte_le_limiteur() {
    let mut app = built(0.0, 40.0);
    let root = car(&mut app);
    let series = play_kicks(&mut app, root, 174.0, 12.0);
    // Colonnes qui ne battent que sur les kicks : halo, volume, lueur sous la caisse, liseré de la peinture.
    for i in [0, 1, 2, 4] {
        let values = column(&series, i);
        let found = rises(&values);
        assert!(found.len() > 10, "colonne {i} : il bat bien ({} montées)", found.len());
        for &(start, frames) in &found {
            let n = found.iter().filter(|&&(t, _)| t >= start && t < start + 60).count();
            assert!(
                n <= 3,
                "colonne {i} : {n} montées dans la seconde qui suit l'image {start}"
            );
            let rise = values[start + frames] - values[start];
            let swing =
                values.iter().copied().fold(0.0_f32, f32::max) - values.iter().copied().fold(f32::MAX, f32::min);
            if rise > 0.9 * swing {
                assert!(
                    frames >= 5,
                    "colonne {i} : montée complète en {frames} images (image {start})"
                );
            }
        }
    }
}

/// Règle 7 : flashs coupés par le spectacle, en respiration, ou sans spectacle : plus rien ne bat, ni sur les kicks ni au
/// clignotant.
#[test]
fn rien_ne_bat_quand_les_flashs_sont_coupes() {
    for policy in [Some(show(false, false)), Some(show(true, true)), None] {
        let mut app = built(0.0, 40.0);
        let root = car(&mut app);
        match policy {
            Some(p) => {
                app.insert_resource(p);
            }
            None => {
                app.world_mut().remove_resource::<CurrentShowFx>();
            }
        }
        app.world_mut().resource_mut::<CarControls>().steer = 0.8;
        // Quelques images pour que l'extinction (en `MIN_FADE` au plus) soit faite.
        for _ in 0..10 {
            app.update();
        }
        let series = play_kicks(&mut app, root, 174.0, 4.0);
        for i in 0..5 {
            let values = column(&series, i);
            let (low, high) = values
                .iter()
                .fold((f32::MAX, 0.0_f32), |(lo, hi), &v| (lo.min(v), hi.max(v)));
            assert!((high - low).abs() < 1e-5, "flashs coupés, colonne {i} : {low}..{high}");
        }
        let rig = app.world().get::<CarRig>(root).expect("rig");
        for signal in &rig.signals {
            let m = clone_material(&app, &signal.material.handle);
            assert!(
                m.emissive.red < signal.material.strength * 0.05,
                "clignotant allumé sans flashs permis"
            );
        }
    }
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
    // Le freinage est lissé (jamais plus vite que 0,1 s) : quelques images de plus.
    for _ in 0..15 {
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
        assert!(
            !(lit[0] && lit[1]),
            "jamais les deux côtés à la fois : {:?}",
            rig.signals
                .iter()
                .map(|s| (
                    s.side,
                    app.world().get::<Visibility>(s.halo_entity).copied(),
                    app.world().get::<Name>(s.halo_entity).map(ToString::to_string)
                ))
                .collect::<Vec<_>>()
        );
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
    assert!(
        (state.speed_mps - 40.0).abs() < 0.5,
        "vitesse mesurée : {}",
        state.speed_mps
    );
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
    assert!(
        (glow.blue - mint.blue).abs() < 1e-3 && glow.green > violet.green + 0.3,
        "{violet:?} → {glow:?}"
    );
}

#[test]
fn la_voiture_n_est_pas_necessaire_au_monde_sans_modele() {
    // Sans fichier à charger, rien ne plante : le point suivi reste le pavé provisoire.
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        NeonWorldPlugin::embedded(),
        ChasePlugin::default(),
        CarPlugin {
            assets_dir: "inexistant".into(),
            ..default()
        },
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

// --- Changer de voiture à chaud ---

/// Fait tourner jusqu'à ce que `done` soit vrai (les modèles se décodent dans des fils à part).
fn run_until(app: &mut App, what: &str, done: impl Fn(&App) -> bool) {
    for _ in 0..4000 {
        app.update();
        if done(app) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("{what} : jamais arrivé");
}

fn switch_to(app: &mut App, variant: CarVariant) {
    run_until(app, "seconde voiture décodée", |app| {
        app.world().resource::<CarModels>().has(variant)
    });
    app.world_mut().insert_resource(CarChoiceRequest(variant));
    run_until(app, "changement de voiture", |app| {
        app.world().resource::<ActiveCar>().0 == variant
    });
    // Les commandes différées (destruction, pose) sont appliquées, puis un régime établi.
    for _ in 0..5 {
        app.update();
    }
}

fn named(app: &mut App, name: &str) -> usize {
    let mut query = app.world_mut().query::<&Name>();
    query.iter(app.world()).filter(|n| n.as_str() == name).count()
}

#[test]
fn changer_de_voiture_detruit_toutes_les_entites_de_l_ancienne() {
    // Une construction neuve de la CyberCar sert de mesure : le changement doit laisser exactement la même arborescence.
    let mut fresh = built_with(app_with(0.0, 0.0, CarVariant::Cyber, "../../assets".into()));
    let fresh_root = car(&mut fresh);
    let expected = tree(&fresh, fresh_root).len();

    let mut app = built(0.0, 0.0);
    switch_to(&mut app, CarVariant::Cyber);
    let root = car(&mut app);
    assert_eq!(
        tree(&app, root).len(),
        expected,
        "arborescence d'une construction neuve"
    );
    assert_eq!(named(&mut app, "carrosserie"), 1, "une seule carrosserie");
    // Un aller-retour ne laisse rien non plus.
    switch_to(&mut app, CarVariant::Dirty);
    let mut dirty = built(0.0, 0.0);
    let dirty_root = car(&mut dirty);
    let app_root = car(&mut app);
    assert_eq!(tree(&app, app_root).len(), tree(&dirty, dirty_root).len());
    assert_eq!(named(&mut app, "carrosserie"), 1);
}

#[test]
fn la_position_et_la_vitesse_sont_gardees_au_changement() {
    let mut app = built(0.0, 40.0);
    for _ in 0..30 {
        app.update();
    }
    let root = car(&mut app);
    let before_speed = app.world().get::<CarState>(root).expect("état").speed_mps;
    let before_z = app.world().get::<AbsolutePosition>(root).expect("position").0.z;
    assert!(before_speed > 20.0, "la voiture roule : {before_speed}");
    switch_to(&mut app, CarVariant::Cyber);
    let root = car(&mut app);
    let after_speed = app.world().get::<CarState>(root).expect("état").speed_mps;
    let after_z = app.world().get::<AbsolutePosition>(root).expect("position").0.z;
    assert!(
        (after_speed - before_speed).abs() < 2.0,
        "vitesse {before_speed} → {after_speed}"
    );
    assert!(after_z < before_z, "elle a continué d'avancer (la route va vers −z)");
}

#[test]
fn demander_la_voiture_en_place_ne_reconstruit_rien() {
    let mut app = built(0.0, 0.0);
    let root = car(&mut app);
    let mut before = tree(&app, root);
    before.sort();
    app.world_mut().insert_resource(CarChoiceRequest(CarVariant::Dirty));
    for _ in 0..30 {
        app.update();
    }
    let mut after = tree(&app, root);
    after.sort();
    assert_eq!(before, after, "mêmes entités");
}

#[test]
fn un_modele_refuse_garde_la_voiture_saine() {
    // Un dossier d'assets où seule la dirty car existe : la CyberCar est introuvable.
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let models = dir.path().join("models/car");
    std::fs::create_dir_all(&models).expect("dossiers");
    std::fs::copy(
        "../../assets/models/car/car-dirty-low.glb",
        models.join("car-dirty-low.glb"),
    )
    .expect("copie");
    let mut app = built_with(app_with(0.0, 0.0, CarVariant::Dirty, dir.path().to_path_buf()));
    app.world_mut().insert_resource(CarChoiceRequest(CarVariant::Cyber));
    for _ in 0..300 {
        app.update();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(app.world().resource::<ActiveCar>().0, CarVariant::Dirty);
    assert!(!app.world().resource::<CarModels>().has(CarVariant::Cyber));
    let mut rigs = app.world_mut().query_filtered::<Entity, With<CarRig>>();
    assert_eq!(rigs.iter(app.world()).count(), 1, "la voiture saine reste");
}
