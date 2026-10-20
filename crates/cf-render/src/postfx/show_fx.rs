//! Effets du spectacle sur l'image finale (portage de `engine/show-fx.ts` de Sillage), sans type de moteur.
//!
//! Flash de drop et de kick, séparation RVB, vague de lumière de changement de cue, étalonnage par chaleur, rendu de
//! bullet-time. **Tout flash ou variation de luminance passe par le [`FlashLimiter`] de `cf-show`** : les niveaux de
//! flash et de vague publiés ici sont ceux du limiteur, jamais une demande brute. Aucune allocation par image.

use cf_show::director::ShowState;
use cf_show::drop::{DropFxPackage, DropPlan};
use cf_show::limiter::{Channel, FlashLimiter, FlashRequest, MAX_FLASH_PEAK, MAX_SWEEP_PEAK, Rgb, SafeColor};

/// Écart minimal entre deux flashs de kick (s) : au plus 2 par seconde, le limiteur garde la marge pour les drops.
const KICK_FLASH_GAP: f64 = 0.5;
const SWEEP_ATTACK: f32 = 0.5;
const SWEEP_DECAY: f32 = 1.4;
const SWEEP_RUN: f32 = SWEEP_ATTACK + SWEEP_DECAY;
/// Sous cette échelle de temps, le monde est « au ralenti » ; plein effet à `SLOW_FULL`.
const SLOW_START: f32 = 0.6;
const SLOW_FULL: f32 = 0.25;
/// Pas de temps maximal (s) entre deux images : un trou d'image ne fait pas sauter les fondus.
const MAX_DT: f32 = 0.25;
/// Niveau de qualité adaptative au-delà duquel le flou radial du bullet-time est coupé.
const BLUR_MAX_QUALITY: u8 = 1;

/// Ce que les effets lisent du spectacle à chaque image : un extrait du `ShowState` (et du drop préparé), copiable,
/// pour que la logique se teste sans chef du spectacle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShowFxInput {
    /// Flashs permis (`FlashKill` les coupe).
    pub flashes: bool,
    /// Mode respiration : aucun flash, séparation RVB presque nulle.
    pub breathing: bool,
    /// Escalade lissée, 0 → 1.
    pub heat: f32,
    /// Facteur de temps du monde (1 normal) : en dessous de 0,6, rendu de bullet-time.
    pub time_scale: f32,
    /// Monte à chaque entrée de cue (`levelSeq` de Sillage) : déclenche la vague de lumière.
    pub cue_seq: u32,
    /// `gates.fx.rgb_split` du look : dosage de la séparation RVB du kick, 0 → 1.
    pub fx_rgb_split: f32,
    /// `gates.fx.flash` du look : dosage des flashs de kick, 0 → 1.
    pub fx_flash: f32,
    /// Effets du drop qui frappe **dans cette image** (`dropNow ? drop.fx`), sinon `None`.
    pub drop: Option<DropFxPackage>,
}

impl ShowFxInput {
    /// Extrait de l'état du spectacle de l'image ; `drop` est le drop préparé (`ShowDirector::drop_plan`).
    #[must_use]
    pub fn from_show(state: &ShowState, drop: Option<&DropPlan>) -> Self {
        let gates = state.gates();
        Self {
            flashes: state.flashes,
            breathing: state.breathing,
            heat: state.heat,
            time_scale: state.time_scale,
            cue_seq: state.cue_seq,
            fx_rgb_split: gates.fx.rgb_split,
            fx_flash: gates.fx.flash,
            drop: if state.drop_now { drop.map(|plan| plan.fx) } else { None },
        }
    }
}

/// Les couleurs de flash et de vague, tirées de la palette du thème (`ice`, `azure`, `mint`, `violet`). Toutes passent
/// par [`SafeColor`] : aucun rouge saturé plein écran possible.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlashPalette {
    /// Couleur du flash (`palette.ice`).
    pub flash: SafeColor,
    /// Couleurs de la vague, tournantes à chaque cue (`azure`, `mint`, `violet`, `ice`).
    pub sweeps: [SafeColor; 4],
}

