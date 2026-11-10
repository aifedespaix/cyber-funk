//! La chaîne de filtrage d'une route (P2 §2.2), de la bande brute à la valeur de la cible.

use cf_analysis::AudioFeatures;
use cf_analysis::filters::{
    AttackRelease, LevelAgc, OneEuro, PeakHold, Schmitt, SlewLimiter, Spring, curve_pow, dead_zone, sat,
};
use cf_show::limiter::{Channel, FlashLimiter, FlashRequest, MIN_FADE};

use super::types::{Curve, Detector, LimiterLane, Quantize, Route, Smooth, Source};

/// Seuils de la bascule de Schmitt de l'étape 4 (P2 §2.2).
const THETA_ON: f32 = 0.12;
const THETA_OFF: f32 = 0.08;
/// Largeur (en écarts-types au-delà du seuil) sur laquelle un flux passe de 0 à 1.
const FLUX_SPAN: f32 = 4.0;
/// Seuils de détection de front des routes événementielles et des routes sous limiteur.
const EDGE_ON: f32 = 0.5;
const EDGE_OFF: f32 = 0.3;
/// Retombée (s) d'un événement tiré.
const EVENT_DECAY_S: f32 = 0.08;

/// Ce que la chaîne sait de l'image.
pub(super) struct Frame<'a> {
    pub features: &'a AudioFeatures,
    pub dt: f32,
    pub now: f64,
    /// Phase de la mesure vue à l'écran.
    pub bar_phase: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Valeur continue, sans limiteur.
    Plain,
    /// Valeur continue qui est une variation de luminance : sous limiteur.
    Limited,
    /// Événement (flux ou quantifié) : tiré, puis retombée.
    Event,
}

/// L'état d'une route.
#[derive(Debug, Clone)]
pub(super) struct Chain {
    route: Route,
    kind: Kind,
    agc: LevelAgc,
    gate: Schmitt,
    attack_release: AttackRelease,
    one_euro: OneEuro,
    spring: Spring,
    peak: PeakHold,
    slew: SlewLimiter,
    /// Front du signal (détection) et front en attente de la prochaine frontière de grille.
    edge: Schmitt,
    pending: Option<f32>,
    previous_cell: Option<i32>,
    denied: bool,
    pulse: PeakHold,
    lane_slew: SlewLimiter,
}

impl Chain {
    pub(super) fn new(route: &Route) -> Self {
        let (attack, release) = match route.smooth {
            Smooth::AttackRelease { attack_s, release_s }
            | Smooth::AttackReleaseSpring {
                attack_s, release_s, ..
            } => (attack_s, release_s),
            _ => (0.0, 0.0),
        };
        let (f_min, beta) = match route.smooth {
            Smooth::OneEuro { f_min, beta } => (f_min, beta),
            _ => (1.0, 0.0),
        };
        let (decay, hold) = match route.detector {
            Detector::Peak { decay_s, hold_s } => (decay_s, hold_s),
            // Le spectre garde sa crête 200 ms (vu-mètre des fenêtres).
            Detector::Spectrum { .. } => (0.2, 0.0),
            _ => (0.0, 0.0),
        };
        let kind = if matches!(route.detector, Detector::Flux { .. }) || route.quantize != Quantize::None {
            Kind::Event
        } else if route.limiter == LimiterLane::None {
            Kind::Plain
        } else {
            Kind::Limited
        };
        Self {
            route: route.clone(),
            kind,
            agc: LevelAgc::new(),
            gate: Schmitt::new(THETA_ON, THETA_OFF),
            attack_release: AttackRelease::new(attack, release),
            one_euro: OneEuro::new(f_min, beta),
            spring: Spring::new(),
            peak: PeakHold::new(decay, hold),
            slew: SlewLimiter::new(route.slew.up, route.slew.down),
            edge: Schmitt::new(EDGE_ON, EDGE_OFF),
            pending: None,
            previous_cell: None,
            denied: false,
            pulse: PeakHold::new(EVENT_DECAY_S, 0.0),
            lane_slew: SlewLimiter::new(1.0 / MIN_FADE, 1.0 / MIN_FADE),
        }
    }

    pub(super) const fn target(&self) -> super::types::Target {
        self.route.target
    }

