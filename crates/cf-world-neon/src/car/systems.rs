//! Les systèmes de la voiture : création à l'arrivée du modèle, mouvement de la caisse et des roues, lumière.

use bevy::asset::{Assets, Handle};
use bevy::color::{LinearRgba, Mix};
use bevy::light::{PointLight, SpotLight};
use bevy::math::{DVec3, Quat, Vec3, Vec4};
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures_lite::future};
use cf_render::warmup::StepId;
use cf_render::{AbsolutePosition, AudioFeatures, CurrentShowFx, LoadTracker, SceneColor, ScenePalette};

use super::effects;
use super::glb::CarModelError;
use super::glow::{
    CarPalette, HEADLIGHT_LEVEL, HIGH_BEAMS_LEVEL, HIGH_BEAMS_RATIO, LightInputs, LightLevels, NEON_INTENSITY, Tinted,
};
use super::materials::{CarLightMaterial, CarPaintMaterial};
use super::model::{CarModelAsset, Side};
use super::motion::{MotionInput, MotionPose};
use super::paint::paint_scales;
use super::rig::{Car, CarRig, CarState, Emissive, Look};
use super::spawn::{CarAssets, spawn_car};
use crate::RoadLook;
use crate::chase::Runner;

/// Vitesse de croisière de la voiture, m/s : la référence de `speed_ratio` (la vitesse du point suivi provisoire).
pub const CRUISE_SPEED_MPS: f32 = crate::chase::CRUISE_SPEED_MPS as f32;

/// Ce que le pilote (J5-06) décide et que la voiture ne peut pas déduire de son mouvement. La vitesse, la vitesse
/// latérale et l'accélération, elles, sont mesurées sur sa position absolue.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct CarControls {
    /// Angle du volant, −1 (gauche) à 1 (droite) : braque les roues et commande le clignotant.
    pub steer: f32,
    /// Freinage ou levé de pied, 0 à 1 : feux stop, fumée, reflets rouges dans la chaussée.
    pub brake: f32,
    /// Couleur du faisceau que la voiture traverse (`beams.sample`) : la lumière migre vers elle en 0,5 s. `None` :
    /// le violet de la palette.
    pub lit: Option<LinearRgba>,
}

impl Default for CarControls {
    fn default() -> Self {
        Self {
            steer: 0.0,
            brake: 0.0,
            lit: None,
        }
    }
}

/// Réglages que la qualité adaptative (J4-0x) peut baisser sans toucher au reste.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct CarSettings {
    /// Densité de la fumée, 0 à 1 (`density` de `Tires.update` : 0,4 au plus bas de la qualité, 0,25 à l'arrêt).
    pub smoke_density: f32,
}

impl Default for CarSettings {
    fn default() -> Self {
        Self { smoke_density: 1.0 }
    }
}

/// Version du modèle à charger (`pickQuality` de Sillage : la légère pour les machines modestes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelQuality {
    /// `car-high.glb` : textures 2k, ~85 000 triangles.
    High,
    /// `car-low.glb` : textures 1k, ~40 000 triangles.
    Low,
    /// La légère sur quatre cœurs ou moins (comme Sillage : « peu de mémoire ou de cœurs »), la haute sinon.
    #[default]
    Auto,
}

impl ModelQuality {
    /// Chemin du modèle, relatif au dossier `assets/`.
    #[must_use]
    pub fn path(self) -> &'static str {
        let low = match self {
            Self::High => false,
            Self::Low => true,
            Self::Auto => std::thread::available_parallelism().is_ok_and(|n| n.get() <= 4),
        };
        if low {
            "models/car/car-low.glb"
        } else {
            "models/car/car-high.glb"
        }
    }
}

/// Où lire le modèle : posé par le plugin.
#[derive(Resource, Debug, Clone)]
pub(super) struct CarModelSource {
    pub(super) path: std::path::PathBuf,
}