/// Composante sRGB → lumière linéaire (les couleurs de Sillage sont linéarisées par three.js).
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Couleur linéaire à partir d'un `0xRRGGBB` sRGB.
fn hex(rgb: u32) -> SafeColor {
    let channel = |shift: u32| srgb_to_linear(((rgb >> shift) & 0xff) as f32 / 255.0);
    SafeColor::new(Rgb::new(channel(16), channel(8), channel(0)))
}

impl Default for FlashPalette {
    /// Thème « Velours » de Sillage (`scene.css`), en attendant les palettes (J4-08).
    fn default() -> Self {
        let ice = hex(0x00e6_f7ff);
        Self {
            flash: ice,
            sweeps: [hex(0x0012_a8d4), hex(0x0000_e5ff), hex(0x006a_4cc2), ice],
        }
    }
}

/// État des effets du spectacle. Écrit par le seul système du post-process, une fois par image.
#[derive(Debug, Clone)]
pub struct ShowFx {
    limiter: FlashLimiter,
    palette: FlashPalette,
    flash_color: SafeColor,
    sweep_color: SafeColor,
    /// Position de la vague de lumière sur l'écran (−0,25 → 1,25).
    sweep_pos: f32,
    /// Séparation RVB ajoutée à celle du kick.
    split: f32,
    contrast: f32,
    saturation: f32,
    /// Bullet-time, 0 → 1 (fondu).
    slow: f32,
    /// Flou radial du bullet-time : coupé aux basses qualités.
    blur: f32,
    drop_split: f32,
    grade: f32,
    last_time: Option<f64>,
    last_kick: f64,
    pulse_before: f32,
    seen_seq: Option<u32>,
    sweep_age: f32,
}

impl Default for ShowFx {
    fn default() -> Self {
        Self::new(FlashPalette::default())
    }
}

impl ShowFx {
    /// Effets au repos avec les couleurs de `palette`.
    #[must_use]
    pub fn new(palette: FlashPalette) -> Self {
        Self {
            limiter: FlashLimiter::new(),
            palette,
            flash_color: palette.flash,
            sweep_color: palette.sweeps[0],
            sweep_pos: -1.0,
            split: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            slow: 0.0,
            blur: 0.0,
            drop_split: 0.0,
            grade: 0.0,
            last_time: None,
            last_kick: f64::NEG_INFINITY,
            pulse_before: 0.0,
            seen_seq: None,
            sweep_age: SWEEP_RUN,
        }
    }

    /// Change les couleurs (changement de thème). Un flash en cours garde sa couleur jusqu'au suivant.
    pub fn set_palette(&mut self, palette: FlashPalette) {
        self.palette = palette;
    }

    /// À appeler à chaque image. `show` : `None` sans spectacle chargé ; `pulse` : impulsion du kick (1 → 0) ;
    /// `peak` : vrai au sommet du morceau ; `time` : horloge (s) ; `quality` : niveau de qualité adaptative
    /// (0 = plein).
    pub fn update(&mut self, show: Option<&ShowFxInput>, pulse: f32, peak: bool, time: f64, quality: u8) {
        let dt = self.last_time.map_or(0.0, |last| ((time - last) as f32).clamp(0.0, MAX_DT));
        self.last_time = Some(time);
        // Sans spectacle, aucun flash : le limiteur est en mode « respiration ».
        self.limiter
            .set_policy(show.is_some_and(|s| s.flashes), show.is_none_or(|s| s.breathing));

        if let Some(show) = show {
            self.trigger(show, pulse, peak, time);
        } else {
            self.seen_seq = None;
            self.drop_split = 0.0;
        }
        self.limiter.step(dt);

        self.sweep_age += dt;
        self.sweep_pos = -0.25 + 1.5 * (self.sweep_age / SWEEP_RUN).min(1.0);
        self.drop_split = (self.drop_split - dt * 2.0).max(0.0);
        self.split = show.map_or(0.0, |s| {
            let calmed = if s.breathing {
                0.05
            } else if s.flashes {
                1.0
            } else {
                0.5
            };
            (s.fx_rgb_split * pulse * 0.009 + self.drop_split * 0.014) * calmed
        });

        // Étalonnage : contraste et saturation montent avec la chaleur, jamais sous 1 (les noirs ne sont pas bouchés).
        let k = 1.0 - (-dt * 1.2).exp();
        let live = if show.is_some() { 1.0 } else { 0.0 };
        self.grade += (live - self.grade) * k;
        let heat = show.map_or(0.0, |s| s.heat);
        self.contrast = 1.0 + 0.07 * heat * self.grade;
        self.saturation = 1.0 + 0.12 * heat * self.grade;

        let slow_target = show.map_or(0.0, |s| {
            ((SLOW_START - s.time_scale) / (SLOW_START - SLOW_FULL)).clamp(0.0, 1.0)
        });
        self.slow += (slow_target - self.slow) * (1.0 - (-dt * 6.0).exp());
        self.blur = if quality <= BLUR_MAX_QUALITY { self.slow } else { 0.0 };
        self.pulse_before = pulse;
    }

