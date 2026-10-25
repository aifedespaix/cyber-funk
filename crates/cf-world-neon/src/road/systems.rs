//! Les deux systèmes de la route : suivre le point suivi, remplir le matériau.

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec4};
use bevy::prelude::*;
use cf_render::{AbsolutePosition, AudioFeatures, FloatingOrigin, StreamFocus};

use super::material::RoadParams;
use super::{AHEAD_M, Road, RoadLook, RoadMaterial};
use crate::landmarks::field::{CityField, FieldParams};

/// Période, en s, au bout de laquelle le temps du shader repart de zéro : 400 π s (~21 min).
///
/// Le temps est envoyé en `f32` : sans borne, `sin(t × 0,35)` perdrait sa précision au fil d'un set de plusieurs heures.
/// La période est choisie pour que **toutes** les teintes du shader reviennent à l'identique : `prism` mélange
/// `sin(t)` et `sin(1,7 t + 2,1)`, qui ne reviennent ensemble que tous les 2π × 10 en `t` ; avec `t = 0,35 × temps`
/// (liserés) et `t = temps` (joints), 400 π s est un multiple des deux. Seules les gouttes de pluie, dont la phase est
/// aléatoire par cellule, repartent d'un coup, comme les bruits au recentrage d'origine.
const TIME_WRAP_S: f64 = 400.0 * std::f64::consts::PI;

/// Fréquence spatiale (rad/m) de la teinte des liserés (`p.y * 0.035` dans le shader).
const EDGE_HUE_PER_M: f64 = 0.035;
/// Fréquence spatiale (rad/m) de la teinte des joints (`p.y * 0.05` dans le shader).
const SEAM_HUE_PER_M: f64 = 0.05;

/// Période spatiale (m) d'une teinte `prism` de fréquence `per_m` : `sin(t)` et `sin(1,7 t + 2,1)` y reviennent ensemble
/// (2π × 10 en `t`).
fn hue_period_m(per_m: f64) -> f64 {
    std::f64::consts::TAU * 10.0 / per_m
}

/// Position de l'origine (m) modulo la période de la teinte : ajoutée à la coordonnée relative, elle redonne la teinte
/// de la coordonnée absolue. Calculée en `f64` : à 400 km, `z × 0,035` vaut 14 000 rad.
fn origin_phase_m(origin_z: f64, per_m: f64) -> f32 {
    origin_z.rem_euclid(hue_period_m(per_m)) as f32
}

/// Intensité de départ de Sillage (`SectionTracker.intensity = 0.4`), tant qu'aucune analyse n'est là.
const DEFAULT_INTENSITY: f32 = 0.4;

/// Ce que le shader lit de l'image : temps, voiture et musique.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameInputs {
    /// Temps écoulé, en s.
    pub time_s: f64,
    /// La voiture, en repère rendu (relatif à l'origine flottante), en m.
    pub car_render: Vec3,
    /// Pouls du kick, 0 à 1 (`b.pulse`).
    pub pulse: f32,
    /// Énergie des basses, 0 à 1 (`b.bass`).
    pub bass: f32,
    /// Intensité du morceau, 0 (calme) à 1 (sommet) : teinte d'ambiance.
    pub intensity: f32,
    /// Position z (absolue, m) de l'origine flottante : la teinte des liserés et des joints en garde la phase.
    pub origin_z: f64,
}

fn vec4(color: LinearRgba) -> Vec4 {
    Vec4::new(color.red, color.green, color.blue, 1.0)
}

