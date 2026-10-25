//! Les systèmes de la météo : le cycle des averses (`CfSet::Simulate`), puis ce que le rendu en montre (`CfSet::Present`,
//! après le réalisateur : la caméra est celle de l'image).
//!
//! Qui écrit quoi : [`step_weather`] écrit seul [`WeatherState`], et dans `RoadLook` l'humidité et la pluie (les façades
//! les lisent aussi) ainsi que `LensWeather` (gouttes sur l'objectif, post-process). Les matériaux ne sont réécrits que
//! si leurs uniformes changent (Bevy ne re-prépare pas un asset qu'on n'a pas touché).

use bevy::math::Vec4;
use bevy::prelude::*;
use cf_render::postfx::LensWeather;
use cf_render::{
    AbsolutePosition, AudioFeatures, CameraPose, CurrentShow, CurrentShowFx, FloatingOrigin, QualityLevers, SceneMood,
    ScenePalette, StreamFocus,
};

use super::logic::Weather;
use super::mist::{Mist, MistMaterial, MistRig, mist_params, mist_position};
use super::rain::{AHEAD_FRACTION, BOX_M, Rain, RainMaterial, RainParams, RainRig, TIME_WRAP_S};
use super::storm::Lightning;
use crate::car::{Car, CarState};
use crate::landmarks::CityField;
use crate::road::RoadLook;
use crate::sky::SkyLook;

/// Vitesse de croisière (m/s) : rapport de vitesse des gouttes de l'objectif.
const CRUISE_MPS: f32 = 40.0;
/// Densité de pluie en mode calme (`calm ? 0.45` de `sillage-scene.ts`).
pub const CALM_RAIN_DENSITY: f32 = 0.45;

/// Réglages de la météo.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeatherSettings {
    /// Les averses sont permises (`config.rain` de Sillage). Faux : l'averse en cours s'achève, aucune ne vient.
    pub enabled: bool,
    /// Graine des averses.
    pub seed: u64,
}

impl Default for WeatherSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            seed: 0x7a1_5eed,
        }
    }
}

/// L'état de la météo : le cycle des averses et la voie des éclairs d'orage. Écrit par le seul [`step_weather`].
#[derive(Resource, Debug, Clone)]
pub struct WeatherState {
    /// Le cycle des averses.
    pub weather: Weather,
    /// La voie des éclairs d'orage (règle 7).
    pub lightning: Lightning,
}

impl WeatherState {
    /// Un état neuf dont les averses partent de `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            weather: Weather::new(seed),
            lightning: Lightning::default(),
        }
    }
}

impl Default for WeatherState {
    fn default() -> Self {
        Self::new(WeatherSettings::default().seed)
    }
}

/// Le tonnerre gronde (une image) : le son s'y accroche (`sfx.thunder` de Sillage).
#[derive(Message, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Thunder;

/// L'étape où la météo avance (`CfSet::Simulate`).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WeatherSet;