/// Le décodage en cours, dans une tâche de fond : retiré quand la voiture est construite (ou que le modèle est refusé).
#[derive(Resource)]
pub(super) struct CarModelTask {
    task: Task<Result<CarModelAsset, CarModelError>>,
    /// Étape du carnet de chargement (J4-07) : close quand la voiture est posée, ou en échec si le modèle est refusé.
    step: Option<StepId>,
}

/// `Startup` : lance la lecture et le décodage du modèle (hors du fil principal).
pub(super) fn load_model(
    mut commands: Commands,
    source: Res<CarModelSource>,
    tracker: Option<ResMut<LoadTracker>>,
) {
    let path = source.path.clone();
    let task = AsyncComputeTaskPool::get().spawn(async move {
        let bytes = std::fs::read(&path).map_err(|source| CarModelError::Read {
            path: path.display().to_string(),
            source,
        })?;
        CarModelAsset::from_glb(&bytes)
    });
    let step = tracker.map(|mut tracker| tracker.register("Voiture (modèle GLB)"));
    commands.insert_resource(CarModelTask { task, step });
}

fn color(c: cf_render::palette::LinearRgb) -> LinearRgba {
    LinearRgba::rgb(c.r, c.g, c.b)
}

/// Les couleurs de la scène que lit la voiture.
pub(super) fn car_palette(scene: &ScenePalette) -> CarPalette {
    CarPalette {
        violet: color(scene.get(SceneColor::Violet)),
        rose: color(scene.get(SceneColor::Rose)),
        ice: color(scene.get(SceneColor::Ice)),
        alarm: color(scene.get(SceneColor::Alarm)),
        amber: color(scene.get(SceneColor::Amber)),
    }
}

/// `CfSet::Simulate` : quand le modèle est décodé, pose la voiture sur le point suivi provisoire (`Runner`) ou sur
/// l'entité `Car` que le pilote a créée, et lance la traînée et la fumée.
///
/// La voiture ne crée jamais sa propre racine : un seul `StreamFocus` doit exister, c'est au pilote (ou au point suivi
/// provisoire) de le porter.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources, requêtes et collections d'assets.
pub(super) fn build_car(
    mut commands: Commands,
    task: Option<ResMut<CarModelTask>>,
    mut assets: CarAssets,
    scene: Res<ScenePalette>,
    targets: Query<Entity, (With<Car>, Without<CarRig>)>,
    runner: Query<Entity, (With<Runner>, Without<Car>)>,
    mut positions: Query<&mut AbsolutePosition>,
    tracker: Option<ResMut<LoadTracker>>,
) {
    let Some(mut task) = task else { return };
    let Some(result) = future::block_on(future::poll_once(&mut task.task)) else {
        return;
    };
    let target = targets.iter().next().or_else(|| runner.iter().next());
    let Some(root) = target else { return };
    commands.remove_resource::<CarModelTask>();
    let (step, mut tracker) = (task.step, tracker);
    let model = match result {
        Ok(model) => model,
        Err(error) => {
            if let (Some(step), Some(tracker)) = (step, tracker.as_mut()) {
                tracker.fail(step, format!("voiture : {error}"));
            }
            // Pas de voiture : le point suivi provisoire reste à l'écran, le show continue (replis de `live-safety`).
            tracing::error!(%error, "voiture : modèle illisible, le point suivi provisoire reste");
            return;
        }
    };

    let palette = car_palette(&scene);
    let rig = spawn_car(&mut commands, root, model, &mut assets, &palette, palette.violet);
    // Le point suivi provisoire est un pavé : la voiture le remplace. Elle pose ses pneus à y = 0 (le pavé était posé
    // sur le sol par son centre, à 0,65 m).
    commands
        .entity(root)
        .remove::<(Mesh3d, MeshMaterial3d<StandardMaterial>)>()
        .insert((Car, CarState::new(palette.violet), rig));
    if let Ok(mut position) = positions.get_mut(root) {
        position.0.y = 0.0;
    }
    effects::spawn_effects(&mut commands, &mut assets.meshes, &mut assets.lights);
    if let (Some(step), Some(tracker)) = (step, tracker.as_mut()) {
        tracker.finish(step);
    }
}

