//! L'éclairage de scène écrit (DA §2.1) : le monde est émissif et non éclairé, donc la lumière principale, le
//! remplissage, le contre-jour et la brume sont un petit bloc d'uniformes commun, [`SceneLightParams`], que lisent les
//! shaders du monde par `atmosphere.wesl`.
//!
//! - [`SceneLight`] : l'éclairage de l'image en positions **absolues**, composé par [`compose`] (pur, testable) depuis
//!   la palette, la teinte d'ambiance, la densité visuelle `Δ` et l'état de la brume.
//! - [`SceneLightParams`] : le bloc d'uniformes, en repère rendu (relatif à l'origine flottante).
//! - [`SceneLightUniform`] : la ressource que recopient les matériaux ; un seul système l'écrit.

use bevy::ecs::resource::Resource;
use bevy::math::{DVec3, Vec3, Vec4};
use bevy::render::render_resource::ShaderType;

use super::formula::Shell;
use super::modulation::FogState;

/// Le texte WESL partagé (bloc `SceneLight` et fonctions de brume), ajouté au bout des shaders du monde.
pub const ATMOSPHERE_WESL: &str = include_str!("../../../../assets/shaders/cf-world-neon/atmosphere.wesl");

/// Nombre de tubes proches que la brume diffuse en forme close.
pub const TUBES: usize = 4;

/// Luminance relative (Rec. 709) d'une couleur linéaire.
#[must_use]
pub fn luma(c: Vec3) -> f32 {
    c.dot(Vec3::new(0.2126, 0.7152, 0.0722))
}

/// Ramène une couleur à la luminance `target` en gardant sa teinte ; une couleur noire devient un gris.
#[must_use]
pub fn at_luma(c: Vec3, target: f32) -> Vec3 {
    let l = luma(c);
    if l > 1.0e-5 { c * (target / l) } else { Vec3::splat(target) }
}

/// Réglages de l'éclairage de scène (valeurs de départ de DA §1.2, §2.1 et §3.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightTuning {
    /// Luminance de la lumière principale.
    pub key_luma: f32,
    /// Rapport principale / remplissage, au calme et au sommet (`8:1` et `16:1`).
    pub key_ratio: (f32, f32),
    /// Luminance du contre-jour.
    pub rim_luma: f32,
    /// Luminance propre de la brume, au calme et au sommet. Au-dessus des tours d'horizon, qu'elle découpe.
    pub fog_luma: (f32, f32),
    /// Part de la teinte d'ambiance dans la brume (le reste est le violet nuit).
    pub fog_mood: f32,
    /// Part des lumières principale et de contre-jour que la brume diffuse vers l'œil.
    pub scatter: f32,
    /// Force de la teinte de Rayleigh sur les lointains, 0..1.
    pub rayleigh: f32,
    /// Asymétrie de la rue : le côté dominant (−1 gauche, +1 droite), tiré par quartier.
    pub key_side: f32,
    /// Intensité d'un tube proche (sans unité, multipliée par `σ_s·ρ`).
    pub tube_gain: f32,
    /// Coquille de kick : vitesse (m/s), épaisseur (m), durée de vie (s), profondeur.
    pub shell: Shell,
    /// Part d'atmosphère, 0..1 (0 : le brouillard d'avant, fondu vers la teinte de fond).
    pub amount: f32,
}

impl Default for LightTuning {
    fn default() -> Self {
        Self {
            key_luma: 0.4,
            key_ratio: (8.0, 16.0),
            rim_luma: 0.22,
            fog_luma: (0.05, 0.065),
            fog_mood: 0.35,
            scatter: 0.5,
            rayleigh: 0.5,
            key_side: -1.0,
            tube_gain: 20.0,
            shell: Shell {
                speed: 40.0,
                sigma: 5.0,
                tau: 0.6,
                depth: 0.5,
            },
            amount: 1.0,
        }
    }
}

/// Teinte de Rayleigh en RVB (680, 550, 440 nm) : `(0,43 ; 1 ; 2,44)` ramenée à une luminance de 1.
#[must_use]
pub fn rayleigh_tint() -> Vec3 {
    at_luma(Vec3::new(0.43, 1.0, 2.44), 1.0)
}

