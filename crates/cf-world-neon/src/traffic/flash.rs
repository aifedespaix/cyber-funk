//! Sécurité photosensible du trafic (règle 7 de `CLAUDE.md`, WCAG 2.3.1) : toute variation brusque de luminance des
//! véhicules passe par le [`FlashLimiter`] de `cf-show`, comme le font la voiture (`car::safety`) et le post-process.
//!
//! Sont concernés : le pouls des bandes néon et des pièces d'accent sur les kicks (`uPulse` de
//! `vehicle-material.ts`) et les gyrophares de la police, qui alternaient en tout ou rien à 2,6 Hz dans Sillage.
//! Chaque source a **son** limiteur, avec la même politique que la voiture : permis seulement si le spectacle le permet
//! (`flashes`) et hors respiration ; sans spectacle, rien ne flashe. Ce que le shader montre est le niveau que le
//! limiteur rend : au plus 3 départs par seconde, montée et descente d'au moins `MIN_FADE`, plafond de la voie.
//!
//! Les gyrophares battent **ensemble** pour tous les véhicules (une horloge commune, au lieu d'une phase par véhicule) :
//! sans cela, six voitures de police feraient dix-huit départs par seconde sur l'image. Chaque côté part une fois par
//! période (1 s), gauche puis droite : deux départs par seconde au total, sous le plafond de trois.
//!
//! Les événements spectaculaires (J7-05) passent aussi par ici : l'éclair d'orage, l'éclat d'une salve de feu d'artifice
//! et les variations de lumière de toute la ville (panne de courant) **demandent** un départ dans
//! [`SceneFx`](super::scene::SceneFx) ; [`TrafficFlash::serve`] les présente au limiteur de leur voie et publie dans
//! `SceneFx` ce qu'il accorde. Un événement ne règle jamais lui-même un éclat plein écran.

use bevy::ecs::resource::Resource;
use cf_show::limiter::{Channel, FlashLimiter, FlashRequest};

use super::scene::SceneFx;

/// Période du feu à éclats des gyrophares (s) : un départ rouge et un départ bleu par période. Sillage : 0,385 s
/// (2,6 Hz) en tout ou rien, ce que le limiteur ne permettrait pas.
pub const BEACON_PERIOD_S: f64 = 1.0;
/// Montée d'un éclat de gyrophare ou d'un pouls de kick (s) : un peu plus que `MIN_FADE`, soit 6 images à 60 i/s.
pub const ATTACK_S: f32 = 0.1;
/// Descente d'un éclat de gyrophare (s).
pub const BEACON_DECAY_S: f32 = 0.25;
/// Descente du pouls de kick (s).
pub const KICK_DECAY_S: f32 = 0.25;
/// Montée et descente d'un éclair d'orage (s). Sillage : 0,42 s en tout, avec un scintillement ; la montée est celle du
/// limiteur.
pub const LIGHTNING_ATTACK_S: f32 = 0.1;
/// Descente de l'éclair, s.
pub const LIGHTNING_DECAY_S: f32 = 0.32;
/// Montée et descente de l'éclat d'une salve (s).
pub const BURST_ATTACK_S: f32 = 0.1;
/// Descente de l'éclat d'une salve, s.
pub const BURST_DECAY_S: f32 = 0.2;
/// Une variation de lumière de toute la ville (début ou fin d'une panne de courant) est présentée à la voie « vague » :
/// elle compte dans les trois départs par seconde et suit la politique (rien en respiration, rien si les flashs sont
/// coupés).
pub const WIDE_ATTACK_S: f32 = 0.3;
/// Descente du départ « vague » (s) : la voie est de nouveau libre pour le retour de la lumière.
pub const WIDE_DECAY_S: f32 = 0.5;
/// Le pouls doit passer de sous `EDGE_LOW` à au-dessus de `EDGE_HIGH` pour compter pour un kick.
const EDGE_HIGH: f32 = 0.8;
const EDGE_LOW: f32 = 0.5;

