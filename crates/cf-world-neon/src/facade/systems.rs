//! Le système des façades : remplir le matériau d'après l'image.

use bevy::math::Vec4;
use bevy::prelude::*;
use cf_render::palette::LinearRgb;
use cf_render::{AudioFeatures, CurrentShowFx, FloatingOrigin, SceneColor, ScenePalette};

use super::material::{FacadeParams, TINT_COUNT};
use super::temperament::{TEMPER_COUNT, TEMPERAMENTS};
use super::{FacadeLook, FacadeMaterial};
use cf_show::limiter::MIN_FADE;

use crate::beams::BeamPulse;
use crate::landmarks::field::{CityField, FieldParams};
use crate::road::RoadLook;

/// Période, en s, au bout de laquelle le temps du shader repart de zéro : 40 min.
///
/// Le temps est envoyé en `f32` : sans borne, `fract(t × 1,5 + …)` perdrait sa précision au fil d'un set de plusieurs
/// heures. 2 400 s × 1,5 = 3 600 : un nombre entier de périodes du clignotement des fenêtres, qui revient donc à
/// l'identique. Seuls les filets d'eau (pluie), dont la vitesse varie d'un filet à l'autre, repartent d'un coup.
const TIME_WRAP_S: f64 = 2400.0;

/// Ce que le shader lit de l'image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameInputs {
    /// Temps écoulé, en s.
    pub time_s: f64,
    /// Pouls du kick, 0 à 1 (`b.pulse`).
    pub pulse: f32,
    /// Énergie des basses, 0 à 1.
    pub bass: f32,
    /// Médiums, 0 à 1.
    pub mid: f32,
    /// Aigus, 0 à 1.
    pub high: f32,
    /// Temps musical continu (`b.beat`) : le numéro du temps plus une montée douce de 0 à 1.
    pub beat: f64,
    /// Impulsion d'un transitoire large bande (caisse claire, clap), 1 → 0.
    pub onset: f32,
    /// Impulsion d'un transitoire d'aigus (charleston), 1 → 0.
    pub hat: f32,
    /// Brillance du spectre, 0..1.
    pub brightness: f32,
    /// Bruit (platitude du spectre), 0..1.
    pub noisiness: f32,
    /// Voie rapide des mouvements, déjà passée par le limiteur (`MotionDrive::sweep`), 0 à 1.
    pub motion_sweep: f32,
    /// Part de mouvement rapide autorisée (`MotionDrive::flicker`), 0 à 1.
    pub motion_flicker: f32,
}

impl Default for FrameInputs {
    fn default() -> Self {
        Self {
            time_s: 0.0,
            pulse: 0.0,
            bass: 0.0,
            mid: 0.0,
            high: 0.0,
            beat: 0.0,
            onset: 0.0,
            hat: 0.0,
            brightness: 0.0,
            noisiness: 0.0,
            motion_sweep: 0.0,
            motion_flicker: 0.0,
        }
    }
}

fn vec4(color: LinearRgb) -> Vec4 {
    Vec4::new(color.r, color.g, color.b, 1.0)
}

/// Les seize teintes que le tag d'une pièce nomme : le cycle prismatique (0..5), puis les couleurs de la scène (6..15, dans
/// l'ordre de [`SceneColor::ALL`] sans l'encre), voir [`tint_rank`].
fn tints(palette: &ScenePalette) -> [Vec4; TINT_COUNT] {
    let mut out = [Vec4::ZERO; TINT_COUNT];
    for (slot, color) in out.iter_mut().zip(palette.prismatic_cycle()) {
        *slot = vec4(color);
    }
    for color in SceneColor::ALL {
        if let Some(rank) = tint_rank(color) {
            out[rank] = vec4(palette.get(color));
        }
    }
    out
}

/// La table des tempéraments pour le shader, deux `Vec4` par entrée : `(hue_span_deg, lit_bias, amber_share, ice_share)` puis
/// `(forme, mouvement, spark_rate, 0)`. Forme et mouvement sont leur rang dans l'énumération (l'ordre de déclaration).
fn temper_table() -> [Vec4; 2 * TEMPER_COUNT] {
    let mut out = [Vec4::ZERO; 2 * TEMPER_COUNT];
    for (i, t) in TEMPERAMENTS.iter().enumerate() {
        out[2 * i] = Vec4::new(t.hue_span_deg, t.lit_bias, t.amber_share, t.ice_share);
        out[2 * i + 1] = Vec4::new(f32::from(t.shape as u8), f32::from(t.motion as u8), t.spark_rate, 0.0);
    }
    out
}