fn set_if_different<A: Asset>(
    assets: &mut Assets<A>,
    handle: &Handle<A>,
    differs: impl Fn(&A) -> bool,
    set: impl FnOnce(&mut A),
) {
    if assets.get(handle).is_some_and(differs)
        && let Some(mut asset) = assets.get_mut(handle)
    {
        set(&mut asset);
    }
}

/// `CfSet::Simulate` : mesure ce que fait la voiture (sa position absolue), avance suspension et roues, pose les
/// transformations.
pub(super) fn drive_car(
    time: Res<Time>,
    controls: Res<CarControls>,
    mut cars: Query<(&AbsolutePosition, &CarRig, &mut CarState, &mut Transform), With<Car>>,
    mut transforms: Query<&mut Transform, Without<Car>>,
) {
    let dt = time.delta_secs();
    for (position, rig, mut state, mut root) in &mut cars {
        measure(&mut state, position.0, dt);
        let speed_ratio = (state.speed_mps / CRUISE_SPEED_MPS).max(0.0);
        let input = MotionInput {
            dt_s: dt,
            speed_mps: state.speed_mps,
            speed_ratio,
            lateral_mps: state.lateral_mps,
            accel_mps2: state.accel_mps2,
            steer: controls.steer,
        };
        let pose = state.motion.update(&input);
        apply_pose(&pose, rig, &mut root, &mut transforms);
    }
}

/// Vitesse, vitesse latérale et accélération, mesurées sur le déplacement de la position absolue.
fn measure(state: &mut CarState, position: DVec3, dt: f32) {
    if let Some(previous) = state.previous
        && dt > 0.0
    {
        let delta = position - previous;
        let step = f64::from(dt);
        // La route va vers −z.
        let speed = (-delta.z / step) as f32;
        state.lateral_mps = (delta.x / step) as f32;
        if state.warm {
            // Accélération lissée : un déplacement ne donne la vitesse qu'à l'image près, sa dérivée est bruitée.
            let raw = ((speed - state.speed_mps) / dt).clamp(-40.0, 40.0);
            state.accel_mps2 += (raw - state.accel_mps2) * (1.0 - (-dt * 8.0).exp());
        }
        state.speed_mps = speed;
        state.warm = true;
    }
    state.previous = Some(position);
}

fn apply_pose(
    pose: &MotionPose,
    rig: &CarRig,
    root: &mut Transform,
    transforms: &mut Query<&mut Transform, Without<Car>>,
) {
    root.rotation = Quat::from_rotation_y(pose.yaw_rad);
    if let Ok(mut chassis) = transforms.get_mut(rig.chassis) {
        chassis.rotation = pose.chassis_rotation;
        chassis.translation = pose.chassis_translation;
    }
    for &entity in &rig.spin {
        if let Ok(mut wheel) = transforms.get_mut(entity) {
            wheel.rotation = Quat::from_rotation_x(pose.wheel_spin_rad);
        }
    }
    for &entity in &rig.steer {
        if let Ok(mut wheel) = transforms.get_mut(entity) {
            wheel.rotation = Quat::from_rotation_y(pose.wheel_steer_rad);
        }
    }
}

/// Règle un néon : couleur de base au quart, émission pleine (`paintNeon` de `CyberCar.ts`).
fn neon(assets: &mut Assets<StandardMaterial>, neon: Option<&Emissive>, glow: LinearRgba) {
    let Some(neon) = neon else { return };
    let emissive = LinearRgba::rgb(
        glow.red * neon.strength * NEON_INTENSITY,
        glow.green * neon.strength * NEON_INTENSITY,
        glow.blue * neon.strength * NEON_INTENSITY,
    );
    let base = Color::LinearRgba(LinearRgba::rgb(glow.red * 0.25, glow.green * 0.25, glow.blue * 0.25));
    set_if_different(
        assets,
        &neon.handle,
        |m| m.emissive != emissive,
        |m| {
            m.emissive = emissive;
            m.base_color = base;
        },
    );
}