fn request(attack_s: f32, decay_s: f32) -> FlashRequest {
    FlashRequest {
        channel: Channel::Flash,
        peak: Channel::Flash.cap(),
        attack_s,
        decay_s,
    }
}

/// Les limiteurs du trafic : le pouls des kicks, le gyrophare gauche (rouge), le droit (bleu), puis ceux des événements
/// spectaculaires : l'éclair d'orage, l'éclat d'une salve, la variation de lumière de toute la ville.
#[derive(Resource, Debug, Clone, Default)]
pub struct TrafficFlash {
    kick: FlashLimiter,
    left: FlashLimiter,
    right: FlashLimiter,
    lightning: FlashLimiter,
    burst: FlashLimiter,
    wide: FlashLimiter,
    last_pulse: f32,
    last_left: bool,
    last_right: bool,
}

impl TrafficFlash {
    /// Avance d'une image. `policy` : `(flashes, breathing)` du spectacle, `None` sans spectacle ; `now` : horloge de la
    /// scène (s) ; `pulse` : pouls du kick de l'analyse (0 → 1).
    pub fn update(&mut self, dt_s: f32, now: f64, policy: Option<(bool, bool)>, pulse: f32) {
        let (flashes, breathing) = policy.unwrap_or((false, true));
        for limiter in self.limiters_mut() {
            limiter.set_policy(flashes, breathing);
        }
        if pulse > EDGE_HIGH && self.last_pulse < EDGE_LOW {
            self.kick.trigger(request(ATTACK_S, KICK_DECAY_S), now);
        }
        self.last_pulse = pulse;

        // Gauche pendant la première moitié de la période, droite pendant la seconde.
        let phase = (now / BEACON_PERIOD_S).rem_euclid(1.0);
        let (left_on, right_on) = (phase < 0.5, phase >= 0.5);
        if left_on && !self.last_left {
            self.left.trigger(request(ATTACK_S, BEACON_DECAY_S), now);
        }
        if right_on && !self.last_right {
            self.right.trigger(request(ATTACK_S, BEACON_DECAY_S), now);
        }
        self.last_left = left_on;
        self.last_right = right_on;

        for limiter in self.limiters_mut() {
            limiter.step(dt_s);
        }
    }

    fn limiters_mut(&mut self) -> [&mut FlashLimiter; 6] {
        [
            &mut self.kick,
            &mut self.left,
            &mut self.right,
            &mut self.lightning,
            &mut self.burst,
            &mut self.wide,
        ]
    }

    /// Présente les demandes des événements à leurs limiteurs et publie dans `fx` ce qui est accordé : le pouls du kick,
    /// le blanchissement de l'éclair, l'éclat de la salve (0 → 1 chacun), si les flashs sont permis, et si la dernière
    /// variation de lumière de toute la ville l'a été. À appeler après [`TrafficFlash::update`], à chaque image.
    pub fn serve(&mut self, now: f64, fx: &mut SceneFx) {
        if fx.lightning_request {
            self.lightning
                .trigger(request(LIGHTNING_ATTACK_S, LIGHTNING_DECAY_S), now);
        }
        if fx.burst_request {
            self.burst.trigger(request(BURST_ATTACK_S, BURST_DECAY_S), now);
        }
        if fx.wide_request {
            fx.wide_ok = self.wide.trigger(
                FlashRequest {
                    channel: Channel::Flash,
                    peak: Channel::Flash.cap(),
                    attack_s: WIDE_ATTACK_S,
                    decay_s: WIDE_DECAY_S,
                },
                now,
            );
        }
        fx.kick = self.pulse();
        fx.lightning = self.lightning.value(Channel::Flash) / Channel::Flash.cap();
        fx.burst = self.burst.value(Channel::Flash) / Channel::Flash.cap();
        fx.flashes = self.permitted();
    }

    /// Le pouls que montrent les bandes néon, 0 → 1 : le niveau du limiteur ramené au plafond de sa voie.
    #[must_use]
    pub fn pulse(&self) -> f32 {
        self.kick.value(Channel::Flash) / Channel::Flash.cap()
    }