    /// Le signal d'entrée de la route, avant la chaîne.
    fn input(&mut self, f: &Frame<'_>) -> f32 {
        let a = f.features;
        let b = &a.sub_bands;
        if let Detector::Flux { z } = self.route.detector {
            let score = match self.route.source {
                Source::Punch | Source::SubBass => a.z_pb,
                Source::Transient => a.z_tr,
                _ => a.hfc_z,
            };
            return sat((score - z) / FLUX_SPAN);
        }
        if matches!(self.route.detector, Detector::Spectrum { .. }) {
            return a.spectrum.iter().sum::<f32>() / a.spectrum.len() as f32;
        }
        let band = |rms: f32, chain: &mut Self| {
            let x = chain.agc.step(rms * rms, f.dt);
            let x = match chain.route.curve {
                Curve::Linear => x,
                Curve::Pow(g) => curve_pow(x, g),
            };
            // Étape 4 : Schmitt, sinon zone morte rééchelonnée.
            if chain.gate.update(x) {
                dead_zone(x, THETA_OFF)
            } else {
                0.0
            }
        };
        match self.route.source {
            Source::SubBass => band(b.sb, self),
            Source::Punch => band(b.pb, self),
            Source::LowMid => band(b.lm, self),
            Source::HighMid => band(b.hm, self),
            Source::Transient => band(b.tr, self),
            Source::Slow => sat(a.intensity),
            Source::Fast => sat(a.flux),
            Source::Phase => (f.bar_phase * 4.0).rem_euclid(1.0),
            Source::Bar => f.bar_phase,
        }
    }

    /// Lissage, crête, pente, gain et bornes.
    fn shape(&mut self, x: f32, dt: f32) -> f32 {
        let mut y = match self.route.smooth {
            Smooth::None => x,
            Smooth::AttackRelease { .. } => self.attack_release.step(x, dt),
            Smooth::AttackReleaseSpring { hz, zeta, .. } => {
                let s = self.attack_release.step(x, dt);
                self.spring.step(s, hz, zeta, dt)
            }
            Smooth::OneEuro { .. } => self.one_euro.step(x, dt),
            Smooth::Spring { hz, zeta } => self.spring.step(x, hz, zeta, dt),
        };
        if matches!(self.route.detector, Detector::Peak { .. } | Detector::Spectrum { .. }) {
            y = self.peak.step(y, dt);
        }
        y = self.slew.step(y, dt);
        (y * self.route.gain).clamp(self.route.clamp.0, self.route.clamp.1)
    }

    fn lane(&self) -> Channel {
        if self.route.limiter == LimiterLane::Sweep {
            Channel::Sweep
        } else {
            Channel::Flash
        }
    }

    /// Une demande de départ au limiteur. Vrai si elle est acceptée.
    fn ask(&self, limiter: &mut FlashLimiter, now: f64) -> bool {
        limiter.trigger(
            FlashRequest {
                channel: self.lane(),
                peak: self.lane().cap(),
                attack_s: MIN_FADE,
                decay_s: EVENT_DECAY_S.max(MIN_FADE),
            },
            now,
        )
    }

    /// Avance d'une image et rend la valeur de la cible.
    pub(super) fn step(&mut self, f: &Frame<'_>, limiter: &mut FlashLimiter) -> f32 {
        let x = self.input(f);
        let y = self.shape(x, f.dt);
        match self.kind {
            Kind::Plain => y,
            Kind::Limited => self.limited(y, f, limiter),
            Kind::Event => self.event(y, f, limiter),
        }
    }

    /// Valeur continue sous limiteur : chaque montée franchissant le seuil est un départ ; refusé, la route se tait
    /// jusqu'à ce que le signal retombe. La variation ne dépasse jamais `1/MIN_FADE` par seconde.
    fn limited(&mut self, y: f32, f: &Frame<'_>, limiter: &mut FlashLimiter) -> f32 {
        let was_high = self.edge.is_high();
        let high = self.edge.update(y);
        if high && !was_high {
            self.denied = !self.ask(limiter, f.now);
        } else if !high {
            self.denied = false;
        }
        let allowed = if self.denied || !limiter.permitted() { 0.0 } else { y };
        self.lane_slew.step(allowed, f.dt)
    }

    /// Événement : le front est tiré tout de suite, ou à la prochaine frontière de la grille quantifiée.
    fn event(&mut self, y: f32, f: &Frame<'_>, limiter: &mut FlashLimiter) -> f32 {
        let was_high = self.edge.is_high();
        let high = self.edge.update(y);
        if high && !was_high {
            self.pending = Some(self.pending.map_or(y, |p| p.max(y)));
        } else if high && let Some(p) = &mut self.pending {
            *p = p.max(y);
        }
        let fire = match self.route.quantize.per_bar() {
            None => self.pending.take(),
            Some(per_bar) => {
                let cell = (f.bar_phase * per_bar).floor() as i32;
                let crossed = self.previous_cell.is_some_and(|c| c != cell);
                self.previous_cell = Some(cell);
                if crossed { self.pending.take() } else { None }
            }
        };
        if let Some(level) = fire {
            let accepted = self.route.limiter == LimiterLane::None || self.ask(limiter, f.now);
            if accepted {
                self.pulse.step(level, 0.0);
            }
        }
        let out = self.pulse.step(0.0, f.dt);
        if self.route.limiter == LimiterLane::None {
            out
        } else {
            self.lane_slew.step(if limiter.permitted() { out } else { 0.0 }, f.dt)
        }
    }
}
