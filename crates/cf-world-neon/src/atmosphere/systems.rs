//! Les systèmes de l'atmosphère : composer l'éclairage de scène de l'image, le recopier dans les matériaux du monde,
//! animer les poussières.

use bevy::math::{DVec3, Vec3};
use bevy::prelude::*;
use cf_render::audio::{FilteredSignals, VisualDensity};
use cf_render::palette::{LinearRgb, NEUTRAL_INTENSITY, mood_color};
use cf_render::{
    AbsolutePosition, AudioFeatures, CameraPose, CurrentShowFx, FloatingOrigin, QualityLevers, SceneColor, SceneMood,
    ScenePalette, StreamFocus,
};

use super::dust::{Dust, DustMaterial, DustRig, dust_params, shell_shares};
use super::light::{
    FogReach, LightColors, LightInput, LightTuning, SceneLightParams, SceneLightUniform, TUBES, Tube, compose,
};
use super::modulation::{FogInput, FogModulation, FogTuning};
use crate::car::CarState;
use crate::landmarks::CityField;
use crate::road::RoadLook;
use crate::sky::SkyLook;

/// Ce que le spectacle demande à l'atmosphère, en plus du son. Écrit par les événements (apnée d'EV4, chantier C7).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct AtmosphereLook {
    /// Apnée : la brume se retire en une mesure.
    pub apnea: bool,
}

/// Les réglages de l'atmosphère (règle 6 du plan : pas de nombres épars).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct AtmosphereTuning {
    /// La brume.
    pub fog: FogTuning,
    /// L'éclairage de scène.
    pub light: LightTuning,
}

/// Un matériau du monde qui porte le bloc `SceneLight` (`@binding(9)`, `atmosphere.wesl`).
pub trait SceneLit: bevy::asset::Asset {
    /// Le bloc du matériau.
    fn scene_light(&self) -> &SceneLightParams;
    /// Remplace le bloc du matériau.
    fn set_scene_light(&mut self, params: SceneLightParams);
}

/// Déclare [`SceneLit`] pour un matériau dont le champ s'appelle `scene`.
macro_rules! scene_lit {
    ($($material:ty),+ $(,)?) => {$(
        impl $crate::atmosphere::SceneLit for $material {
            fn scene_light(&self) -> &$crate::atmosphere::SceneLightParams {
                &self.scene
            }
            fn set_scene_light(&mut self, params: $crate::atmosphere::SceneLightParams) {
                self.scene = params;
            }
        }
    )+};
}
pub(crate) use scene_lit;

fn vec(c: LinearRgb) -> Vec3 {
    Vec3::new(c.r, c.g, c.b)
}

/// Ce que l'atmosphère lit du point suivi : sa position absolue et, si c'est la voiture, son état.
type Focus = (&'static AbsolutePosition, Option<&'static CarState>);

/// `CfSet::Simulate`, après la météo : avance la brume et compose l'éclairage de scène de l'image.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(super) fn update_scene_light(
    time: Res<Time<Real>>,
    tuning: Res<AtmosphereTuning>,
    look: Res<AtmosphereLook>,
    features: Option<Res<AudioFeatures>>,
    signals: Option<Res<FilteredSignals>>,
    density: Option<Res<VisualDensity>>,
    show_fx: Option<Res<CurrentShowFx>>,
    road: Option<Res<RoadLook>>,
    palette: Option<Res<ScenePalette>>,
    mood: Option<Res<SceneMood>>,
    origin: Option<Res<FloatingOrigin>>,
    field: Option<Res<CityField>>,
    focus: Query<Focus, With<StreamFocus>>,
    mut modulation: Local<FogModulation>,
    mut uniform: ResMut<SceneLightUniform>,
    mut reach: ResMut<FogReach>,
) {
    let (Some(palette), Some(origin)) = (palette, origin) else {
        return;
    };
    let f = features.as_deref().map(|f| &f.0);
    let fx = show_fx.as_deref().map(|s| &s.0);
    let fog = modulation.step(
        &tuning.fog,
        &FogInput {
            dt_s: time.delta_secs(),
            now_s: time.elapsed_secs_f64(),
            energy_slow: f.map_or(0.5, |f| f.intensity),
            energy_fast: f.map_or(0.5, |f| f.loud),
            low_mid: f.map_or(0.0, |f| f.mid),
            rain: road.as_deref().map_or(0.0, |r| r.rain),
            wet: road.as_deref().map_or(0.0, |r| r.wet),
            drop: fx.is_some_and(|s| s.drop.is_some()),
            beat_s: f.and_then(|f| f.bpm).map_or(0.5, |bpm| 60.0 / bpm.max(30.0)),
            apnea: look.apnea,
            flashes: fx.is_none_or(|s| s.flashes),
            breathing: fx.is_some_and(|s| s.breathing),
        },
    );
    reach.set_if_neq(FogReach(fog.reach_m));

    let mood = mood.map_or_else(|| mood_color(&palette, NEUTRAL_INTENSITY), |m| m.color);
    let colors = LightColors {
        velvet: vec(palette.get(SceneColor::Velvet)),
        mood: vec(mood),
        ice: vec(palette.get(SceneColor::Ice)),
        azure: vec(palette.get(SceneColor::Azure)),
    };
    let source = focus.single().map_or(origin.origin(), |(position, _)| position.0);
    // Les tubes proches : le sujet (ses phares, sa lueur), puis les lumières des immeubles-vitrines.
    let mut tubes = [Tube::default(); TUBES];
    tubes[0] = Tube {
        position: source + DVec3::new(0.0, 0.8, -3.0),
        color: colors.mood.lerp(colors.ice, 0.4),
    };
    if let Some(field) = field.as_deref() {
        let lit = field.state.lights.iter().filter(|l| l.color != Vec3::ZERO);
        for (tube, light) in tubes.iter_mut().skip(1).zip(lit) {
            *tube = Tube {
                position: light.position,
                color: light.color,
            };
        }
    }
    let light = compose(
        &tuning.light,
        &colors,
        &LightInput {
            density: density.map_or(0.5, |d| d.0.delta),
            fog,
            kick_ages: signals.map_or([60.0; 4], |s| s.kick_wave.ages()),
            source,
            tubes,
        },
    );
    uniform.set_if_neq(SceneLightUniform(light.params(origin.origin(), tuning.light.tube_gain)));
}