/// Les uniformes de l'image (`Road.update` de Sillage, plus le bloc de palette).
#[must_use]
pub fn build_params(look: &RoadLook, frame: &FrameInputs) -> RoadParams {
    let p = &look.palette;
    RoadParams {
        ink: vec4(p.ink),
        mint: vec4(p.mint),
        azure: vec4(p.azure),
        violet: vec4(p.violet),
        rose: vec4(p.rose),
        ice: vec4(p.ice),
        glow: vec4(look.glow),
        mood: vec4(p.mood(frame.intensity)),
        car: frame.car_render.extend(0.0),
        frame: Vec4::new(
            (frame.time_s % TIME_WRAP_S) as f32,
            frame.pulse,
            frame.bass,
            look.brake.clamp(0.0, 1.0),
        ),
        look: Vec4::new(
            look.draw.clamp(0.0, 1.0),
            look.wet.clamp(0.0, 1.0),
            look.rain.clamp(0.0, 1.0),
            0.0,
        ),
        phase: Vec4::new(
            origin_phase_m(frame.origin_z, EDGE_HUE_PER_M),
            origin_phase_m(frame.origin_z, SEAM_HUE_PER_M),
            0.0,
            0.0,
        ),
    }
}

/// `CfSet::Simulate` : le plan suit le point suivi en profondeur, 300 m devant lui ; la route paraît infinie.
pub(super) fn follow_focus(
    focus: Query<&AbsolutePosition, (With<StreamFocus>, Without<Road>)>,
    mut road: Query<&mut AbsolutePosition, With<Road>>,
) {
    let Ok(focus) = focus.single() else { return };
    let target = DVec3::new(0.0, 0.0, focus.0.z - AHEAD_M);
    for mut position in &mut road {
        // `set_if_neq` : un point suivi immobile ne réécrit pas le `Transform` de la route.
        position.set_if_neq(AbsolutePosition(target));
    }
}