/// Densité de gouttes voulue : l'intensité de l'averse, réduite par la qualité adaptative (`rain_density`) ou le mode calme.
#[must_use]
pub fn rain_density(rain: f32, levers: Option<&QualityLevers>, calm: bool) -> f32 {
    let lever = levers.map_or(1.0, |l| l.rain_density);
    let lever = if calm { lever.min(CALM_RAIN_DENSITY) } else { lever };
    let density = rain * lever;
    if density.is_finite() {
        density.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Inclinaison de la pluie vers l'arrière selon la vitesse de la voiture, m/s (`uTilt` de `Rain.update`).
#[must_use]
pub fn rain_tilt(speed_mps: f32) -> f32 {
    0.12 + (speed_mps / 22.0).clamp(0.0, 1.8) * 0.35
}

/// Écrit un matériau seulement s'il a changé.
fn write<M: Asset + PartialEq>(assets: &mut Assets<M>, handle: &Handle<M>, value: M) {
    if assets.get(handle).is_some_and(|m| *m == value) {
        return;
    }
    if let Some(mut material) = assets.get_mut(handle) {
        *material = value;
    }
}

/// `CfSet::Simulate`, après les tronçons : avance les averses d'après le niveau du spectacle, éclaire les nuages sur le
/// tonnerre (par le limiteur photosensible), et publie humidité et pluie pour la route, les façades et l'objectif.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(super) fn step_weather(
    time: Res<Time>,
    settings: Res<WeatherSettings>,
    show: Option<Res<CurrentShow>>,
    show_fx: Option<Res<CurrentShowFx>>,
    sky: Option<Res<SkyLook>>,
    car: Query<&CarState, With<Car>>,
    mut state: ResMut<WeatherState>,
    mut road: ResMut<RoadLook>,
    mut lens: ResMut<LensWeather>,
    mut thunder: MessageWriter<Thunder>,
) {
    let dt = time.delta_secs();
    let state = &mut *state;
    state.weather.set_enabled(settings.enabled);
    state.weather.update(dt, show.map(|s| s.0.level));
    let policy = show_fx.map(|s| (s.0.flashes, s.0.breathing));
    let rumble = state.weather.thunder;
    if rumble {
        thunder.write(Thunder);
    }
    // Pas d'éclair sans le limiteur : flashs coupés, respiration ou sans spectacle, il reste le tonnerre sonore.
    state.lightning.update(dt, time.elapsed_secs_f64(), rumble, policy);

    road.wet = state.weather.wet;
    road.rain = state.weather.rain;
    // Mode calme (`reducedMotion`) : l'objectif reste sec (`setLens` de `postfx.ts`).
    let calm = sky.is_some_and(|s| s.calm);
    lens.rain = if calm { 0.0 } else { state.weather.rain };
    lens.speed_ratio = car.single().map_or(0.0, |c| (c.speed_mps / CRUISE_MPS).clamp(0.0, 1.0));
}

/// `CfSet::Simulate`, après les tronçons : le plan de brume suit la voiture (comme la route).
pub(super) fn follow_mist(
    focus: Query<&AbsolutePosition, (With<StreamFocus>, Without<Mist>)>,
    mut mist: Query<&mut AbsolutePosition, With<Mist>>,
) {
    let Ok(focus) = focus.single() else { return };
    for mut position in &mut mist {
        position.set_if_neq(AbsolutePosition(mist_position(focus.0)));
    }
}

/// `CfSet::Present` : remplit le matériau de la brume (palette, pouls, éclair accordé, qualité).
pub(super) fn update_mist(
    time: Res<Time>,
    palette: Res<ScenePalette>,
    mood: Option<Res<SceneMood>>,
    features: Option<Res<AudioFeatures>>,
    levers: Option<Res<QualityLevers>>,
    state: Res<WeatherState>,
    rig: Res<MistRig>,
    mut materials: ResMut<Assets<MistMaterial>>,
) {
    let mood = mood.map_or_else(
        || cf_render::palette::mood_color(&palette, cf_render::palette::NEUTRAL_INTENSITY),
        |m| m.color,
    );
    let pulse = features.map_or(0.0, |f| f.0.pulse);
    // Le bruit allégé suit la densité de particules de la qualité adaptative.
    let cheap = levers.is_some_and(|l| l.particle_density < 1.0);
    let params = mist_params(
        &palette,
        mood,
        time.elapsed_secs_f64(),
        pulse,
        state.lightning.level(),
        cheap,
    );
    write(&mut materials, &rig.material, MistMaterial { params });
}

/// `CfSet::Present` : remplit le matériau de la pluie, place la boîte devant la caméra et la cache sans averse.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(super) fn update_rain(
    time: Res<Time>,
    state: Res<WeatherState>,
    pose: Option<Res<CameraPose>>,
    origin: Res<FloatingOrigin>,
    palette: Res<ScenePalette>,
    mood: Option<Res<SceneMood>>,
    levers: Option<Res<QualityLevers>>,
    sky: Option<Res<SkyLook>>,
    field: Option<Res<CityField>>,
    car: Query<(&AbsolutePosition, Option<&CarState>), With<StreamFocus>>,
    rig: Res<RainRig>,
    mut materials: ResMut<Assets<RainMaterial>>,
    mut rain: Query<(&mut AbsolutePosition, &mut Visibility), (With<Rain>, Without<StreamFocus>)>,
) {
    let calm = sky.is_some_and(|s| s.calm);
    let density = rain_density(state.weather.rain, levers.as_deref(), calm);
    let visible = density > 0.01;
    let center = pose.map(|p| {
        let c = p.position;
        bevy::math::DVec3::new(c.x, 0.0, c.z - f64::from(BOX_M.z * AHEAD_FRACTION))
    });
    for (mut position, mut visibility) in &mut rain {
        if let Some(center) = center {
            position.set_if_neq(AbsolutePosition(center));
        }
        let wanted = if visible && center.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        visibility.set_if_neq(wanted);
    }
    if !visible {
        return;
    }
    let (car_render, speed) = car.single().map_or((Vec3::ZERO, 0.0), |(position, state)| {
        (origin.to_render(position.0), state.map_or(0.0, |s| s.speed_mps))
    });
    let mood = mood.map_or_else(
        || cf_render::palette::mood_color(&palette, cf_render::palette::NEUTRAL_INTENSITY),
        |m| m.color,
    );
    let ice = palette.get(cf_render::palette::SceneColor::Ice);
    let rgb = |c: cf_render::palette::LinearRgb| Vec4::new(c.r, c.g, c.b, 1.0);
    let params = RainParams {
        car: car_render.extend(density),
        frame: Vec4::new(
            (time.elapsed_secs_f64() % TIME_WRAP_S) as f32,
            rain_tilt(speed),
            0.0,
            0.0,
        ),
        box_size: BOX_M.extend(0.0),
        ice: rgb(ice),
        mood: rgb(mood),
    };
    let field = field.as_deref().map(|f| f.params(origin.origin())).unwrap_or_default();
    write(&mut materials, &rig.material, RainMaterial { params, field });
}