/// Le rang de teinte d'une couleur de la scène dans le tag d'une pièce (`6 + position`, l'encre n'en a pas : un corps noir
/// n'a pas de sens), ou `None`.
#[must_use]
pub fn tint_rank(color: SceneColor) -> Option<usize> {
    let position = SceneColor::ALL.iter().position(|c| *c == color)?;
    (position > 0).then(|| 5 + position)
}

/// Les uniformes de l'image (`Scenery.update` de Sillage, plus le bloc de palette).
#[must_use]
pub fn build_params(
    palette: &ScenePalette,
    look: &FacadeLook,
    wet: f32,
    rain: f32,
    frame: &FrameInputs,
) -> FacadeParams {
    let get = |color| palette.get(color);
    // `uMist` de Sillage : `velvet` mélangé de 35 % au violet, assombri de 10 %.
    let mist = get(SceneColor::Velvet).lerp(get(SceneColor::Violet), 0.35);
    let mist = LinearRgb {
        r: mist.r * 0.9,
        g: mist.g * 0.9,
        b: mist.b * 0.9,
    };
    FacadeParams {
        ink: vec4(get(SceneColor::Ink)),
        ice: vec4(get(SceneColor::Ice)),
        amber: vec4(get(SceneColor::Amber)),
        haze: vec4(get(SceneColor::Violet)),
        mist: vec4(mist),
        scan: vec4(get(SceneColor::Mint)),
        cycle: tints(palette),
        tempers: temper_table(),
        frame: Vec4::new((frame.time_s % TIME_WRAP_S) as f32, frame.pulse, frame.bass, frame.mid),
        audio: Vec4::new(
            frame.high,
            frame.beat as f32,
            look.build.clamp(0.0, 1.0),
            look.flash.clamp(0.0, 1.0),
        ),
        weather: Vec4::new(
            wet.clamp(0.0, 1.0),
            rain.clamp(0.0, 1.0),
            look.win_pulse.clamp(0.0, 1.0),
            0.0,
        ),
        power: Vec4::new(
            look.power.x,
            look.power.y.max(0.001),
            look.power.z.clamp(0.0, 1.0),
            look.power.w,
        ),
        texture: Vec4::new(
            frame.onset.clamp(0.0, 1.0),
            frame.hat.clamp(0.0, 1.0),
            frame.brightness.clamp(0.0, 1.0),
            frame.noisiness.clamp(0.0, 1.0),
        ),
        motion: Vec4::new(
            frame.motion_sweep.clamp(0.0, 1.0),
            frame.motion_flicker.clamp(0.0, 1.0),
            0.0,
            0.0,
        ),
    }
}

/// Les deux voies de flash des façades : transitoire large bande et charleston. Les deux passent par le limiteur
/// photosensible (règle 7 de `CLAUDE.md`, WCAG 2.3.1), comme le pouls des faisceaux : 3 départs par seconde au plus, jamais
/// de front plus raide que `MIN_FADE`, rien sans spectacle ni en mode respiration.
#[derive(Debug, Default)]
pub(super) struct TextureFlash {
    onset: BeamPulse,
    hat: BeamPulse,
}

/// Ce que les mouvements de lumière ont le droit de montrer à cette image.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MotionDrive {
    /// Voie rapide limitée, 0 à 1 : le niveau du limiteur (3 départs par seconde au plus, jamais plus raide que `MIN_FADE`).
    pub sweep: f32,
    /// Part de mouvement rapide autorisée, 0 à 1 : 0 sans spectacle, flashs coupés ou en respiration ; elle monte et
    /// descend en `MIN_FADE` au moins (jamais de saut de luminance).
    pub flicker: f32,
}

/// La voie de flash des mouvements de lumière (vagues, balayages, pulsation) : le pouls du kick passe par le limiteur
/// photosensible (règle 7 de `CLAUDE.md`), comme [`TextureFlash`]. Le mouvement lent et lisse du shader n'en dépend pas.
#[derive(Debug, Default)]
pub struct MotionFlash {
    pulse: BeamPulse,
    flicker: f32,
}

