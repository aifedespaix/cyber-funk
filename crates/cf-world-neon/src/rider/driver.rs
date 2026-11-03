//! Le conducteur de la voiture : le manul, assis au volant de la « dirty car », figé dans sa pose (`pose_car` de
//! `tools/char/pose.py`, GLB `characters/driver-car.glb`). Il est posé sur la caisse : il suit la suspension.
//!
//! Il n'est posé que sur la dirty car : la pose est calée sur son siège, son volant et ses pédales (la `CyberCar` a une autre
//! habitacle). Un changement de voiture détruit la caisse et son conducteur ; le suivant est reposé sur la nouvelle caisse.
//!
//! Décodé une fois à l'ouverture (règle 8), puis cloné à chaque pose de la voiture. Sans fichier, la voiture roule sans
//! conducteur : le show continue.

use std::path::PathBuf;

use bevy::ecs::system::{Local, Query, Res};
use bevy::math::{EulerRot, Quat, Vec3};
use bevy::prelude::{Commands, Component, Entity, ResMut, Resource, Time, Transform, With, Without};
use cf_render::AudioFeatures;

use super::arm_ik::turned_grip;
use super::gearbox::{Gear, LEVER_LENGTH_M, Shift, lever_angles};
use super::spawn::{DriverNodes, RiderAssets, spawn_model};
use crate::car::model::CarModelAsset;
use crate::car::{ActiveCar, Car, CarRig, CarState, CarVariant};

/// Chemin du GLB du conducteur (dans le dossier d'assets de la voiture).
#[derive(Resource, Debug, Clone)]
pub struct DriverSource(pub PathBuf);

/// Le conducteur décodé : un exemplaire intact, cloné à chaque pose de la voiture.
#[derive(Resource, Debug)]
pub struct DriverModel(pub CarModelAsset);

/// Marque l'entité racine du conducteur.
#[derive(Component, Debug, Clone, Copy)]
pub struct Driver;

/// `Startup` : lit et décode le conducteur. Un fichier absent ou illisible est dit au journal, sans arrêter le show.
pub(crate) fn load_driver(mut commands: Commands, source: Res<DriverSource>) {
    let decoded = std::fs::read(&source.0)
        .map_err(|error| error.to_string())
        .and_then(|bytes| CarModelAsset::from_glb(&bytes).map_err(|error| error.to_string()));
    match decoded {
        Ok(model) => commands.insert_resource(DriverModel(model)),
        Err(error) => {
            tracing::warn!(%error, path = %source.0.display(), "conducteur illisible : la voiture roule sans lui");
        }
    }
}

/// `CfSet::Simulate`, après le choix de la voiture : pose le conducteur sur la caisse de la dirty car quand elle vient
/// d'être posée (la caisse change d'entité à chaque changement de voiture).
pub(crate) fn attach_driver(
    mut commands: Commands,
    cars: Query<&CarRig, With<Car>>,
    active: Res<ActiveCar>,
    model: Option<Res<DriverModel>>,
    mut assets: RiderAssets,
    mut equipped: Local<Option<Entity>>,
) {
    let (Some(model), Ok(rig)) = (model, cars.single()) else {
        return;
    };
    if active.0 != CarVariant::Dirty {
        *equipped = None;
        return;
    }
    if *equipped == Some(rig.chassis) {
        return;
    }
    let (root, spawned) = spawn_model(&mut commands, rig.chassis, model.0.clone(), &mut assets.refs(), None);
    commands
        .entity(root)
        .insert((Driver, DriverRig(spawned.driver), Gearshift::default()));
    *equipped = Some(rig.chassis);
}

/// Les bras du conducteur à animer : posés avec lui, lus par [`animate_driver`].
#[derive(Component, Debug, Clone, Copy)]
pub struct DriverRig(pub DriverNodes);

/// La boîte de vitesses du conducteur : le rapport engagé et le geste en cours. `None` tant que la voiture n'a pas de
/// vitesse mesurée (elle peut démarrer lancée : le rapport est alors posé d'emblée, sans geste).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Gearshift {
    /// Rapport engagé (celui de la case où repose le levier).
    pub gear: Option<Gear>,
    /// Changement de rapport en cours.
    pub shift: Option<Shift>,
}

impl Gearshift {
    /// Avance la boîte de `dt_s` pour une voiture à `speed_mps` : un geste se déroule, puis le suivant peut commencer.
    pub fn advance(&mut self, speed_mps: f32, dt_s: f32) {
        let Some(gear) = self.gear else {
            self.gear = Some(Gear::settled(speed_mps));
            return;
        };
        if let Some(shift) = self.shift.as_mut() {
            shift.elapsed_s += dt_s;
            if shift.is_done() {
                self.gear = Some(shift.to);
                self.shift = None;
            }
        }
        if self.shift.is_none() {
            let next = gear_after(self.gear.unwrap_or(gear), speed_mps);
            if let Some(next) = next {
                self.shift = Some(Shift::new(self.gear.unwrap_or(gear), next));
            }
        }
    }

