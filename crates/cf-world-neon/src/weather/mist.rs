//! La brume (`mist.ts` de Sillage) : une nappe de brouillard posée sous la route, qui cache le pied des tours et donne la
//! sensation que la route flotte très haut au-dessus du vide. Un seul plan de [`SIZE_M`] m de côté qui suit la voiture ;
//! le motif est calculé en coordonnées monde par le shader (`assets/shaders/cf-world-neon/weather_mist.wesl`).
//!
//! # Culling (règle 6 de `CLAUDE.md`)
//!
//! Pas de `NoFrustumCulling` (Sillage mettait `frustumCulled = false`) : le plan est un maillage statique centré sur son
//! origine, son `Aabb` calculé par Bevy est exact (1 500 m × 1 500 m, épaisseur nulle). Il suit la voiture par sa
//! `AbsolutePosition`, comme la route : quand la caméra regarde vers le ciel, le plan sort du champ et n'est pas dessiné.
//!
//! # Qualité adaptative
//!
//! Dès que la qualité retire des particules (`QualityLevers::particle_density` < 1), le shader n'évalue plus qu'une couche
//! de bruit à trois octaves au lieu de deux couches à quatre : le plan couvre l'écran entier, son coût est celui du bruit.

use bevy::asset::{Asset, Assets, Handle, uuid_handle};
use bevy::math::Vec4;
use bevy::pbr::Material;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::{Shader, ShaderRef};
use cf_render::palette::{LinearRgb, SceneColor};
use cf_render::{AbsolutePosition, ScenePalette, WorldLayer};

use crate::meteors::MIST_Y;

/// Côté du plan (m) : `SIZE` de `mist.ts`.
pub const SIZE_M: f32 = 1500.0;
/// Part du plan qui est devant la voiture : son centre est à `0,3 × SIZE_M` devant elle.
pub const AHEAD_FRACTION: f64 = 0.3;
/// Au-delà de cette période (s), l'horloge de la dérive repart de zéro : `temps × 0,02` reste petit, donc précis, même
/// après six heures. La brume dérive de quelques mètres par seconde et se répète : un saut de motif s'y confond.
pub const TIME_WRAP_S: f64 = 3600.0;

const MIST_SHADER: Handle<Shader> = uuid_handle!("6d1c8a31-7b34-4e59-8f0a-2c9e5d7b1a51");
const MIST_SOURCE: &str = include_str!("../../../../assets/shaders/cf-world-neon/weather_mist.wesl");
const MIST_PATH: &str = "cf-world-neon/weather_mist.wesl";

/// Enregistre le shader de la brume dans `Assets<Shader>`.
pub(super) fn embed_shader(shaders: &mut Assets<Shader>) {
    if let Err(error) = shaders.insert(MIST_SHADER.id(), Shader::from_wesl(MIST_SOURCE, MIST_PATH)) {
        // Impossible avec un identifiant UUID ; si cela arrivait, la brume resterait invisible plutôt que de couper le show.
        tracing::error!(%error, "shader de la brume non enregistré");
    }
}

/// Uniformes de la brume. Même ordre que la structure `Mist` de `weather_mist.wesl`.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct MistParams {
    /// Encre (`uInk`).
    pub ink: Vec4,
    /// Fond de la nappe (`uBase`) : velours tiré vers le violet, assombri.
    pub base: Vec4,
    /// Teinte d'ambiance (`uGlow`).
    pub glow: Vec4,
    /// Teinte d'ambiance assombrie (`uTint`).
    pub tint: Vec4,
    /// `x` : temps (s), `y` : pouls (`uPulse`), `z` : éclair d'orage accordé par le limiteur, `w` : 1 = bruit allégé.
    pub frame: Vec4,
}

/// Matériau de la brume : opaque, sans lumière ni ombre.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct MistMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: MistParams,
}

impl Material for MistMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(MIST_SHADER)
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

/// Marqueur du plan de brume.
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(WorldLayer, AbsolutePosition)]
pub struct Mist;

