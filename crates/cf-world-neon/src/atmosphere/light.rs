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
use cf_analysis::SPECTRUM_BANDS;

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
    if l > 1.0e-5 {
        c * (target / l)
    } else {
        Vec3::splat(target)
    }
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
    /// Plafond d'émissif d'une fenêtre : `0,9·θ_b`, `θ_b` étant le seuil du bloom (1,0 depuis C2).
    pub window_cap: f32,
    /// Gain du halo qu'un tube proche pose sur les murs (C4, `wall_halo` de `facade_light.wesl`).
    pub halo_gain: f32,
    /// Énergie lente sous laquelle le morceau est « calme » (P2 §1.1 : `calme = 1 − sat(E_s/0,25)`).
    pub calm_energy: f32,
}

impl Default for LightTuning {
    fn default() -> Self {
        Self {
            key_luma: 0.4,
            key_ratio: (8.0, 16.0),
            rim_luma: 0.25,
            fog_luma: (0.05, 0.065),
            fog_mood: 0.35,
            scatter: 0.5,
            rayleigh: 0.5,
            key_side: -1.0,
            tube_gain: 4.0,
            shell: Shell {
                speed: 40.0,
                sigma: 5.0,
                tau: 0.6,
                depth: 0.5,
            },
            amount: 1.0,
            window_cap: crate::facade::windows::CAP_SHARE * cf_render::postfx::BLOOM_THRESHOLD,
            halo_gain: 8.0,
            calm_energy: 0.25,
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
    /// Densité visuelle `Δ`, 0..1 (part des fenêtres allumées, C4).
    pub density: f32,
    /// Spectre lissé en 32 bandes, 0..1 (façades égaliseur, C4) ; à plat quand rien ne doit pulser.
    pub spectrum: [f32; SPECTRUM_BANDS],
    /// Plafond d'émissif d'une fenêtre : `0,9·θ_b` (C4).
    pub window_cap: f32,
    /// Les signaux des surfaces (C4).
    pub surface: Surface,
    /// Gain du halo des tubes sur les murs.
    pub halo_gain: f32,
}

/// Ce que les surfaces lisent du son et de la qualité (C4, `SceneLight::surface` de `atmosphere.wesl`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Surface {
    /// Frémissement des flaques, 0..1 (`Target::Puddles` : le kick élargit les reflets pendant 180 ms).
    pub puddles: f32,
    /// Saturation des néons, 0..1 (`Target::NeonSaturation`, haut-médium).
    pub neon_saturation: f32,
    /// Énergie lente du morceau, 0..1 : sous `LightTuning::calm_energy`, les flaques deviennent un miroir net.
    pub energy_slow: f32,
    /// Détail des surfaces, 0..1 (qualité adaptative) : fenêtres habitées, aberration des reflets.
    pub detail: f32,
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
    /// Spectre lissé en 32 bandes, 0..1.
    pub spectrum: [f32; SPECTRUM_BANDS],
    /// Les signaux des surfaces, bruts : [`compose`] les borne et les met à plat quand rien ne doit pulser.
    pub surface: Surface,
}

/// Compose l'éclairage de l'image. Pur : mêmes entrées, même sortie.
#[must_use]
pub fn compose(tuning: &LightTuning, colors: &LightColors, input: &LightInput) -> SceneLight {
    let d = if input.density.is_finite() {
        input.density.clamp(0.0, 1.0)
    } else {
        0.5
    };
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
        density: d,
        // Respiration ou flashs coupés (`fog.shell` faux) : le vu-mètre des façades reste à plat (règle 7).
        spectrum: if input.fog.shell {
            input
                .spectrum
                .map(|v| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 })
        } else {
            [0.0; SPECTRUM_BANDS]
        },
        window_cap: tuning.window_cap,
        surface: Surface {
            // Respiration ou flashs coupés : ni frémissement ni saturation qui suit la musique (règle 7).
            puddles: if input.fog.shell {
                unit(input.surface.puddles)
            } else {
                0.0
            },
            neon_saturation: if input.fog.shell {
                unit(input.surface.neon_saturation)
            } else {
                0.0
            },
            energy_slow: unit(input.surface.energy_slow),
            detail: unit(input.surface.detail),
        },
        halo_gain: tuning.halo_gain,
    }
}

