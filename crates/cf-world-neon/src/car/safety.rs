//! Sécurité photosensible de la voiture (règle 7 de `CLAUDE.md`, WCAG 2.3.1) : toute variation brusque de luminance de la
//! voiture passe par le [`FlashLimiter`] de `cf-show`, comme les flashs du post-process (`cf-render::postfx::ShowFx`).
//!
//! Sont concernés : les pulsations de glow sur les kicks (halos, volumes et flaques de phares, lueur sous la caisse,
//! logo arrière, liseré de la peinture) et les clignotants (1,5 Hz). Chacun a **son** limiteur, avec la même politique
//! que le post-process : permis seulement si le spectacle le permet (`ShowFxInput::flashes`) et hors respiration ;
//! sans spectacle, comme `ShowFx::update`, rien ne flashe. Ce que la lumière montre est le niveau que le limiteur rend :
//! au plus 3 départs par seconde, montée et descente d'au moins `MIN_FADE`, plafond de la voie ; flashs coupés, tout
//! retombe à 0 en `MIN_FADE` au plus.
//!
//! Deux limiteurs et non un : celui du post-process est privé (`PostFxState` ne le prête qu'en lecture, aucun
//! `trigger`) et ne s'élargit pas ici. Chacun borne sa source à 3 par seconde ; le compte total à l'écran n'est donc pas
//! partagé avec le post-process, mais la voiture occupe une petite partie de l'image.
//!
//! Les autres hausses de lumière de la voiture (freinage, feux de route) ne battent pas : elles sont lissées par
//! [`Slew`], jamais plus rapides que `MIN_FADE`.

use cf_show::limiter::{Channel, FlashLimiter, FlashRequest, MIN_FADE};

/// Montée du flash de kick (s) : un peu plus que `MIN_FADE`, soit 6 images à 60 i/s.
pub const KICK_ATTACK_S: f32 = 0.1;
/// Descente du flash de kick (s).
pub const KICK_DECAY_S: f32 = 0.25;
/// Montée d'un battement de clignotant (s).
pub const BLINK_ATTACK_S: f32 = 0.1;
/// Descente d'un battement de clignotant (s) : un battement dure ~0,3 s, comme la moitié du cycle de 0,66 s.
pub const BLINK_DECAY_S: f32 = 0.2;
/// Le pouls doit passer de sous `EDGE_LOW` à au-dessus de `EDGE_HIGH` pour compter pour un kick (`ShowFx::trigger`).
const EDGE_HIGH: f32 = 0.8;
const EDGE_LOW: f32 = 0.5;

/// Les deux limiteurs de la voiture et leur détection de fronts.
#[derive(Debug, Clone, Default)]
pub struct CarFlash {
    kick: FlashLimiter,
    blink: FlashLimiter,
    last_pulse: f32,
    last_lit: bool,
}

impl CarFlash {
    /// Avance d'une image. `policy` : `(flashes, breathing)` du spectacle, `None` sans spectacle ; `now` : horloge (s) ;
    /// `pulse` : pouls du kick de l'analyse (0 → 1) ; `lit` : le clignotant est-il à son temps allumé.
    pub fn update(&mut self, dt_s: f32, now: f64, policy: Option<(bool, bool)>, pulse: f32, lit: bool) {
        let (flashes, breathing) = policy.unwrap_or((false, true));
        self.kick.set_policy(flashes, breathing);
        self.blink.set_policy(flashes, breathing);
        if pulse > EDGE_HIGH && self.last_pulse < EDGE_LOW {
            self.kick.trigger(
                FlashRequest {
                    channel: Channel::Flash,
                    peak: Channel::Flash.cap(),
                    attack_s: KICK_ATTACK_S,
                    decay_s: KICK_DECAY_S,
                },
                now,
            );
        }
        if lit && !self.last_lit {
            self.blink.trigger(
                FlashRequest {
                    channel: Channel::Flash,
                    peak: Channel::Flash.cap(),
                    attack_s: BLINK_ATTACK_S,
                    decay_s: BLINK_DECAY_S,
                },
                now,
            );
        }
        self.last_pulse = pulse;
        self.last_lit = lit;
        self.kick.step(dt_s);
        self.blink.step(dt_s);
    }

    /// Le pouls que montrent les lumières, 0 → 1 : le niveau du limiteur ramené au plafond de sa voie.
    #[must_use]
    pub fn kick(&self) -> f32 {
        self.kick.value(Channel::Flash) / Channel::Flash.cap()
    }

    /// Le battement du clignotant, 0 → 1.
    #[must_use]
    pub fn blink(&self) -> f32 {
        self.blink.value(Channel::Flash) / Channel::Flash.cap()
    }

    /// Les flashs sont-ils permis en ce moment ?
    #[must_use]
    pub fn permitted(&self) -> bool {
        self.kick.permitted()
    }
}

