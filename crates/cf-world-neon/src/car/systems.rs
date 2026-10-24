//! Les systèmes de la voiture : création à l'arrivée du modèle, mouvement de la caisse et des roues, lumière.

use bevy::asset::{AssetServer, Assets, Handle};
use bevy::color::{LinearRgba, Mix};
use bevy::light::{PointLight, SpotLight};
use bevy::math::{DVec3, Quat, Vec3};
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use cf_render::{AbsolutePosition, AudioFeatures, SceneColor, ScenePalette};

use super::effects;
use super::glow::{
    CarPalette, HEADLIGHT_LEVEL, HIGH_BEAMS_LEVEL, HIGH_BEAMS_RATIO, LightInputs, LightLevels, NEON_INTENSITY,
};
use super::materials::{CarLightMaterial, CarPaintMaterial};
use super::model::{CarModelAsset, Side};
use super::motion::{MotionInput, MotionPose};
use super::paint::paint_scales;
use super::rig::{Car, CarRig, CarState, Emissive};
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

/// Le modèle en cours de chargement : posé au démarrage, retiré quand la voiture est construite.
#[derive(Resource, Debug)]
pub struct CarModelHandle(pub Handle<CarModelAsset>);

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

/// Réglage du plugin, gardé en ressource jusqu'au chargement.
#[derive(Resource, Debug, Clone, Copy)]
pub(super) struct CarQuality(pub ModelQuality);

/// `Startup` : lance le chargement du modèle (hors du fil principal).
pub(super) fn load_model(mut commands: Commands, server: Res<AssetServer>, quality: Res<CarQuality>) {
    commands.insert_resource(CarModelHandle(server.load(quality.0.path())));
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
    handle: Option<Res<CarModelHandle>>,
    mut models: ResMut<Assets<CarModelAsset>>,
    mut assets: CarAssets,
    scene: Res<ScenePalette>,
    targets: Query<Entity, (With<Car>, Without<CarRig>)>,
    runner: Query<Entity, (With<Runner>, Without<Car>)>,
    mut positions: Query<&mut AbsolutePosition>,
) {
    let Some(handle) = handle else { return };
    if !models.contains(handle.0.id()) {
        return;
    }
    let target = targets.iter().next().or_else(|| runner.iter().next());
    let Some(root) = target else { return };
    let Some(model) = models.remove(handle.0.id()) else { return };
    commands.remove_resource::<CarModelHandle>();

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
}