/// Borne un signal à 0..1 ; une valeur non finie vaut 0.
fn unit(v: f32) -> f32 {
    if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 }
}

impl SceneLight {
    /// Le calme du morceau, 0..1 : `1 − sat(E_s / seuil)` (P2 §1.1).
    #[must_use]
    pub fn calm(&self, calm_energy: f32) -> f32 {
        1.0 - (self.surface.energy_slow / calm_energy.max(1.0e-3)).clamp(0.0, 1.0)
    }
}

/// Le bloc d'uniformes. Même ordre et mêmes types que la structure `SceneLight` de `atmosphere.wesl` : tout en `Vec4`.
/// Les positions sont relatives à l'origine flottante. À zéro (défaut), les shaders gardent leur brouillard d'avant.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct SceneLightParams {
    /// `xyz` : direction vers la lumière principale, `w` : rapport principale / remplissage.
    pub dir_key: Vec4,
    /// `xyz` : lumière principale, `w` : plafond d'émissif d'une fenêtre (`0,9·θ_b`).
    pub c_key: Vec4,
    /// `xyz` : remplissage, `w` : densité visuelle `Δ`.
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
    /// Spectre lissé, 32 bandes, quatre par `Vec4` (façades égaliseur, C4).
    pub spectrum: [Vec4; SPECTRUM_BANDS / 4],
    /// Surfaces (C4) : `x` frémissement des flaques, `y` saturation des néons, `z` calme, `w` détail.
    pub surface: Vec4,
}

impl SceneLight {
    /// Le bloc d'uniformes, en positions relatives à `origin` (l'origine flottante). `tube_gain` vient du réglage.
    #[must_use]
    pub fn params(&self, origin: DVec3, tube_gain: f32, calm_energy: f32) -> SceneLightParams {
        let rel = |p: DVec3| (p - origin).as_vec3();
        let mut out = SceneLightParams {
            dir_key: self.dir_key.extend(self.key_ratio),
            c_key: self.c_key.extend(self.window_cap),
            c_fill: self.c_fill.extend(self.density),
            dir_rim: self.dir_rim.extend(0.0),
            c_rim: self.c_rim.extend(0.0),
            fog: Vec4::new(self.fog.sigma_rho, self.fog.height_m, self.fog.g, self.amount),
            c_fog: self.c_fog.extend(self.scatter),
            rayleigh: rayleigh_tint().extend(self.rayleigh),
            kick_age: Vec4::from_array(self.kick_ages),
            shell: Vec4::new(self.shell.speed, self.shell.sigma, self.shell.tau, self.shell.depth),
            source: rel(self.source).extend((self.ground_y - origin.y) as f32),
            surface: Vec4::new(
                self.surface.puddles,
                self.surface.neon_saturation,
                self.calm(calm_energy),
                self.surface.detail,
            ),
            ..SceneLightParams::default()
        };
        for (slot, quad) in out.spectrum.iter_mut().zip(self.spectrum.as_chunks::<4>().0) {
            *slot = Vec4::from_array(*quad);
        }
        for (i, tube) in self.tubes.iter().enumerate() {
            // La couleur du bloc est déjà multipliée par `tube_gain·σ_s·ρ` (la brume) : `w` ramène le halo des murs à
            // `couleur·halo_gain`, quelle que soit la densité de la brume.
            let scale = tube_gain * self.fog.sigma_rho;
            let halo = if scale > 1.0e-9 { self.halo_gain / scale } else { 0.0 };
            out.tube_pos[i] = rel(tube.position).extend(halo);
            out.tube_col[i] = (tube.color * scale).extend(0.0);
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
        Self(super::formula::fog_reach(
            super::modulation::FogTuning::default().sigma_rho,
        ))
    }
}

#[cfg(test)]
#[path = "light_tests.rs"]
mod tests;