    /// Où est le pommeau (m, dans la grille) et part de la main qui y est (0 volant, 1 pommeau).
    #[must_use]
    pub fn knob_and_hand(&self) -> ((f32, f32), f32) {
        match (self.shift, self.gear) {
            (Some(shift), _) => (shift.knob(), shift.hand_on_lever()),
            (None, Some(gear)) => (gear.gate(), 0.0),
            (None, None) => (Gear::Neutral.gate(), 0.0),
        }
    }
}

/// Hoche la tête du conducteur sur le tempo : fait pivoter son buste autour du bassin et publie ses yeux pour la vue au volant.
fn nod_head(
    time: &Time,
    features: Option<&AudioFeatures>,
    view: &mut DriverView,
    rig: &CarRig,
    driver: &DriverRig,
    transforms: &mut Query<&mut Transform, Without<Car>>,
) {
    let (phase, pulse) = match features.map(|f| &f.0) {
        Some(f) if f.bpm.is_some() => (f.phase, f.pulse),
        // Sans tempo détecté, la tête garde une cadence tranquille plutôt que de se figer.
        _ => ((time.elapsed_secs_f64() * FALLBACK_BEATS_PER_S).fract() as f32, 0.0),
    };
    let target = head_nod_target(phase, pulse);
    let k = 1.0 - (-time.delta_secs() / NOD_SMOOTHING_S).exp();
    let nod = view.nod_rad + (target - view.nod_rad) * k;
    if let Some((entity, rest)) = driver.0.body
        && let Ok(mut body) = transforms.get_mut(entity)
    {
        // Le bassin dans le repère du parent (`rider`) : le modèle est posé à l'origine du repère, le parent est à `rider`.
        let hips_local = HIPS_MODEL - RIDER_ORIGIN_MODEL;
        *body = body_nod(rest, hips_local, nod);
    }
    // Yeux dans le repère de la voiture : le modèle regarde vers +Z, la voiture vers −Z (demi-tour autour de Y).
    let eye = rig.frame.to_car.transform_point(nodded_eye(nod));
    // En repère voiture (nez vers −z) un tangage positif autour de x lève le regard : baisser = angle négatif.
    let tilt = Quat::from_rotation_x(-(nod + LOOK_DOWN_RAD));
    let next = DriverView {
        active: true,
        eye,
        tilt,
        nod_rad: nod,
    };
    if *view != next {
        *view = next;
    }
}

/// Origine du nœud `rider` dans le repère du modèle de la voiture (m) : le parent de `rider_mesh`.
const RIDER_ORIGIN_MODEL: Vec3 = Vec3::new(0.49, 0.66, 0.15);

/// Le prochain rapport s'il diffère de l'actuel.
fn gear_after(gear: Gear, speed_mps: f32) -> Option<Gear> {
    let next = gear.next(speed_mps);
    (next != gear).then_some(next)
}

/// Un point de l'axe de la colonne de direction (le centre du volant), repère du modèle de la voiture (`steering_column`).
pub const WHEEL_CENTRE: Vec3 = Vec3::new(0.492, 0.997, 0.695);
/// Direction de cet axe, vers le conducteur (Y local du pivot `steering_pivot`, déjà unitaire à 1e-3 près).
pub const WHEEL_AXIS: Vec3 = Vec3::new(0.0, 0.312, -0.951);
/// Pivot du levier de vitesses (`pivot_gear_lever`), repère du modèle de la voiture.
pub const LEVER_PIVOT: Vec3 = Vec3::new(0.001_267, 0.550_308, 0.585_242);
/// Distance du poignet au centre du pommeau quand la main le tient (m) : la paume entoure le pommeau, le poignet reste en
/// retrait, du côté de l'épaule.
pub const GRIP_SETBACK_M: f32 = 0.11;

/// Rotation du pivot du levier pour un pommeau en (`lateral_m`, `fore_m`) : ordre XYZ de `CyberCar.ts` (`rotation.x` =
/// tangage, `rotation.z` = −roulis, car un +Z de glTF envoie le pommeau vers −X).
#[must_use]
pub fn lever_rotation(knob: (f32, f32)) -> Quat {
    let (pitch, roll) = lever_angles(knob.0, knob.1);
    Quat::from_euler(EulerRot::XYZ, pitch, 0.0, -roll)
}