/// Un tube proche : une source que la brume diffuse.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Tube {
    /// Position absolue, m.
    pub position: DVec3,
    /// Couleur linéaire déjà multipliée par l'intensité relative (nulle : éteint).
    pub color: Vec3,
}

/// Les couleurs que l'éclairage lit de la scène.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightColors {
    /// Violet nuit (`velvet`) : le fond de la brume.
    pub velvet: Vec3,
    /// Teinte d'ambiance (la dominante du moment).
    pub mood: Vec3,
    /// Glace : la lumière principale tire dessus.
    pub ice: Vec3,
    /// Azur : le ciel lointain du contre-jour.
    pub azure: Vec3,
}

/// L'éclairage de scène de l'image, en positions absolues.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneLight {
    /// Direction vers la lumière principale (unitaire).
    pub dir_key: Vec3,
    /// Rapport principale / remplissage.
    pub key_ratio: f32,
    /// Lumière principale.
    pub c_key: Vec3,
    /// Remplissage.
    pub c_fill: Vec3,
    /// Direction vers le contre-jour (unitaire).
    pub dir_rim: Vec3,
    /// Contre-jour.
    pub c_rim: Vec3,
    /// La brume de l'image.
    pub fog: FogState,
    /// Couleur propre de la brume.
    pub c_fog: Vec3,
    /// Part de lumière diffusée vers l'œil.
    pub scatter: f32,
    /// Force de la teinte de Rayleigh.
    pub rayleigh: f32,
    /// Âges des quatre derniers kicks, s.
    pub kick_ages: [f32; 4],
    /// Coquille de kick (profondeur nulle si elle est interdite).
    pub shell: Shell,
    /// Source de la coquille (le sujet), position absolue.
    pub source: DVec3,
    /// Altitude du sol, m.
    pub ground_y: f64,
    /// Les tubes proches.
    pub tubes: [Tube; TUBES],
    /// Part d'atmosphère, 0..1.
    pub amount: f32,
}

/// Ce que [`compose`] lit en plus des couleurs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightInput {
    /// Densité visuelle `Δ`, 0..1.
    pub density: f32,
    /// La brume de l'image.
    pub fog: FogState,
    /// Âges des quatre derniers kicks, s.
    pub kick_ages: [f32; 4],
    /// Le sujet, position absolue.
    pub source: DVec3,
    /// Les tubes proches.
    pub tubes: [Tube; TUBES],
}

/// Compose l'éclairage de l'image. Pur : mêmes entrées, même sortie.
#[must_use]
pub fn compose(tuning: &LightTuning, colors: &LightColors, input: &LightInput) -> SceneLight {
    let d = if input.density.is_finite() { input.density.clamp(0.0, 1.0) } else { 0.5 };
    let ratio = tuning.key_ratio.0 + (tuning.key_ratio.1 - tuning.key_ratio.0) * d;
    let c_key = at_luma(colors.mood.lerp(colors.ice, 0.5), tuning.key_luma);
    let fog_hue = colors.velvet.lerp(colors.mood, tuning.fog_mood);
    let fog_luma = tuning.fog_luma.0 + (tuning.fog_luma.1 - tuning.fog_luma.0) * d;
    let mut shell = tuning.shell;
    if !input.fog.shell {
        shell.depth = 0.0;
    }
    SceneLight {
        dir_key: Vec3::new(0.75 * tuning.key_side.signum(), 0.45, -0.45).normalize(),
        key_ratio: ratio,
        c_key,
        c_fill: at_luma(fog_hue, tuning.key_luma / ratio),
        // Le contre-jour est dans l'axe de la route, un peu au-dessus de l'horizon.
        dir_rim: Vec3::new(0.0, 0.12, -1.0).normalize(),
        c_rim: at_luma(colors.azure.lerp(colors.mood, 0.5), tuning.rim_luma),
        fog: input.fog,
        c_fog: at_luma(fog_hue, fog_luma),
        scatter: tuning.scatter,
        rayleigh: tuning.rayleigh,
        kick_ages: input.kick_ages,
        shell,
        source: input.source,
        ground_y: 0.0,
        tubes: input.tubes,
        amount: tuning.amount,
    }
}