    fn trigger(&mut self, show: &ShowFxInput, pulse: f32, peak: bool, time: f64) {
        match self.seen_seq {
            None => self.seen_seq = Some(show.cue_seq),
            Some(seen) if seen != show.cue_seq => {
                self.seen_seq = Some(show.cue_seq);
                let sweep = FlashRequest {
                    channel: Channel::Sweep,
                    peak: MAX_SWEEP_PEAK,
                    attack_s: SWEEP_ATTACK,
                    decay_s: SWEEP_DECAY,
                };
                if self.limiter.trigger(sweep, time) {
                    self.sweep_age = 0.0;
                    self.sweep_color = self.palette.sweeps[show.cue_seq as usize % self.palette.sweeps.len()];
                }
            }
            Some(_) => {}
        }
        if let Some(fx) = show.drop {
            self.drop_split = self.drop_split.max(fx.rgb_split);
            self.flash_color = self.palette.flash;
            let flash = FlashRequest {
                channel: Channel::Flash,
                peak: fx.flash * MAX_FLASH_PEAK,
                attack_s: 0.09,
                decay_s: 0.5,
            };
            self.limiter.trigger(flash, time);
        } else if peak
            && show.fx_flash > 0.0
            && pulse > 0.8
            && self.pulse_before < 0.5
            && time - self.last_kick > KICK_FLASH_GAP
        {
            let flash = FlashRequest {
                channel: Channel::Flash,
                peak: show.fx_flash * 0.18,
                attack_s: 0.08,
                decay_s: 0.25,
            };
            if self.limiter.trigger(flash, time) {
                self.last_kick = time;
            }
        }
    }

    /// Lumière ajoutée par le flash, déjà bornée par le limiteur (0 → `MAX_FLASH_PEAK`).
    #[must_use]
    pub fn flash(&self) -> f32 {
        self.limiter.value(Channel::Flash)
    }

    /// Intensité de la vague de lumière, déjà bornée par le limiteur (0 → `MAX_SWEEP_PEAK`).
    #[must_use]
    pub fn sweep(&self) -> f32 {
        self.limiter.value(Channel::Sweep)
    }

    /// Couleur du flash (linéaire), sans rouge saturé.
    #[must_use]
    pub fn flash_color(&self) -> Rgb {
        self.flash_color.rgb()
    }

    /// Couleur de la vague (linéaire), sans rouge saturé.
    #[must_use]
    pub fn sweep_color(&self) -> Rgb {
        self.sweep_color.rgb()
    }

    /// Position de la vague sur l'écran, en largeur d'écran (−0,25 → 1,25).
    #[must_use]
    pub fn sweep_pos(&self) -> f32 {
        self.sweep_pos
    }

    /// Séparation RVB du spectacle, ajoutée à celle du kick.
    #[must_use]
    pub fn split(&self) -> f32 {
        self.split
    }

    /// Contraste de l'étalonnage (≥ 1).
    #[must_use]
    pub fn contrast(&self) -> f32 {
        self.contrast
    }

    /// Saturation de l'étalonnage (≥ 1).
    #[must_use]
    pub fn saturation(&self) -> f32 {
        self.saturation
    }

    /// Bullet-time, 0 → 1.
    #[must_use]
    pub fn slow(&self) -> f32 {
        self.slow
    }

    /// Flou radial du bullet-time, 0 → 1 (0 aux basses qualités).
    #[must_use]
    pub fn blur(&self) -> f32 {
        self.blur
    }

    /// Le limiteur, en lecture (diagnostic, tests).
    #[must_use]
    pub fn limiter(&self) -> &FlashLimiter {
        &self.limiter
    }
}

#[cfg(test)]
#[path = "show_fx_tests.rs"]
mod tests;