fn emit(assets: &mut Assets<StandardMaterial>, emissive: &Emissive, level: f32) {
    let value = emissive.at(level);
    set_if_different(
        assets,
        &emissive.handle,
        |m| m.emissive != value,
        |m| m.emissive = value,
    );
}

/// Allume ou éteint un halo ou une flaque : teinte et visibilité (`sprite.visible = level > 0.02`).
fn glow_material(
    lights: &mut Assets<CarLightMaterial>,
    visibility: &mut Query<&mut Visibility>,
    handle: &Handle<CarLightMaterial>,
    entity: Entity,
    color: LinearRgba,
) {
    let on = color.red + color.green + color.blue > 0.02;
    let value = Vec4::new(color.red, color.green, color.blue, 1.0);
    set_if_different(lights, handle, |m| m.params.color != value, |m| m.params.color = value);
    if let Ok(mut v) = visibility.get_mut(entity) {
        v.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
    }
}

/// `CfSet::Simulate` : décide de la lumière de l'image (teinte du moment, clignotant, logo, niveaux des optiques) et la
/// range dans [`CarState::look`] ; écrit aussi la lueur et le freinage que lit la route (`RoadLook`).
pub(super) fn advance_look(
    time: Res<Time>,
    features: Option<Res<AudioFeatures>>,
    show: Option<Res<CurrentShowFx>>,
    scene: Res<ScenePalette>,
    controls: Res<CarControls>,
    mut look: ResMut<RoadLook>,
    mut cars: Query<&mut CarState>,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs_f64();
    let palette = car_palette(&scene);
    let raw_pulse = features.map_or(0.0, |f| f.0.pulse);
    // Même politique que le post-process (`ShowFx::update`) : permis par le spectacle, hors respiration ; sans
    // spectacle, rien ne flashe.
    let policy = show.map(|s| (s.0.flashes, s.0.breathing));
    for mut state in &mut cars {
        let speed_ratio = (state.speed_mps / CRUISE_SPEED_MPS).max(0.0);
        let brake = state.brake.toward(controls.brake.clamp(0.0, 1.0), dt, 1.0);
        state.glow.set_target(controls.lit.unwrap_or(palette.violet));
        let glow = state.glow.update(dt);
        state.blinker.update(dt, controls.steer);
        // Règle 7 : kicks et clignotants ne montrent que ce que leurs limiteurs accordent.
        let lit = state.blinker.lit();
        state.flash.update(dt, now, policy, raw_pulse, lit);
        let (pulse, blink) = (state.flash.kick(), state.flash.blink());
        let animated = state.flash.permitted();
        state.logo.update(dt, pulse, animated);
        let high_beams = speed_ratio > HIGH_BEAMS_RATIO;
        let target = if high_beams { HIGH_BEAMS_LEVEL } else { HEADLIGHT_LEVEL };
        let level = state.level.toward(target, dt, HIGH_BEAMS_LEVEL - HEADLIGHT_LEVEL);
        let inputs = LightInputs {
            glow,
            pulse,
            brake,
            speed_ratio,
            level,
        };
        state.look = Some(Look {
            glow,
            secondary: glow.mix(&palette.rose, 0.7),
            palette,
            levels: LightLevels::compute(&palette, &inputs),
            high_beams,
            pulse,
            blink,
            brake,
        });
        // La route reflète la lueur de la voiture et ses feux stop.
        if look.glow != glow {
            look.glow = glow;
        }
        if (look.brake - brake).abs() > f32::EPSILON {
            look.brake = brake;
        }
    }
}