/// Le bloc d'uniformes. Même ordre et mêmes types que la structure `SceneLight` de `atmosphere.wesl` : tout en `Vec4`.
/// Les positions sont relatives à l'origine flottante. À zéro (défaut), les shaders gardent leur brouillard d'avant.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct SceneLightParams {
    /// `xyz` : direction vers la lumière principale, `w` : rapport principale / remplissage.
    pub dir_key: Vec4,
    /// Lumière principale.
    pub c_key: Vec4,
    /// Remplissage.
    pub c_fill: Vec4,
    /// Direction vers le contre-jour.
    pub dir_rim: Vec4,
    /// Contre-jour.
    pub c_rim: Vec4,
    /// `x` : `σ_t·ρ0` (1/m), `y` : `H_f` (m), `z` : `g`, `w` : part d'atmosphère.
    pub fog: Vec4,
    /// `xyz` : couleur propre de la brume, `w` : part de lumière diffusée.
    pub c_fog: Vec4,
    /// `xyz` : teinte de Rayleigh, `w` : sa force.
    pub rayleigh: Vec4,
    /// Âges des quatre derniers kicks, s.
    pub kick_age: Vec4,
    /// Coquille : vitesse, épaisseur, durée de vie, profondeur.
    pub shell: Vec4,
    /// `xyz` : source de la coquille, `w` : altitude du sol.
    pub source: Vec4,
    /// Position des tubes.
    pub tube_pos: [Vec4; TUBES],
    /// Couleur des tubes, déjà multipliée par l'intensité et par `σ_s·ρ`.
    pub tube_col: [Vec4; TUBES],
}

impl SceneLight {
    /// Le bloc d'uniformes, en positions relatives à `origin` (l'origine flottante). `tube_gain` vient du réglage.
    #[must_use]
    pub fn params(&self, origin: DVec3, tube_gain: f32) -> SceneLightParams {
        let rel = |p: DVec3| (p - origin).as_vec3();
        let mut out = SceneLightParams {
            dir_key: self.dir_key.extend(self.key_ratio),
            c_key: self.c_key.extend(0.0),
            c_fill: self.c_fill.extend(0.0),
            dir_rim: self.dir_rim.extend(0.0),
            c_rim: self.c_rim.extend(0.0),
            fog: Vec4::new(self.fog.sigma_rho, self.fog.height_m, self.fog.g, self.amount),
            c_fog: self.c_fog.extend(self.scatter),
            rayleigh: rayleigh_tint().extend(self.rayleigh),
            kick_age: Vec4::from_array(self.kick_ages),
            shell: Vec4::new(self.shell.speed, self.shell.sigma, self.shell.tau, self.shell.depth),
            source: rel(self.source).extend((self.ground_y - origin.y) as f32),
            ..SceneLightParams::default()
        };
        for (i, tube) in self.tubes.iter().enumerate() {
            out.tube_pos[i] = rel(tube.position).extend(0.0);
            out.tube_col[i] = (tube.color * tube_gain * self.fog.sigma_rho).extend(0.0);
        }
        out
    }
}

/// Le bloc d'uniformes de l'image, que les matériaux du monde recopient. Écrit par le seul `update_scene_light`.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct SceneLightUniform(pub SceneLightParams);

/// Distance utile de la brume (m), publiée pour le culling (C7) : au-delà, `T < 2 %`.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct FogReach(pub f32);

impl Default for FogReach {
    fn default() -> Self {
        Self(super::formula::fog_reach(super::modulation::FogTuning::default().sigma_rho))
    }
}

#[cfg(test)]
mod tests {
    use super::super::formula;
    use super::super::modulation::{FogInput, FogModulation, FogTuning};
    use super::*;

    fn colors() -> LightColors {
        LightColors {
            velvet: Vec3::new(0.03, 0.01, 0.09),
            mood: Vec3::new(0.25, 0.1, 0.8),
            ice: Vec3::new(0.8, 0.9, 1.0),
            azure: Vec3::new(0.1, 0.4, 0.9),
        }
    }