/// Suit une cible sans jamais aller plus vite que `MIN_FADE` pour la parcourir en entier : une hausse brusque de l'entrée
/// (freinage, feux de route) n'est jamais une hausse brusque de lumière.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Slew {
    value: f32,
}

impl Slew {
    /// Valeur de départ.
    #[must_use]
    pub const fn new(value: f32) -> Self {
        Self { value }
    }

    /// Avance vers `target` (de pleine échelle `span`, soit la plus grande variation possible) : au plus `span / 0,1 s`
    /// par seconde, donc jamais en moins de `0,1 s` (> `MIN_FADE`).
    pub fn toward(&mut self, target: f32, dt_s: f32, span: f32) -> f32 {
        let step = span / (MIN_FADE + 0.02) * dt_s.max(0.0);
        self.value += (target - self.value).clamp(-step, step);
        self.value
    }

    /// Valeur actuelle.
    #[must_use]
    pub const fn value(&self) -> f32 {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    /// Rejoue `seconds` secondes à 60 i/s avec un kick toutes les `60 / bpm` s ; rend la série `kick()`.
    fn kicks(bpm: f64, seconds: f64, policy: Option<(bool, bool)>) -> Vec<f32> {
        let mut flash = CarFlash::default();
        let period = 60.0 / bpm;
        let frames = (seconds * 60.0) as usize;
        let mut series = Vec::with_capacity(frames);
        let mut next_kick = 0.0;
        for i in 0..frames {
            let now = i as f64 / 60.0;
            // Le pouls vaut 1 à l'image du kick, puis décroît en ~0,12 s (comme `AudioFeatures::pulse`).
            let pulse = if now >= next_kick {
                next_kick += period;
                1.0
            } else {
                (-((now - (next_kick - period)) as f32) / 0.12).exp()
            };
            flash.update(DT, now, policy, pulse, false);
            series.push(flash.kick());
        }
        series
    }

    /// Les montées d'une série : (image de départ, nombre d'images, niveau atteint). Une montée est une suite d'images
    /// où le niveau croît.
    fn rises(series: &[f32]) -> Vec<(usize, usize, f32)> {
        let mut out = Vec::new();
        let mut start = None;
        for i in 1..series.len() {
            let up = series[i] > series[i - 1] + 1e-6;
            match (up, start) {
                (true, None) => start = Some(i - 1),
                (false, Some(s)) => {
                    out.push((s, i - 1 - s, series[i - 1]));
                    start = None;
                }
                _ => {}
            }
        }
        out
    }

    /// À 174 BPM (un kick toutes les 0,345 s), le glow ne monte jamais plus de 3 fois par seconde sur une fenêtre
    /// glissante d'une seconde.
    #[test]
    fn a_174_bpm_le_glow_ne_monte_pas_plus_de_3_fois_par_seconde() {
        let series = kicks(174.0, 30.0, Some((true, false)));
        let starts: Vec<usize> = rises(&series).into_iter().map(|(s, _, _)| s).collect();
        assert!(starts.len() > 30, "il y a bien des montées : {}", starts.len());
        for &s in &starts {
            let in_window = starts.iter().filter(|&&t| t >= s && t < s + 60).count();
            assert!(
                in_window <= 3,
                "{in_window} montées dans la seconde qui suit l'image {s}"
            );
        }
    }

    /// Aucune montée n'est plus brusque que 5 images (à 60 i/s), et aucune ne dépasse le plafond de la voie.
    #[test]
    fn aucune_montee_en_moins_de_5_images() {
        for bpm in [90.0, 128.0, 174.0, 200.0] {
            let series = kicks(bpm, 20.0, Some((true, false)));
            for (start, frames, peak) in rises(&series) {
                // Une montée interrompue par un nouveau départ ne compte pas : seules comptent celles qui atteignent
                // le plafond, la montée complète dure `KICK_ATTACK_S` (6 images).
                if peak > 0.99 {
                    assert!(frames >= 5, "{bpm} BPM : montée de {frames} images à l'image {start}");
                }
                assert!(peak <= 1.0 + 1e-6, "plafond de la voie dépassé : {peak}");
            }
            let steepest = series.windows(2).map(|w| w[1] - w[0]).fold(0.0_f32, f32::max);
            assert!(
                steepest <= DT / MIN_FADE + 1e-4,
                "{bpm} BPM : pente {steepest} par image"
            );
        }
    }

    #[test]
    fn rien_ne_flashe_quand_les_flashs_sont_coupes() {
        for policy in [Some((false, false)), Some((true, true)), Some((false, true)), None] {
            let series = kicks(174.0, 10.0, policy);
            assert!(
                series.iter().all(|&v| v == 0.0),
                "{policy:?} : le glow ne doit pas bouger"
            );
            let mut flash = CarFlash::default();
            for i in 0..600 {
                flash.update(DT, f64::from(i) / 60.0, policy, 1.0, i % 40 < 20);
                assert!(flash.blink() == 0.0, "{policy:?} : le clignotant ne doit pas battre");
            }
        }
    }

    /// Flashs coupés en pleine montée ou en plein flash : tout retombe à 0 en `MIN_FADE` au plus, sans coupure sèche.
    #[test]
    fn couper_les_flashs_eteint_en_min_fade_sans_coupure_seche() {
        let mut flash = CarFlash::default();
        let mut now = 0.0;
        flash.update(DT, now, Some((true, false)), 1.0, true);
        for _ in 0..8 {
            now += f64::from(DT);
            flash.update(DT, now, Some((true, false)), 0.0, true);
        }
        let before = flash.kick();
        assert!(before > 0.5, "en plein flash : {before}");
        let mut previous = before;
        let mut frames = 0;
        while flash.kick() > 0.0 {
            now += f64::from(DT);
            flash.update(DT, now, Some((false, false)), 0.0, true);
            assert!(flash.kick() <= previous, "descente monotone");
            assert!(
                previous - flash.kick() <= DT / MIN_FADE * 1.0 + 1e-4,
                "pas de coupure sèche"
            );
            previous = flash.kick();
            frames += 1;
            assert!(frames < 10, "toujours allumé après {frames} images");
        }
        assert!(f64::from(DT) * f64::from(frames) <= f64::from(MIN_FADE) + 2.0 * f64::from(DT));
    }

    /// Le clignotant (1,5 Hz) passe par le limiteur : même plafond, montée de 6 images, au plus 3 par seconde.
    #[test]
    fn le_clignotant_est_borne_par_le_limiteur() {
        let mut flash = CarFlash::default();
        let mut series = Vec::new();
        for i in 0..1200 {
            let t = f64::from(i) / 60.0;
            // Allumé la première moitié de chaque cycle de 0,66 s.
            let lit = t % 0.66 < 0.33;
            flash.update(DT, t, Some((true, false)), 0.0, lit);
            series.push(flash.blink());
        }
        let found = rises(&series);
        assert!(found.len() >= 25, "il bat : {} montées en 20 s", found.len());
        for (start, frames, peak) in &found {
            assert!(
                *frames >= 5 && *peak > 0.99,
                "montée de {frames} images à l'image {start} (niveau {peak})"
            );
        }
        for &(s, _, _) in &found {
            assert!(found.iter().filter(|&&(t, _, _)| t >= s && t < s + 60).count() <= 3);
        }
    }

    /// Ce que montrent vraiment les lumières (la lueur sous la caisse de `LightLevels`) suit la même règle à 174 BPM.
    #[test]
    fn la_lueur_sous_la_caisse_respecte_la_meme_borne() {
        use super::super::glow::{CarPalette, LightInputs, LightLevels};
        use bevy::color::LinearRgba;
        let palette = CarPalette {
            violet: LinearRgba::rgb(0.14, 0.07, 0.54),
            rose: LinearRgba::rgb(1.0, 0.1, 0.03),
            ice: LinearRgba::rgb(0.8, 0.93, 1.0),
            alarm: LinearRgba::rgb(1.0, 0.01, 0.01),
            amber: LinearRgba::rgb(1.0, 0.5, 0.0),
        };
        let luma: Vec<f32> = kicks(174.0, 20.0, Some((true, false)))
            .into_iter()
            .map(|kick| {
                let levels = LightLevels::compute(
                    &palette,
                    &LightInputs {
                        glow: palette.violet,
                        pulse: kick,
                        brake: 0.0,
                        speed_ratio: 1.0,
                        level: 1.1,
                    },
                );
                levels.under.red + levels.under.green + levels.under.blue
            })
            .collect();
        let found = rises(&luma);
        for &(s, frames, _) in &found {
            assert!(found.iter().filter(|&&(t, _, _)| t >= s && t < s + 60).count() <= 3);
            let _ = frames;
        }
        let steepest = luma.windows(2).map(|w| w[1] - w[0]).fold(0.0_f32, f32::max);
        let swing = luma.iter().copied().fold(0.0_f32, f32::max) - luma.iter().copied().fold(f32::MAX, f32::min);
        assert!(
            steepest <= swing * DT / MIN_FADE + 1e-4,
            "pente {steepest} pour une amplitude de {swing}"
        );
    }

    #[test]
    fn le_lissage_ne_va_jamais_plus_vite_que_min_fade() {
        let mut slew = Slew::new(0.0);
        let mut frames = 0;
        while slew.toward(1.0, DT, 1.0) < 1.0 {
            frames += 1;
            assert!(frames < 20);
        }
        assert!(
            f64::from(DT) * f64::from(frames) >= f64::from(MIN_FADE),
            "{frames} images pour 0 → 1"
        );
        assert!(slew.toward(0.0, DT, 1.0) > 0.0, "la descente est lissée aussi");
        assert!((slew.value() - slew.toward(slew.value(), DT, 1.0)).abs() < f32::EPSILON);
    }
}