/// `CfSet::Present` : remplit le matériau (voiture, temps, pouls, basses, teinte). Le matériau n'est touché que si
/// quelque chose a changé : Bevy ne re-prépare pas un asset qu'on n'a pas écrit.
pub(super) fn update_material(
    time: Res<Time<Real>>,
    features: Option<Res<AudioFeatures>>,
    look: Res<RoadLook>,
    origin: Res<FloatingOrigin>,
    focus: Query<&AbsolutePosition, With<StreamFocus>>,
    road: Query<&MeshMaterial3d<RoadMaterial>, With<Road>>,
    mut materials: ResMut<Assets<RoadMaterial>>,
    field: Option<Res<CityField>>,
) {
    let (pulse, bass, intensity) = features
        .as_deref()
        .map_or((0.0, 0.0, DEFAULT_INTENSITY), |f| (f.0.pulse, f.0.bass, f.0.intensity));
    let car_render = focus.single().map_or(Vec3::ZERO, |car| origin.to_render(car.0));
    let params = build_params(
        &look,
        &FrameInputs {
            time_s: look.clock_s.unwrap_or_else(|| time.elapsed_secs_f64()),
            car_render,
            pulse,
            bass,
            intensity,
            origin_z: origin.origin().z,
        },
    );
    // Le champ de la ville (J7-08) : lumières, zone de glitch et décalque des immeubles-vitrines, en repère rendu.
    let (field_params, decal) = field.as_deref().map_or((FieldParams::default(), None), |field| {
        (field.params(origin.origin()), field.decal_map.clone())
    });
    for handle in &road {
        if materials
            .get(&handle.0)
            .is_some_and(|m| m.params == params && m.field == field_params && m.decal == decal)
        {
            continue;
        }
        if let Some(mut material) = materials.get_mut(&handle.0) {
            material.params = params;
            material.field = field_params;
            material.decal.clone_from(&decal);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> FrameInputs {
        FrameInputs {
            time_s: 12.5,
            car_render: Vec3::new(0.0, 0.0, -1234.5),
            pulse: 0.6,
            bass: 0.3,
            intensity: 0.5,
            origin_z: 0.0,
        }
    }

    #[test]
    fn les_uniformes_portent_l_image() {
        let look = RoadLook {
            brake: 2.0,
            wet: 0.7,
            ..RoadLook::default()
        };
        let params = build_params(&look, &frame());
        assert!((params.frame.x - 12.5).abs() < 1e-6);
        assert!((params.frame.y - 0.6).abs() < 1e-6 && (params.frame.z - 0.3).abs() < 1e-6);
        assert!((params.frame.w - 1.0).abs() < 1e-6, "le freinage est borné à 1");
        assert!((params.car.z + 1234.5).abs() < 1e-3);
        assert!((params.look.y - 0.7).abs() < 1e-6);
        // Intensité 0,5 : la teinte d'ambiance est le violet du groove.
        assert!((params.mood.x - look.palette.violet.red).abs() < 1e-6);
    }

    /// `prism` : les deux sinus du mélange de teintes, à la position `z` et au temps `time_s`.
    fn prism_terms(argument: f64) -> (f64, f64) {
        (argument.sin(), (argument * 1.7 + 2.1).sin())
    }

    /// La teinte d'un liséré ou d'un joint au point `z` : calculée dans le repère rendu avec la phase de l'origine, elle
    /// doit redonner celle de la coordonnée absolue, pour les deux sinus de `prism`, quelle que soit l'origine.
    #[test]
    fn la_teinte_est_continue_au_recentrage() {
        for k in 0..=14 {
            let origin_z = -f64::from(k) * 28_800.0;
            let params = build_params(&RoadLook::default(), &FrameInputs { origin_z, ..frame() });
            let time = 123.4;
            for absolute in [-14_000.0_f64, -403_321.7, -1.5, 77.0] {
                let render = f64::from((absolute - origin_z) as f32);
                // Liserés : `prism((p.y + phase) * 0.035 + time * 0.35)`.
                let seen = prism_terms((render + f64::from(params.phase.x)) * 0.035 + time * 0.35);
                let expected = prism_terms(absolute * 0.035 + time * 0.35);
                assert!(
                    (seen.0 - expected.0).abs() < 2e-3 && (seen.1 - expected.1).abs() < 2e-3,
                    "liseré, origine {origin_z}, z {absolute}"
                );
                // Joints : `prism((p.y + phase) * 0.05 - time)`.
                let seen = prism_terms((render + f64::from(params.phase.y)) * 0.05 - time);
                let expected = prism_terms(absolute * 0.05 - time);
                assert!(
                    (seen.0 - expected.0).abs() < 2e-3 && (seen.1 - expected.1).abs() < 2e-3,
                    "joint, origine {origin_z}, z {absolute}"
                );
            }
        }
        // Origine nulle : la phase est nulle, le rendu est celui de Sillage.
        let near_start = build_params(&RoadLook::default(), &frame());
        assert!(near_start.phase.x.abs() < 1e-9 && near_start.phase.y.abs() < 1e-9);
    }

    /// Le temps repart de zéro sans saut de teinte : 400 π s est un multiple de la période des deux teintes.
    #[test]
    fn le_temps_du_shader_repart_sans_sauter_sur_la_teinte() {
        let mut inputs = frame();
        inputs.time_s = TIME_WRAP_S + 3.0;
        let wrapped = build_params(&RoadLook::default(), &inputs).frame.x;
        assert!((wrapped - 3.0).abs() < 1e-3, "{wrapped}");
        for (scale, name) in [(0.35, "liserés"), (1.0, "joints")] {
            let turns = TIME_WRAP_S * scale / (std::f64::consts::TAU * 10.0);
            assert!((turns - turns.round()).abs() < 1e-9, "{name} : {turns} périodes");
        }
    }

    #[test]
    fn la_route_suit_le_point_suivi_300_m_devant() {
        let mut app = App::new();
        app.add_systems(Update, follow_focus);
        app.world_mut()
            .spawn((StreamFocus, AbsolutePosition(DVec3::new(2.0, 1.0, -50_000.0))));
        let road = app.world_mut().spawn((Road, AbsolutePosition::default())).id();
        app.update();
        let position = app.world().get::<AbsolutePosition>(road).map(|p| p.0);
        assert_eq!(position, Some(DVec3::new(0.0, 0.0, -50_300.0)));
    }
}