    fn input(density: f32) -> LightInput {
        let fog = FogModulation::default().step(&FogTuning::default(), &FogInput { dt_s: 1.0 / 60.0, ..FogInput::default() });
        LightInput {
            density,
            fog,
            kick_ages: [60.0; 4],
            source: DVec3::new(0.0, 0.5, -40_000.0),
            tubes: [Tube::default(); TUBES],
        }
    }

    #[test]
    fn le_rapport_principale_remplissage_va_de_8_a_16() {
        let calm = compose(&LightTuning::default(), &colors(), &input(0.0));
        let peak = compose(&LightTuning::default(), &colors(), &input(1.0));
        assert!((luma(calm.c_key) / luma(calm.c_fill) - 8.0).abs() < 0.01);
        assert!((luma(peak.c_key) / luma(peak.c_fill) - 16.0).abs() < 0.02);
        assert!((calm.dir_key.length() - 1.0).abs() < 1e-6 && (calm.dir_rim.length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn les_positions_du_bloc_sont_relatives_a_l_origine() {
        let light = compose(&LightTuning::default(), &colors(), &input(0.5));
        let p = light.params(DVec3::new(0.0, 0.0, -40_000.0), 20.0);
        assert!((p.source.truncate() - Vec3::new(0.0, 0.5, 0.0)).length() < 1e-4);
        assert!((p.fog.x - 0.0037).abs() < 1e-6 && (p.fog.w - 1.0).abs() < f32::EPSILON);
        assert!((luma(p.rayleigh.truncate()) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn respiration_la_coquille_de_kick_est_a_plat() {
        let mut i = input(0.5);
        i.fog.shell = false;
        let light = compose(&LightTuning::default(), &colors(), &i);
        assert!(light.shell.depth.abs() < f32::EPSILON);
    }

    /// DA §1.2, règles 1 et 2, sur la formule : une tour d'horizon sombre finit **sous** la brume qui l'entoure, et les
    /// plans P3 (20 m), P2 (250 m) et P1 (700 m) d'une même façade sont séparés d'au moins 0,12 en luminance affichée
    /// (après la courbe de transfert sRGB, celle de l'écran).
    #[test]
    fn les_plans_sont_separes_en_valeur() {
        let light = compose(&LightTuning::default(), &colors(), &input(0.5));
        let cam = Vec3::new(0.0, 2.0, 0.0);
        let shade = |surface: Vec3, dist: f32, y: f32| {
            let p = Vec3::new(20.0, y, -dist);
            let ray = p - cam;
            let d = ray.length();
            let dir = ray / d;
            let tau = formula::optical_depth(light.fog.sigma_rho, light.fog.height_m, 0.0, cam, dir, d);
            let t = formula::transmittance(tau);
            let scatter = light.c_key * formula::phase_hg(dir.dot(light.dir_key), light.fog.g)
                + light.c_rim * formula::phase_hg(dir.dot(light.dir_rim), light.fog.g);
            let mist = light.c_fog + scatter * light.scatter;
            luma(formula::aerial(surface, mist, t))
        };
        let display = |l: f32| l.max(0.0).powf(1.0 / 2.2);
        // Une façade moyennement éclairée (fenêtres allumées moyennées) et une silhouette d'horizon éteinte.
        let facade = Vec3::new(0.30, 0.22, 0.45);
        let (p3, p2, p1) = (display(shade(facade, 20.0, 8.0)), display(shade(facade, 250.0, 8.0)), display(shade(facade, 700.0, 8.0)));
        assert!(p3 - p2 >= 0.12 && p2 - p1 >= 0.12, "P3 {p3}, P2 {p2}, P1 {p1}");
        let silhouette = shade(Vec3::new(0.004, 0.003, 0.008), 700.0, 40.0);
        let fog_behind = shade(Vec3::ZERO, 5000.0, 40.0);
        assert!(silhouette < fog_behind, "silhouette {silhouette}, brume {fog_behind}");
    }
}