fn set_if_different<A: Asset>(assets: &mut Assets<A>, handle: &Handle<A>, differs: impl Fn(&A) -> bool, set: impl FnOnce(&mut A)) {
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

fn apply_pose(pose: &MotionPose, rig: &CarRig, root: &mut Transform, transforms: &mut Query<&mut Transform, Without<Car>>) {
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
fn neon(assets: &mut Assets<StandardMaterial>, neon: &Option<Emissive>, glow: LinearRgba) {
    let Some(neon) = neon else { return };
    let emissive = LinearRgba::rgb(
        glow.red * neon.strength * NEON_INTENSITY,
        glow.green * neon.strength * NEON_INTENSITY,
        glow.blue * neon.strength * NEON_INTENSITY,
    );
    let base = Color::LinearRgba(LinearRgba::rgb(glow.red * 0.25, glow.green * 0.25, glow.blue * 0.25));
    set_if_different(assets, &neon.handle, |m| m.emissive != emissive, |m| {
        m.emissive = emissive;
        m.base_color = base;
    });
}

fn emit(assets: &mut Assets<StandardMaterial>, emissive: &Emissive, level: f32) {
    let value = emissive.at(level);
    set_if_different(assets, &emissive.handle, |m| m.emissive != value, |m| m.emissive = value);
}

/// Allume ou éteint un halo ou une flaque : teinte et visibilité (`sprite.visible = level > 0.02`).
fn glow_material(
    lights: &mut Assets<CarLightMaterial>,
    visibility: &mut Query<&mut Visibility>,
    handle: &Handle<CarLightMaterial>,
    entity: Entity,
    color: LinearRgba,
    k: f32,
) {
    let on = color.red + color.green + color.blue > 0.02 && k > 0.0;
    let value = Vec4::new(color.red, color.green, color.blue, k);
    set_if_different(lights, handle, |m| m.params.color != value, |m| m.params.color = value);
    if let Ok(mut v) = visibility.get_mut(entity) {
        v.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
    }
}

/// `CfSet::Simulate` : la lumière de la voiture. Teinte du moment, néons, optiques, clignotants, halos, flaques,
/// logo arrière, peinture, projecteurs ; écrit aussi la lueur et le freinage que lit la route (`RoadLook`).
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources, requêtes et collections d'assets.
pub(super) fn light_car(
    time: Res<Time>,
    features: Option<Res<AudioFeatures>>,
    scene: Res<ScenePalette>,
    controls: Res<CarControls>,
    mut look: ResMut<RoadLook>,
    mut cars: Query<(&CarRig, &mut CarState)>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut paint: ResMut<Assets<CarPaintMaterial>>,
    mut lights: ResMut<Assets<CarLightMaterial>>,
    mut visibility: Query<&mut Visibility>,
    mut transforms: Query<&mut Transform, Without<Car>>,
    mut spots: Query<&mut SpotLight>,
    mut points: Query<&mut PointLight, Without<SpotLight>>,
) {
    let dt = time.delta_secs();
    let palette = car_palette(&scene);
    let (live, bass, pulse) = features.map_or((false, 0.0, 0.0), |f| (f.0.live, f.0.bass, f.0.pulse));
    for (rig, mut state) in &mut cars {
        let speed_ratio = (state.speed_mps / CRUISE_SPEED_MPS).max(0.0);
        let brake = controls.brake.clamp(0.0, 1.0);
        state.glow.set_target(controls.lit.unwrap_or(palette.violet));
        let glow = state.glow.update(dt);
        state.blinker.update(dt, controls.steer);
        state.logo.update(dt, live, bass, pulse);
        let high_beams = speed_ratio > HIGH_BEAMS_RATIO;
        let level = if high_beams { HIGH_BEAMS_LEVEL } else { HEADLIGHT_LEVEL };
        let levels = LightLevels::compute(
            &palette,
            &LightInputs {
                glow,
                pulse,
                brake,
                speed_ratio,
                level,
            },
        );
        let secondary = glow.mix(&palette.rose, 0.7);

        // Néons : la lueur du moment, et la même mêlée de corail pour les accents.
        let m = &rig.materials;
        neon(&mut standard, &m.neon_primary, glow);
        neon(&mut standard, &m.neon_secondary, secondary);
        // Optiques du modèle : phares allumés (plus fort en feux de route), feux stop au freinage.
        if let Some(head) = &m.head {
            emit(&mut standard, head, if high_beams { 1.35 } else { 1.0 });
        }
        if let Some(tail) = &m.brake {
            emit(&mut standard, tail, LightLevels::brake_light_factor(brake, true));
        }
        // Clignotants : un seul côté, celui où le volant tourne. La lentille, son halo et sa flaque battent ensemble.
        let side = state.blinker.side;
        let lit = state.blinker.lit();
        for signal in &rig.signals {
            let mine = (signal.side == Side::Left && side < 0) || (signal.side == Side::Right && side > 0);
            let on = lit && mine;
            emit(&mut standard, &signal.material, if on { 1.0 } else { 0.03 });
            let amber = palette.amber;
            let scale = |k: f32| LinearRgba::rgb(amber.red * k, amber.green * k, amber.blue * k);
            let (halo, pool) = if on { (scale(2.2), scale(0.9)) } else { (LinearRgba::BLACK, LinearRgba::BLACK) };
            glow_material(&mut lights, &mut visibility, &signal.halo, signal.halo_entity, halo, 1.0);
            glow_material(&mut lights, &mut visibility, &signal.pool, signal.pool_entity, pool, 1.0);
        }

        // Logo arrière : plus lumineux que les néons, il pulse avec les basses et les kicks.
        if let Some(logo) = &m.logo {
            let emissive = LinearRgba::rgb(
                glow.red * state.logo.emissive_strength(),
                glow.green * state.logo.emissive_strength(),
                glow.blue * state.logo.emissive_strength(),
            );
            let base = Color::LinearRgba(LinearRgba::rgb(glow.red * 0.25, glow.green * 0.25, glow.blue * 0.25));
            set_if_different(&mut standard, &logo.handle, |x| x.emissive != emissive, |x| {
                x.emissive = emissive;
                x.base_color = base;
            });
        }
        if let Some((entity, rest)) = rig.logo
            && let Ok(mut transform) = transforms.get_mut(entity)
        {
            transform.scale = rest * state.logo.scale();
        }

        // Volumes de phares, flaques, halos et lueur sous la caisse.
        let beam = Vec4::new(levels.beam.color.red, levels.beam.color.green, levels.beam.color.blue, levels.beam.k);
        set_if_different(&mut lights, &rig.lights.beam, |x| x.params.color != beam, |x| x.params.color = beam);
        for (handle, tinted) in [(&rig.lights.front_pool, levels.front_pool), (&rig.lights.rear_pool, levels.rear_pool)] {
            let value = Vec4::new(tinted.color.red, tinted.color.green, tinted.color.blue, tinted.k);
            set_if_different(&mut lights, handle, |x| x.params.color != value, |x| x.params.color = value);
        }
        for halo in &rig.lights.heads {
            glow_material(&mut lights, &mut visibility, &halo.material, halo.entity, levels.head_halo, 1.0);
        }
        for halo in &rig.lights.tails {
            glow_material(&mut lights, &mut visibility, &halo.material, halo.entity, levels.tail_halo, 1.0);
        }
        let under = &rig.lights.under;
        glow_material(&mut lights, &mut visibility, &under.material, under.entity, levels.under, 1.0);
        if let Ok(mut transform) = transforms.get_mut(under.entity) {
            transform.scale = Vec3::new(3.2, 1.0, levels.under_length_m);
        }

        // Peinture : teinte du thème, liseré néon qui pulse sur les kicks.
        if let Some(handle) = &m.paint {
            let (hue, sat, val) = paint_scales(palette.violet);
            let rim = Vec4::new(glow.red, glow.green, glow.blue, 1.0);
            let rim_b = Vec4::new(secondary.red, secondary.green, secondary.blue, 1.0);
            let strength = 0.35 * (1.0 + pulse * 1.6);
            let wanted = Vec4::new(hue, sat, val, 0.0);
            let differs = |x: &CarPaintMaterial| {
                let p = &x.extension.params;
                p.paint.truncate() != wanted.truncate()
                    || p.rim_a != rim
                    || p.rim_b != rim_b
                    || (p.finish.w - strength).abs() > 1e-4
            };
            set_if_different(&mut paint, handle, differs, |x| {
                let p = &mut x.extension.params;
                p.paint = Vec4::new(hue, sat, val, p.paint.w);
                p.rim_a = rim;
                p.rim_b = rim_b;
                p.finish.w = strength;
            });
        }

        // Projecteurs de Bevy : feux de croisement ou de route.
        for &entity in &rig.lights.spots {
            let Ok(mut spot) = spots.get_mut(entity) else { continue };
            let (cd, angle, range) = if high_beams { (150.0, 0.3, 100.0) } else { (60.0, 0.5, 55.0) };
            let wanted = cd * 4.0 * std::f32::consts::PI;
            if (spot.intensity - wanted).abs() > f32::EPSILON {
                spot.intensity = wanted;
                spot.outer_angle = angle;
                spot.inner_angle = angle * if high_beams { 0.5 } else { 0.3 };
                spot.range = range;
            }
        }
        if let Some(entity) = rig.lights.under_light
            && let Ok(mut light) = points.get_mut(entity)
        {
            light.color = Color::LinearRgba(glow);
        }
        if let Some(entity) = rig.lights.logo_light
            && let Ok(mut light) = points.get_mut(entity)
        {
            light.color = Color::LinearRgba(glow);
            // `0,7 × (0,5 + 3,2 × niveau × min(1,5 ; boost))` candelas, boost du logo à 1,6.
            light.intensity = 0.7 * (0.5 + 3.2 * state.logo.level() * 1.5) * 4.0 * std::f32::consts::PI;
        }

        // La route reflète la lueur de la voiture et ses feux stop.
        if look.glow != glow {
            look.glow = glow;
        }
        if (look.brake - brake).abs() > f32::EPSILON {
            look.brake = brake;
        }
    }
}