impl MotionFlash {
    /// Avance d'une image. `policy` : `(flashes, breathing)` du spectacle, `None` sans spectacle ; `now_s` : horloge de la
    /// scène (s) ; `drive` : pouls du kick, 0 à 1.
    pub fn update(&mut self, dt_s: f32, now_s: f64, policy: Option<(bool, bool)>, drive: f32) -> MotionDrive {
        self.pulse.update(dt_s, now_s, policy, drive);
        let target = if self.pulse.permitted() { 1.0 } else { 0.0 };
        let step = dt_s.max(0.0) / MIN_FADE;
        self.flicker += (target - self.flicker).clamp(-step, step);
        MotionDrive {
            sweep: self.pulse.pulse(),
            flicker: self.flicker,
        }
    }
}

/// `CfSet::Present` : remplit le matériau (temps, musique, météo, palette). Le matériau n'est touché que si quelque chose a
/// changé : Bevy ne re-prépare pas un asset qu'on n'a pas écrit.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(super) fn update_material(
    time: Res<Time<Real>>,
    features: Option<Res<AudioFeatures>>,
    palette: Res<ScenePalette>,
    look: Res<FacadeLook>,
    road: Res<RoadLook>,
    materials: Option<ResMut<Assets<FacadeMaterial>>>,
    scenery: Option<Res<crate::scenery::SceneryAssets>>,
    origin: Option<Res<FloatingOrigin>>,
    field: Option<Res<CityField>>,
    show_fx: Option<Res<CurrentShowFx>>,
    mut flash: Local<TextureFlash>,
    mut motion: Local<MotionFlash>,
) {
    let (Some(mut materials), Some(scenery)) = (materials, scenery) else {
        return;
    };
    let frame = features.as_deref().map_or_else(FrameInputs::default, |f| FrameInputs {
        time_s: 0.0,
        pulse: f.0.pulse,
        bass: f.0.bass,
        mid: f.0.mid,
        high: f.0.high,
        beat: f.0.beat,
        onset: f.0.onset,
        hat: f.0.hat,
        brightness: f.0.brightness,
        noisiness: f.0.noisiness,
        ..FrameInputs::default()
    });
    let now_s = look.clock_s.unwrap_or_else(|| time.elapsed_secs_f64());
    let policy = show_fx.map(|s| (s.0.flashes, s.0.breathing));
    let dt_s = time.delta_secs();
    flash.onset.update(dt_s, now_s, policy, frame.onset);
    flash.hat.update(dt_s, now_s, policy, frame.hat);
    let drive = motion.update(dt_s, now_s, policy, frame.pulse);
    let frame = FrameInputs {
        time_s: now_s,
        motion_sweep: drive.sweep,
        motion_flicker: drive.flicker,
        onset: flash.onset.pulse(),
        hat: flash.hat.pulse(),
        ..frame
    };
    let params = build_params(&palette, &look, road.wet, road.rain, &frame);
    // Le champ de la ville (J7-08) : lumières des immeubles-vitrines, zone de glitch, panne de courant, en repère rendu.
    let field_params = field
        .as_deref()
        .zip(origin.as_deref())
        .map_or_else(FieldParams::default, |(field, origin)| field.params(origin.origin()));
    if materials
        .get(&scenery.material)
        .is_some_and(|m| m.params == params && m.field == field_params)
    {
        return;
    }
    if let Some(mut material) = materials.get_mut(&scenery.material) {
        material.params = params;
        material.field = field_params;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facade::{TEMPER_COUNT, TEMPERAMENTS};

    fn params() -> FacadeParams {
        build_params(
            &ScenePalette::default(),
            &FacadeLook::default(),
            0.0,
            0.0,
            &FrameInputs::default(),
        )
    }

    #[test]
    fn build_params_exposes_temperament_table() {
        let p = params();
        assert_eq!(p.tempers.len(), 2 * TEMPER_COUNT);
        for (i, t) in TEMPERAMENTS.iter().enumerate() {
            assert_eq!(
                p.tempers[2 * i],
                Vec4::new(t.hue_span_deg, t.lit_bias, t.amber_share, t.ice_share),
                "entrée {i}, partie couleurs"
            );
            assert_eq!(
                p.tempers[2 * i + 1],
                Vec4::new(f32::from(t.shape as u8), f32::from(t.motion as u8), t.spark_rate, 0.0),
                "entrée {i}, partie forme"
            );
        }
    }

    #[test]
    fn temperament_zero_is_neutral() {
        let p = params();
        assert_eq!(p.tempers[0], Vec4::new(0.0, 0.0, 0.3, 0.1));
        // Le shader lit les seuils `1 - part` : ils doivent retomber exactement sur les constantes d'avant (0,7 et 0,9).
        assert_eq!(p.tempers[1], Vec4::ZERO, "Panes, Steady, sans étincelles");
        assert_eq!(1.0_f32 - p.tempers[0].z, 0.7);
        assert_eq!(1.0_f32 - p.tempers[0].w, 0.9);
    }

    const DT: f32 = 1.0 / 60.0;

    /// Rejoue `seconds` s avec une entrée à 10 Hz (le pire cas) ; rend la sortie de chaque image.
    fn run_motion(policy: Option<(bool, bool)>, seconds: f32) -> Vec<MotionDrive> {
        let mut flash = MotionFlash::default();
        (0..(seconds / DT) as usize)
            .map(|i| {
                let raw = if i % 6 == 0 { 1.0 } else { 0.0 };
                flash.update(DT, f64::from(DT) * i as f64, policy, raw)
            })
            .collect()
    }

    #[test]
    fn motion_flash_caps_rate() {
        let out = run_motion(Some((true, false)), 10.0);
        let levels: Vec<f32> = out.iter().map(|m| m.sweep).collect();
        assert!(
            levels.iter().copied().fold(0.0, f32::max) > 0.5,
            "la voie ne part jamais"
        );
        let starts: Vec<usize> = (1..levels.len())
            .filter(|&i| levels[i] > 0.0 && levels[i - 1] == 0.0)
            .collect();
        assert!(starts.len() >= 8, "des départs ont lieu : {}", starts.len());
        let window = (1.0 / DT) as usize;
        for &s in &starts {
            let n = starts.iter().filter(|&&t| t >= s && t < s + window).count();
            assert!(n <= 3, "{n} départs en 1 s");
        }
        let max_step = DT / MIN_FADE + 1e-3;
        for pair in levels.windows(2) {
            assert!(
                (pair[0] - pair[1]).abs() <= max_step,
                "saut de {}",
                (pair[0] - pair[1]).abs()
            );
        }
        // La part rapide autorisée monte sans saut elle aussi.
        for pair in out.windows(2) {
            assert!((pair[0].flicker - pair[1].flicker).abs() <= max_step);
        }
        assert!(out.iter().all(|m| m.sweep <= 1.0 + 1e-6 && m.flicker <= 1.0));
    }

    #[test]
    fn motion_flash_off_without_flashes_policy() {
        for policy in [None, Some((false, false)), Some((false, true)), Some((true, true))] {
            let out = run_motion(policy, 5.0);
            assert!(
                out.iter().all(|m| m.sweep == 0.0 && m.flicker == 0.0),
                "{policy:?} : de la lumière rapide malgré la politique"
            );
        }
    }

    #[test]
    fn motion_flash_fades_out_when_flashes_are_cut() {
        let mut flash = MotionFlash::default();
        for i in 0..120 {
            flash.update(
                DT,
                f64::from(DT) * f64::from(i),
                Some((true, false)),
                if i % 6 == 0 { 1.0 } else { 0.0 },
            );
        }
        let mut last = MotionDrive {
            sweep: 1.0,
            flicker: 1.0,
        };
        for i in 120..200 {
            last = flash.update(DT, f64::from(DT) * f64::from(i), Some((false, false)), 1.0);
        }
        assert!(last.sweep == 0.0 && last.flicker == 0.0, "{last:?}");
    }

    #[test]
    fn build_params_carries_the_motion_drive() {
        let frame = FrameInputs {
            motion_sweep: 0.5,
            motion_flicker: 1.0,
            ..FrameInputs::default()
        };
        let p = build_params(&ScenePalette::default(), &FacadeLook::default(), 0.0, 0.0, &frame);
        assert_eq!(p.motion, Vec4::new(0.5, 1.0, 0.0, 0.0));
        assert_eq!(params().motion, Vec4::ZERO);
    }
}