/// Poignée du matériau de la brume, pour le réécrire sans rien créer.
#[derive(Resource, Debug)]
pub struct MistRig {
    pub(super) material: Handle<MistMaterial>,
}

/// Les couleurs de la brume dérivées de la palette (`mist.ts`) : `(encre, fond)`. Le fond est `velvet.lerp(violet, 0.35) × 0,9`.
#[must_use]
pub fn mist_colors(palette: &ScenePalette) -> (LinearRgb, LinearRgb) {
    let base = palette
        .get(SceneColor::Velvet)
        .lerp(palette.get(SceneColor::Violet), 0.35);
    (
        palette.get(SceneColor::Ink),
        LinearRgb {
            r: base.r * 0.9,
            g: base.g * 0.9,
            b: base.b * 0.9,
        },
    )
}

/// Les uniformes de la brume : palette, teinte d'ambiance `mood`, pouls, éclair accordé, qualité.
#[must_use]
pub fn mist_params(
    palette: &ScenePalette,
    mood: LinearRgb,
    time_s: f64,
    pulse: f32,
    lightning: f32,
    cheap: bool,
) -> MistParams {
    let v = |c: LinearRgb| Vec4::new(c.r, c.g, c.b, 1.0);
    let (ink, base) = mist_colors(palette);
    MistParams {
        ink: v(ink),
        base: v(base),
        glow: v(mood),
        tint: v(LinearRgb {
            r: mood.r * 0.7,
            g: mood.g * 0.7,
            b: mood.b * 0.7,
        }),
        frame: Vec4::new(
            (time_s % TIME_WRAP_S) as f32,
            pulse,
            lightning,
            if cheap { 1.0 } else { 0.0 },
        ),
    }
}

/// Crée le plan de brume, une fois, au démarrage.
pub(super) fn spawn_mist(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MistMaterial>>,
) {
    let material = materials.add(MistMaterial::default());
    commands.spawn((
        Name::new("brume"),
        Mist,
        Mesh3d(meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(SIZE_M / 2.0)).mesh().build())),
        MeshMaterial3d(material.clone()),
    ));
    commands.insert_resource(MistRig { material });
}

/// Où se pose le plan quand la voiture est en `car` : `(x de la voiture, MIST_Y, 0,3 × côté devant elle)`.
#[must_use]
pub fn mist_position(car: bevy::math::DVec3) -> bevy::math::DVec3 {
    bevy::math::DVec3::new(car.x, MIST_Y, car.z - f64::from(SIZE_M) * AHEAD_FRACTION)
}

#[cfg(test)]
mod tests {
    use bevy::math::DVec3;

    use super::*;

    #[test]
    fn le_plan_se_pose_sous_la_route_devant_la_voiture() {
        let at = mist_position(DVec3::new(4.0, 1.0, -1_000_000.0));
        assert!((at.y - MIST_Y).abs() < f64::EPSILON && (at.x - 4.0).abs() < f64::EPSILON);
        assert!((at.z - (-1_000_000.0 - 450.0)).abs() < 1e-9, "450 m devant");
    }

    #[test]
    fn la_qualite_allege_le_bruit_et_l_horloge_reste_precise() {
        let palette = ScenePalette::default();
        let full = mist_params(&palette, LinearRgb::default(), 6.0 * 3600.0, 0.0, 0.0, false);
        let cheap = mist_params(&palette, LinearRgb::default(), 6.0 * 3600.0, 0.0, 0.0, true);
        assert!(full.frame.w < 0.5 && cheap.frame.w > 0.5);
        assert!(full.frame.x < TIME_WRAP_S as f32, "temps replié");
    }

    #[test]
    fn le_fond_est_le_velours_tire_vers_le_violet_et_assombri() {
        let palette = ScenePalette::default();
        let (_, base) = mist_colors(&palette);
        let (velvet, violet) = (palette.get(SceneColor::Velvet), palette.get(SceneColor::Violet));
        let expect = (velvet.r + (violet.r - velvet.r) * 0.35) * 0.9;
        assert!((base.r - expect).abs() < 1e-5, "{} contre {expect}", base.r);
    }
}