/// Centre du pommeau dans le repère du modèle.
#[must_use]
pub fn knob_position(knob: (f32, f32)) -> Vec3 {
    LEVER_PIVOT + lever_rotation(knob) * Vec3::new(0.0, LEVER_LENGTH_M, 0.0)
}

/// Les yeux du conducteur, repère du modèle de la voiture (le nez vers +Z) : au-dessus des épaules, un peu en avant.
pub const EYE_MODEL: Vec3 = Vec3::new(0.49, 1.47, 0.14);
/// Le bassin, autour duquel pivote le buste quand il hoche la tête (repère du modèle).
pub const HIPS_MODEL: Vec3 = Vec3::new(0.49, 0.62, -0.05);
/// Hochement maximal du buste sur le temps fort, rad (~3°) : léger, mais on le voit à l'image.
pub const NOD_MAX_RAD: f32 = 0.055;
/// Le regard au volant est un peu baissé, rad : le volant et les mains restent dans le champ.
pub const LOOK_DOWN_RAD: f32 = 0.10;
/// Cadence de repli du hochement sans tempo détecté, battements par seconde (120 bpm).
const FALLBACK_BEATS_PER_S: f64 = 2.0;
/// Lissage du hochement : suit le temps fort sans à-coup (constante de temps, s).
const NOD_SMOOTHING_S: f32 = 0.05;

/// Les yeux du conducteur, que lit le réalisateur pour la vue au volant. Écrite par le seul [`animate_driver`].
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct DriverView {
    /// Vrai quand la voiture a un conducteur : sans lui, la vue au volant garde ses yeux génériques.
    pub active: bool,
    /// Position des yeux dans le repère de la voiture (nez vers −z), hochement compris, m.
    pub eye: Vec3,
    /// Inclinaison de la tête dans le repère de la voiture : regard baissé vers le volant, plus le hochement.
    pub tilt: Quat,
    /// Hochement actuel, rad (positif : vers l'avant et le bas), lissé.
    pub nod_rad: f32,
}

impl Default for DriverView {
    fn default() -> Self {
        Self {
            active: false,
            eye: Vec3::ZERO,
            tilt: Quat::IDENTITY,
            nod_rad: 0.0,
        }
    }
}

/// Hochement visé : la tête plonge sur le temps fort et se redresse jusqu'au suivant. `phase` : position dans la mesure
/// (0 sur le temps, 1 juste avant le suivant) ; `pulse` : le pouls du kick (0 à 1), qui y ajoute un coup sec.
#[must_use]
pub fn head_nod_target(phase: f32, pulse: f32) -> f32 {
    // `cos` redescend de 1 à 0 puis remonte : le plongeon est à 0 (le temps), le redressement lent, comme un mouvement de tête
    // qui suit la musique. Le pouls ajoute l'attaque.
    let swell = 0.5 + 0.5 * (std::f32::consts::TAU * phase).cos();
    NOD_MAX_RAD * (0.65 * swell + 0.35 * pulse.clamp(0.0, 1.0))
}

/// Les yeux au repos pivotés de `nod_rad` autour du bassin, repère du modèle.
#[must_use]
pub fn nodded_eye(nod_rad: f32) -> Vec3 {
    HIPS_MODEL + Quat::from_rotation_x(nod_rad) * (EYE_MODEL - HIPS_MODEL)
}

/// La rotation que le hochement impose au corps (autour du bassin) : `(rotation, translation)` à composer avec la pose de
/// repos du nœud `rider_mesh`, dont `hips_local` est le bassin dans le repère de son parent.
#[must_use]
pub fn body_nod(rest: Transform, hips_local: Vec3, nod_rad: f32) -> Transform {
    let r = Quat::from_rotation_x(nod_rad);
    Transform {
        translation: hips_local + r * (rest.translation - hips_local),
        rotation: r * rest.rotation,
        scale: rest.scale,
    }
}