/// `CfSet::Present` : recopie le bloc de l'image dans chaque matériau `M` du monde. Un matériau déjà à jour n'est pas
/// touché (Bevy ne re-prépare pas un asset qu'on n'a pas écrit).
pub(super) fn sync_scene_light<M: SceneLit>(uniform: Res<SceneLightUniform>, materials: Option<ResMut<Assets<M>>>) {
    let Some(mut materials) = materials else {
        return;
    };
    if materials.iter().all(|(_, m)| *m.scene_light() == uniform.0) {
        return;
    }
    for (_, material) in materials.iter_mut() {
        material.set_scene_light(uniform.0);
    }
}

/// L'entité des poussières (jamais le point suivi).
type DustEntity = (With<Dust>, Without<StreamFocus>);

/// `CfSet::Present` : les poussières suivent la caméra, leur densité suit la qualité adaptative.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(super) fn update_dust(
    time: Res<Time>,
    tuning: Res<AtmosphereTuning>,
    pose: Option<Res<CameraPose>>,
    levers: Option<Res<QualityLevers>>,
    sky: Option<Res<SkyLook>>,
    features: Option<Res<AudioFeatures>>,
    show_fx: Option<Res<CurrentShowFx>>,
    focus: Query<Focus, With<StreamFocus>>,
    rig: Res<DustRig>,
    mut materials: ResMut<Assets<DustMaterial>>,
    mut dust: Query<(&mut AbsolutePosition, &mut Visibility), DustEntity>,
) {
    let shares = shell_shares(levers.map_or(1.0, |l| l.particle_density), sky.is_some_and(|s| s.calm));
    let visible = shares.iter().any(|s| *s > 0.0) && pose.is_some();
    for (mut position, mut visibility) in &mut dust {
        if let Some(pose) = pose.as_deref() {
            position.set_if_neq(AbsolutePosition(pose.position));
        }
        visibility.set_if_neq(if visible { Visibility::Inherited } else { Visibility::Hidden });
    }
    if !visible {
        return;
    }
    let speed = focus.single().ok().and_then(|(_, car)| car).map_or(0.0, |c| c.speed_mps);
    let f = features.as_deref().map(|f| &f.0);
    let quiet = show_fx.is_some_and(|s| s.0.breathing || !s.0.flashes);
    let wind = tuning.fog.wind_mps * (1.0 + 2.0 * f.map_or(0.0, |f| f.mid.clamp(0.0, 1.0)));
    // En respiration, ni scintillement ni éclat sur les transitoires.
    let transient = if quiet { 0.0 } else { f.map_or(0.0, |f| f.transient) };
    let params = dust_params(time.elapsed_secs_f64(), wind, transient, speed, shares, !quiet);
    if materials.get(&rig.material).is_some_and(|m| m.params == params) {
        return;
    }
    if let Some(mut material) = materials.get_mut(&rig.material) {
        material.params = params;
    }
}