/// `CfSet::Simulate` : applique la lumière de l'image aux matériaux du modèle : néons, optiques, clignotants, logo
/// arrière, peinture.
pub(super) fn apply_model_lights(
    mut cars: Query<(&CarRig, &CarState)>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut paint: ResMut<Assets<CarPaintMaterial>>,
    mut lights: ResMut<Assets<CarLightMaterial>>,
    mut visibility: Query<&mut Visibility>,
    mut transforms: Query<&mut Transform, Without<Car>>,
) {
    for (rig, state) in &mut cars {
        let Some(look) = state.look else { continue };
        let m = &rig.materials;
        // Néons : la lueur du moment, et la même mêlée de corail pour les accents.
        neon(&mut standard, m.neon_primary.as_ref(), look.glow);
        neon(&mut standard, m.neon_secondary.as_ref(), look.secondary);
        // Optiques du modèle : phares allumés (plus fort en feux de route), feux stop au freinage.
        if let Some(head) = &m.head {
            emit(&mut standard, head, if look.high_beams { 1.35 } else { 1.0 });
        }
        if let Some(tail) = &m.brake {
            emit(&mut standard, tail, LightLevels::brake_light_factor(look.brake, true));
        }
        // Clignotants : un seul côté, celui où le volant tourne. La lentille, son halo et sa flaque battent ensemble.
        let side = state.blinker.side;
        let amber = look.palette.amber;
        let scale = |k: f32| LinearRgba::rgb(amber.red * k, amber.green * k, amber.blue * k);
        for signal in &rig.signals {
            let mine = (signal.side == Side::Left && side < 0) || (signal.side == Side::Right && side > 0);
            // Le battement est le niveau du limiteur (0 → 1), jamais un créneau.
            let beat = if mine { look.blink } else { 0.0 };
            emit(&mut standard, &signal.material, 0.03 + 0.97 * beat);
            let (halo, pool) = (scale(2.2 * beat), scale(0.9 * beat));
            glow_material(&mut lights, &mut visibility, &signal.halo, signal.halo_entity, halo);
            glow_material(&mut lights, &mut visibility, &signal.pool, signal.pool_entity, pool);
        }
        apply_logo(rig, state, &look, &mut standard, &mut transforms);
        apply_paint(rig, &look, &mut paint);
    }
}

/// Logo arrière : plus lumineux que les néons, il pulse avec les basses et les kicks.
fn apply_logo(
    rig: &CarRig,
    state: &CarState,
    look: &Look,
    standard: &mut Assets<StandardMaterial>,
    transforms: &mut Query<&mut Transform, Without<Car>>,
) {
    let glow = look.glow;
    if let Some(logo) = &rig.materials.logo {
        let k = state.logo.emissive_strength();
        let emissive = LinearRgba::rgb(glow.red * k, glow.green * k, glow.blue * k);
        let base = Color::LinearRgba(LinearRgba::rgb(glow.red * 0.25, glow.green * 0.25, glow.blue * 0.25));
        set_if_different(
            standard,
            &logo.handle,
            |x| x.emissive != emissive,
            |x| {
                x.emissive = emissive;
                x.base_color = base;
            },
        );
    }
    if let Some((entity, rest)) = rig.logo
        && let Ok(mut transform) = transforms.get_mut(entity)
    {
        transform.scale = rest * state.logo.scale();
    }
}