#[cfg(test)]
mod tests {
    use cf_render::MAX_LEVEL;
    use cf_render::quality::levers_for;

    use super::*;

    #[test]
    fn la_densite_suit_l_averse_et_la_qualite_adaptative() {
        let full = levers_for(0);
        let worst = levers_for(MAX_LEVEL);
        assert!((rain_density(1.0, Some(&full), false) - 1.0).abs() < 1e-6);
        assert!(
            (rain_density(1.0, Some(&worst), false) - 0.5).abs() < 1e-6,
            "pire cran : moitié des gouttes"
        );
        assert!((rain_density(0.5, Some(&worst), false) - 0.25).abs() < 1e-6);
        assert!(
            (rain_density(1.0, None, false) - 1.0).abs() < 1e-6,
            "sans qualité : plein"
        );
        assert!(
            (rain_density(1.0, Some(&full), true) - CALM_RAIN_DENSITY).abs() < 1e-6,
            "mode calme"
        );
        assert!(rain_density(f32::NAN, Some(&full), false) == 0.0);
    }

    #[test]
    fn la_densite_decroit_avec_chaque_cran_de_qualite() {
        let densities: Vec<f32> = (0..=MAX_LEVEL)
            .map(|l| rain_density(1.0, Some(&levers_for(l)), false))
            .collect();
        assert!(densities.windows(2).all(|w| w[1] <= w[0]), "{densities:?}");
        assert!(densities[MAX_LEVEL] < densities[0], "le dernier cran allège la pluie");
    }

    #[test]
    fn l_inclinaison_croit_avec_la_vitesse_et_se_borne() {
        assert!((rain_tilt(0.0) - 0.12).abs() < 1e-6);
        assert!(rain_tilt(22.0) > rain_tilt(5.0));
        assert!((rain_tilt(10_000.0) - (0.12 + 1.8 * 0.35)).abs() < 1e-6);
    }
}
