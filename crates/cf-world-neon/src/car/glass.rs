//! Les vitres de la voiture sous la pluie : le verre du modèle devient une extension de `StandardMaterial` dont le shader
//! (`assets/shaders/cf-world-neon/car_glass.wesl`) dessine gouttes posées, gouttes qui coulent et film d'eau.
//!
//! Quand la caméra est au volant, la pluie ne tombe donc plus « sur l'objectif » (voir `weather::step_weather`) : elle est
//! sur le pare-brise et les vitres, que l'on voit à travers. Vue de l'extérieur, les mêmes gouttes se voient sur le verre.
//!
//! Le motif ne dépend d'aucune UV : il est calculé sur le plan de la vitre (voir le shader), donc le pare-brise, les
//! vitres latérales et la lunette de toutes les voitures en profitent, qu'elles aient ou non une texture de gouttes
//! (`rain_glass` du GLB de la `CyberCar`, laissé invisible).

use bevy::asset::{Asset, Assets, Handle, uuid_handle};
use bevy::math::Vec4;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, StandardMaterial};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::{Shader, ShaderRef};

use super::rig::{Car, CarRig, CarState};
use super::systems::CRUISE_SPEED_MPS;
use crate::RoadLook;

const GLASS_SHADER: Handle<Shader> = uuid_handle!("5c9e1d73-2a84-4b60-9f17-e3a8d0b46c25");
const GLASS_SOURCE: &str = include_str!("../../../../assets/shaders/cf-world-neon/car_glass.wesl");
const GLASS_PATH: &str = "cf-world-neon/car_glass.wesl";

/// Au-delà de cette période (s), l'horloge des gouttes repart de zéro : elle reste petite, donc précise (le motif saute une
/// fois par heure, ce qu'une goutte qui recommence son cycle masque).
const TIME_WRAP_S: f64 = 3600.0;

/// Enregistre le shader des vitres dans `Assets<Shader>`.
pub(crate) fn embed_shader(shaders: &mut Assets<Shader>) {
    if let Err(error) = shaders.insert(GLASS_SHADER.id(), Shader::from_wesl(GLASS_SOURCE, GLASS_PATH)) {
        // Impossible avec un identifiant UUID ; sinon les vitres resteraient sèches plutôt que de couper le show.
        tracing::error!(%error, "shader des vitres non enregistré");
    }
}

/// Les uniformes du shader des vitres.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct GlassParams {
    /// `x` : intensité de l'averse, `y` : temps (s), `z` : vitesse rapportée à la croisière, `w` : non utilisé.
    pub rain: Vec4,
}

/// Extension de `StandardMaterial` : les gouttes sur la vitre.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct CarGlassExt {
    /// Uniformes envoyés au shader.
    #[uniform(100)]
    pub params: GlassParams,
}

impl MaterialExtension for CarGlassExt {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(GLASS_SHADER)
    }
}

/// Le matériau des vitres.
pub type CarGlassMaterial = ExtendedMaterial<StandardMaterial, CarGlassExt>;

/// L'intensité des gouttes sur le verre : la pluie qui tombe, et un reste d'humidité quand elle s'arrête (le verre met du
/// temps à sécher, la chaussée aussi).
#[must_use]
pub fn glass_rain(rain: f32, wet: f32) -> f32 {
    rain.max(wet * 0.5).clamp(0.0, 1.0)
}

/// `CfSet::Simulate`, après `advance_look` : règle l'averse, l'horloge et la vitesse des vitres de la voiture. N'écrit le
/// matériau que si une valeur change (au sec, jamais).
pub(super) fn update_glass(
    time: Res<Time>,
    road: Res<RoadLook>,
    cars: Query<(&CarRig, &CarState), With<Car>>,
    mut glass: ResMut<Assets<CarGlassMaterial>>,
) {
    let Ok((rig, state)) = cars.single() else { return };
    let Some(handle) = &rig.materials.glass else { return };
    let amount = glass_rain(road.rain, road.wet);
    let speed = (state.speed_mps / CRUISE_SPEED_MPS).clamp(0.0, 1.0);
    // Au sec, l'horloge n'avance pas pour le shader : rien à réécrire image après image.
    let clock = if amount > 0.01 {
        (time.elapsed_secs_f64() % TIME_WRAP_S) as f32
    } else {
        0.0
    };
    let wanted = GlassParams {
        rain: Vec4::new(if amount > 0.01 { amount } else { 0.0 }, clock, speed, 0.0),
    };
    if glass.get(handle).is_some_and(|m| m.extension.params == wanted) {
        return;
    }
    if let Some(mut material) = glass.get_mut(handle) {
        material.extension.params = wanted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_vitre_reste_mouillee_un_peu_apres_l_averse() {
        assert!(glass_rain(0.0, 0.0).abs() < f32::EPSILON);
        assert!((glass_rain(0.8, 0.6) - 0.8).abs() < 1e-6, "la pluie qui tombe domine");
        assert!((glass_rain(0.0, 0.8) - 0.4).abs() < 1e-6, "reste d'humidité");
        assert!((glass_rain(3.0, 0.0) - 1.0).abs() < f32::EPSILON, "bornée");
    }
}