/// Peinture : teinte du thème, liseré néon qui pulse sur les kicks.
fn apply_paint(rig: &CarRig, look: &Look, paint: &mut Assets<CarPaintMaterial>) {
    let Some(handle) = &rig.materials.paint else { return };
    let (hue, sat, val) = paint_scales(look.palette.violet);
    let rim = Vec4::new(look.glow.red, look.glow.green, look.glow.blue, 1.0);
    let rim_b = Vec4::new(look.secondary.red, look.secondary.green, look.secondary.blue, 1.0);
    let strength = 0.35 * (1.0 + look.pulse * 1.6);
    let wanted = Vec3::new(hue, sat, val);
    let differs = |x: &CarPaintMaterial| {
        let p = &x.extension.params;
        p.paint.truncate() != wanted || p.rim_a != rim || p.rim_b != rim_b || (p.finish.w - strength).abs() > 1e-4
    };
    set_if_different(paint, handle, differs, |x| {
        let p = &mut x.extension.params;
        p.paint = Vec4::new(hue, sat, val, p.paint.w);
        p.rim_a = rim;
        p.rim_b = rim_b;
        p.finish.w = strength;
    });
}

/// `CfSet::Simulate` : applique la lumière de l'image à ce que le code a ajouté : volumes de phares, flaques, halos,
/// lueur sous la caisse, projecteurs.
pub(super) fn apply_rig_lights(
    mut cars: Query<(&CarRig, &CarState)>,
    mut lights: ResMut<Assets<CarLightMaterial>>,
    mut visibility: Query<&mut Visibility>,
    mut transforms: Query<&mut Transform, Without<Car>>,
    mut spots: Query<&mut SpotLight>,
    mut points: Query<&mut PointLight, Without<SpotLight>>,
) {
    for (rig, state) in &mut cars {
        let Some(look) = state.look else { continue };
        let levels = &look.levels;
        let tinted = |t: &Tinted| Vec4::new(t.color.red, t.color.green, t.color.blue, t.k);
        for (handle, value) in [
            (&rig.lights.beam, tinted(&levels.beam)),
            (&rig.lights.front_pool, tinted(&levels.front_pool)),
            (&rig.lights.rear_pool, tinted(&levels.rear_pool)),
        ] {
            set_if_different(
                &mut lights,
                handle,
                |x| x.params.color != value,
                |x| x.params.color = value,
            );
        }
        for halo in &rig.lights.heads {
            glow_material(
                &mut lights,
                &mut visibility,
                &halo.material,
                halo.entity,
                levels.head_halo,
            );
        }
        for halo in &rig.lights.tails {
            glow_material(
                &mut lights,
                &mut visibility,
                &halo.material,
                halo.entity,
                levels.tail_halo,
            );
        }
        let under = &rig.lights.under;
        glow_material(
            &mut lights,
            &mut visibility,
            &under.material,
            under.entity,
            levels.under,
        );
        if let Ok(mut transform) = transforms.get_mut(under.entity) {
            transform.scale = Vec3::new(3.2, 1.0, levels.under_length_m);
        }

        // Projecteurs de Bevy : feux de croisement ou de route.
        for &entity in &rig.lights.spots {
            let Ok(mut spot) = spots.get_mut(entity) else { continue };
            let (cd, angle, range) = if look.high_beams {
                (150.0, 0.3, 100.0)
            } else {
                (60.0, 0.5, 55.0)
            };
            let wanted = cd * 4.0 * std::f32::consts::PI;
            if (spot.intensity - wanted).abs() > f32::EPSILON {
                spot.intensity = wanted;
                spot.outer_angle = angle;
                spot.inner_angle = angle * if look.high_beams { 0.5 } else { 0.3 };
                spot.range = range;
            }
        }
        if let Some(entity) = rig.lights.under_light
            && let Ok(mut light) = points.get_mut(entity)
        {
            light.color = Color::LinearRgba(look.glow);
        }
        if let Some(entity) = rig.lights.logo_light
            && let Ok(mut light) = points.get_mut(entity)
        {
            light.color = Color::LinearRgba(look.glow);
            // `0,7 × (0,5 + 3,2 × niveau × min(1,5 ; boost))` candelas, boost du logo à 1,6.
            light.intensity = 0.7 * (0.5 + 3.2 * state.logo.level() * 1.5) * 4.0 * std::f32::consts::PI;
        }
    }
}