    /// L'éclat du gyrophare gauche (rouge), 0 → 1.
    #[must_use]
    pub fn beacon_left(&self) -> f32 {
        self.left.value(Channel::Flash) / Channel::Flash.cap()
    }

    /// L'éclat du gyrophare droit (bleu), 0 → 1.
    #[must_use]
    pub fn beacon_right(&self) -> f32 {
        self.right.value(Channel::Flash) / Channel::Flash.cap()
    }

    /// Les flashs sont-ils permis en ce moment ?
    #[must_use]
    pub fn permitted(&self) -> bool {
        self.kick.permitted()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_show::limiter::{MAX_FLASHES_PER_SECOND, MIN_FADE};

    const DT: f32 = 1.0 / 60.0;

    /// Rejoue `seconds` secondes avec un kick toutes les 0,25 s (le pire cas) et rend les niveaux de chaque image.
    fn run(policy: Option<(bool, bool)>, seconds: f32) -> Vec<(f32, f32, f32)> {
        let mut flash = TrafficFlash::default();
        let mut out = Vec::new();
        let frames = (seconds / DT) as usize;
        for i in 0..frames {
            let now = f64::from(DT) * i as f64;
            let pulse = if i % 15 == 0 {
                1.0
            } else if i % 15 < 3 {
                0.9
            } else {
                0.0
            };
            flash.update(DT, now, policy, pulse);
            out.push((flash.pulse(), flash.beacon_left(), flash.beacon_right()));
        }
        out
    }

    #[test]
    fn sans_spectacle_rien_ne_flashe() {
        for (kick, left, right) in run(None, 5.0) {
            assert_eq!((kick, left, right), (0.0, 0.0, 0.0));
        }
    }

    #[test]
    fn flashs_coupes_ou_respiration_eteignent_tout() {
        for policy in [Some((false, false)), Some((true, true))] {
            assert!(
                run(policy, 5.0)
                    .iter()
                    .all(|&(k, l, r)| k == 0.0 && l == 0.0 && r == 0.0),
                "{policy:?}"
            );
        }
    }

    #[test]
    fn les_gyrophares_battent_et_restent_sous_le_plafond_de_trois_departs_par_seconde() {
        let frames = run(Some((true, false)), 10.0);
        let left_peak = frames.iter().map(|f| f.1).fold(0.0, f32::max);
        let right_peak = frames.iter().map(|f| f.2).fold(0.0, f32::max);
        assert!(
            left_peak > 0.9 && right_peak > 0.9,
            "les deux côtés s'allument ({left_peak}, {right_peak})"
        );
        // Départs : franchissements de 0,5 vers le haut, gauche et droite ensemble, sur chaque seconde glissante.
        let mut starts = Vec::new();
        for i in 1..frames.len() {
            for (prev, now) in [(frames[i - 1].1, frames[i].1), (frames[i - 1].2, frames[i].2)] {
                if prev < 0.5 && now >= 0.5 {
                    starts.push(i);
                }
            }
        }
        let window = (1.0 / DT) as usize;
        for &s in &starts {
            let in_second = starts.iter().filter(|&&t| t >= s && t < s + window).count();
            assert!(
                in_second <= MAX_FLASHES_PER_SECOND,
                "{in_second} départs en 1 s à l'image {s}"
            );
        }
    }

    #[test]
    fn aucune_montee_ni_descente_plus_rapide_que_min_fade() {
        let frames = run(Some((true, false)), 10.0);
        // Le plus grand saut d'une image à l'autre, ramené à une pleine échelle, ne vaut jamais plus de dt / MIN_FADE.
        let max_step = DT / MIN_FADE + 1e-3;
        for pair in frames.windows(2) {
            for (a, b) in [(pair[0].0, pair[1].0), (pair[0].1, pair[1].1), (pair[0].2, pair[1].2)] {
                assert!((a - b).abs() <= max_step, "saut de {} entre deux images", (a - b).abs());
            }
        }
    }
}