/// `CfSet::Simulate`, après `drive_car` : le conducteur suit le volant. Ses poignets tournent avec la prise (les mains
/// restent sur la jante), ses bras se replient par cinématique inverse. Quand la vitesse change de rapport, la main droite
/// quitte la jante, pousse le levier dans sa grille et revient. Aucune allocation, aucune entité créée.
pub(crate) fn animate_driver(
    time: Res<Time>,
    features: Option<Res<AudioFeatures>>,
    mut view: ResMut<DriverView>,
    cars: Query<(&CarRig, &CarState), With<Car>>,
    mut drivers: Query<(&DriverRig, &mut Gearshift), With<Driver>>,
    mut transforms: Query<&mut Transform, Without<Car>>,
) {
    let (Ok((rig, state)), Ok((driver, mut gearshift))) = (cars.single(), drivers.single_mut()) else {
        if view.active {
            view.active = false;
        }
        return;
    };
    nod_head(&time, features.as_deref(), &mut view, rig, driver, &mut transforms);
    // Pas de vitesse mesurée avant la deuxième image : la boîte attend, le levier reste au point mort.
    if state.warm {
        gearshift.advance(state.speed_mps, time.delta_secs());
    }
    let (knob, on_lever) = gearshift.knob_and_hand();
    if let Some(mut lever) = rig.gear_lever.and_then(|e| transforms.get_mut(e).ok()) {
        lever.rotation = lever_rotation(knob);
    }
    let Some(angle) = rig
        .steering
        .and_then(|e| transforms.get(e).ok())
        .map(|t| 2.0 * t.rotation.y.atan2(t.rotation.w))
    else {
        return;
    };
    let angle = if angle > std::f32::consts::PI {
        angle - std::f32::consts::TAU
    } else {
        angle
    };
    let axis = WHEEL_AXIS.normalize();
    for (index, arm) in [driver.0.left, driver.0.right].into_iter().enumerate() {
        let Some(arm) = arm else { continue };
        let mut target = turned_grip(arm.geometry.wrist, WHEEL_CENTRE, axis, angle);
        // Seule la main droite (la plus proche du levier) le manœuvre.
        if index == 1 && on_lever > 0.0 {
            let knob = knob_position(knob);
            let grip = knob + (arm.geometry.shoulder - knob).normalize_or_zero() * GRIP_SETBACK_M;
            target = target.lerp(grip, on_lever);
        }
        let (up, low) = arm.geometry.solve(target);
        if let Ok(mut t) = transforms.get_mut(arm.up) {
            t.rotation = up;
        }
        if let Ok(mut t) = transforms.get_mut(arm.low) {
            t.rotation = low;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_tete_plonge_sur_le_temps_et_se_redresse_avant_le_suivant() {
        let on_beat = head_nod_target(0.0, 0.0);
        let mid = head_nod_target(0.5, 0.0);
        assert!(on_beat > mid, "le plongeon est sur le temps : {on_beat} contre {mid}");
        // Continu au passage d'un temps à l'autre : pas de saut quand la phase revient à 0.
        assert!((head_nod_target(0.999, 0.0) - head_nod_target(0.0, 0.0)).abs() < 1e-3);
    }

    #[test]
    fn le_hochement_est_leger_mais_visible() {
        let strongest = head_nod_target(0.0, 1.0);
        assert!(strongest <= NOD_MAX_RAD * 1.001, "borné : {strongest}");
        assert!(
            (NOD_MAX_RAD.to_degrees() - 3.15).abs() < 0.1 && head_nod_target(0.0, 0.0) > 0.03,
            "de l'ordre de 2 à 3 degrés"
        );
        // Un pouls hors bornes ne l'emballe pas.
        assert!(head_nod_target(0.0, 9.0) <= NOD_MAX_RAD * 1.001);
    }

    #[test]
    fn les_yeux_plongent_vers_l_avant_et_le_bas_autour_du_bassin() {
        let rest = nodded_eye(0.0);
        let nodded = nodded_eye(0.1);
        assert!(rest.distance(EYE_MODEL) < 1e-6);
        assert!(nodded.y < rest.y, "les yeux baissent");
        assert!(nodded.z > rest.z, "et avancent (le modèle regarde vers +z)");
        // Le bassin ne bouge pas : la distance au bassin est conservée.
        let (a, b) = (rest.distance(HIPS_MODEL), nodded.distance(HIPS_MODEL));
        assert!((a - b).abs() < 1e-5);
    }

    #[test]
    fn le_corps_pivote_autour_du_bassin_sans_se_deformer() {
        let rest = Transform::from_xyz(0.065, 0.335, 0.32).with_scale(Vec3::splat(0.67));
        let hips = HIPS_MODEL - RIDER_ORIGIN_MODEL;
        let at_rest = body_nod(rest, hips, 0.0);
        assert!(at_rest.translation.distance(rest.translation) < 1e-6);
        let tilted = body_nod(rest, hips, 0.08);
        assert_eq!(tilted.scale, rest.scale);
        // Le bassin est le point fixe : il reste au même endroit une fois la rotation appliquée.
        let before = rest.transform_point(Vec3::ZERO);
        let fixed = hips;
        let moved = |t: Transform, p: Vec3| t.transform_point(p);
        assert!((moved(rest, Vec3::ZERO) - before).length() < 1e-6);
        let local = rest.compute_affine().inverse().transform_point3(fixed);
        assert!(
            (moved(tilted, local) - fixed).length() < 1e-5,
            "le bassin reste en place"
        );
    }
}
