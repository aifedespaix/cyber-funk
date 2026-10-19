//! Limiteur de sécurité photosensible (portage de `engine/show-fx-limiter.ts` de Sillage).
//!
//! Tout flash plein écran ou toute forte variation de luminance du spectacle passe par ici, **en dernier**, quelle que
//! soit la demande (look, macro, opérateur). Garanties (WCAG 2.3.1), couvertes chacune par une propriété `proptest`
//! (`tests/limiter_props.rs`) :
//!
//! 1. au plus `MAX_FLASHES_PER_SECOND` flashs par seconde glissante ;
//! 2. aucune variation de luminance plus rapide que `MIN_FADE` (montée comme descente) ;
//! 3. amplitude plafonnée : `MAX_FLASH_PEAK` pour le flash, `MAX_SWEEP_PEAK` pour la vague ;
//! 4. écart minimal `MIN_GAP` entre deux départs ;
//! 5. jamais de rouge saturé plein écran (`defuse_red`, `SafeColor`) ;
//! 6. rien du tout quand les flashs sont coupés (`FlashKill`) ou en mode respiration.

mod color;

pub use color::{Rgb, SafeColor, defuse_red, is_saturated_red};

/// Nombre maximal de flashs par seconde glissante (toutes voies confondues).
pub const MAX_FLASHES_PER_SECOND: usize = 3;
/// Durée minimale (s) d'une montée ou d'une descente complète à l'amplitude maximale.
pub const MIN_FADE: f32 = 0.08;
/// Plafond d'un flash plein écran (lumière ajoutée, 0 → 1).
pub const MAX_FLASH_PEAK: f32 = 0.4;
/// Plafond de la vague de lumière de changement de niveau.
pub const MAX_SWEEP_PEAK: f32 = 0.14;
/// Écart minimal (s) entre deux départs, pour ne pas les grouper.
pub const MIN_GAP: f64 = 0.25;
/// Un trou d'image plus long ne raccourcit pas un fondu : le temps du limiteur est borné (s).
pub const MAX_STEP: f32 = 1.0 / 30.0;
/// Durée (s) maximale d'une montée ou d'une descente demandée. Sillage n'a pas de borne haute ; sans elle, une durée
/// infinie bloquerait la voie à jamais (écart documenté dans la PR).
pub const MAX_FADE: f32 = 30.0;
/// Fenêtre glissante (s) du compte de flashs.
const WINDOW: f64 = 1.0;
/// Sous ce pic une demande est ignorée.
const MIN_PEAK: f32 = 0.001;
/// Au-dessus de ce niveau une voie est considérée occupée : pas de nouveau départ pendant sa descente.
const BUSY_LEVEL: f32 = 0.02;

const CHANNELS: usize = 2;

/// Les deux voies : le flash bref, et la vague lente de changement de niveau.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Channel {
    /// Flash bref (`FX_FLASH` dans Sillage).
    Flash = 0,
    /// Vague lente de changement de niveau (`FX_SWEEP` dans Sillage).
    Sweep = 1,
}

impl Channel {
    /// Les deux voies.
    pub const ALL: [Self; CHANNELS] = [Self::Flash, Self::Sweep];

    /// Plafond d'amplitude de la voie.
    #[must_use]
    pub const fn cap(self) -> f32 {
        match self {
            Self::Flash => MAX_FLASH_PEAK,
            Self::Sweep => MAX_SWEEP_PEAK,
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// Demande de flash : amplitude et durées demandées, avant que le limiteur ne les ramène.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlashRequest {
    /// Voie visée.
    pub channel: Channel,
    /// Amplitude demandée (lumière ajoutée, 0 → 1), plafonnée par la voie.
    pub peak: f32,
    /// Durée de montée demandée (s), relevée à `MIN_FADE` au moins.
    pub attack_s: f32,
    /// Durée de descente demandée (s), relevée à `MIN_FADE` au moins.
    pub decay_s: f32,
}

/// État du limiteur. Aucune allocation, aucun verrou : sûr à appeler à chaque image.
///
/// Écrit par le seul système de post-process (`trigger`, `set_policy`, `step`), lu par ses sorties (`value`).
#[derive(Clone, Debug)]
pub struct FlashLimiter {
    allowed: bool,
    /// Départs récents (s), en anneau : la plus ancienne entrée est à `cursor`.
    starts: [f64; MAX_FLASHES_PER_SECOND],
    cursor: usize,
    last_start: f64,
    level: [f32; CHANNELS],
    peak: [f32; CHANNELS],
    rise: [f32; CHANNELS],
    fall: [f32; CHANNELS],
    rising: [bool; CHANNELS],
}

impl Default for FlashLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl FlashLimiter {
    /// Limiteur au repos, flashs permis.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            allowed: true,
            starts: [f64::NEG_INFINITY; MAX_FLASHES_PER_SECOND],
            cursor: 0,
            last_start: f64::NEG_INFINITY,
            level: [0.0; CHANNELS],
            peak: [0.0; CHANNELS],
            rise: [0.0; CHANNELS],
            fall: [0.0; CHANNELS],
            rising: [false; CHANNELS],
        }
    }

    /// Flashs permis seulement si le réglage est actif et que le mode respiration ne l'interdit pas.
    ///
    /// En passant à « interdit », ce qui brille encore redescend à 0 en `MIN_FADE` au plus (jamais de coupure sèche).
    pub fn set_policy(&mut self, flashes: bool, breathing: bool) {
        let next = flashes && !breathing;
        if next == self.allowed {
            return;
        }
        self.allowed = next;
        if next {
            return;
        }
        for c in 0..CHANNELS {
            self.rising[c] = false;
            self.fall[c] = self.level[c] / MIN_FADE;
        }
    }

    /// Les flashs sont-ils permis en ce moment ?
    #[must_use]
    pub const fn permitted(&self) -> bool {
        self.allowed
    }

    /// Demande un flash à l'instant `now` (s, horloge du set) : renvoie vrai s'il est accordé.
    ///
    /// Refusé si les flashs sont coupés, si la voie est occupée, si le dernier départ date de moins de `MIN_GAP`, si
    /// `MAX_FLASHES_PER_SECOND` départs ont déjà eu lieu dans la dernière seconde, ou si un argument est NaN (les
    /// comparaisons seraient fausses et laisseraient tout passer). Une horloge qui recule refuse aussi.
    pub fn trigger(&mut self, request: FlashRequest, now: f64) -> bool {
        let FlashRequest {
            channel,
            peak,
            attack_s,
            decay_s,
        } = request;
        let c = channel.index();
        // NaN est refusé explicitement : toutes ses comparaisons sont fausses et laisseraient tout passer.
        if !self.allowed
            || !now.is_finite()
            || peak.is_nan()
            || peak <= MIN_PEAK
            || attack_s.is_nan()
            || decay_s.is_nan()
        {
            return false;
        }
        if self.level[c] > BUSY_LEVEL || self.rising[c] {
            return false;
        }
        if now - self.last_start < MIN_GAP {
            return false;
        }
        if now - self.starts[self.cursor] < WINDOW {
            return false;
        }
        self.starts[self.cursor] = now;
        self.cursor = (self.cursor + 1) % MAX_FLASHES_PER_SECOND;
        self.last_start = now;
        let p = peak.min(channel.cap());
        self.peak[c] = p;
        self.rising[c] = true;
        self.rise[c] = p / attack_s.clamp(MIN_FADE, MAX_FADE);
        self.fall[c] = p / decay_s.clamp(MIN_FADE, MAX_FADE);
        true
    }

    /// Fait avancer les fondus de `dt_s` secondes (borné à `MAX_STEP`). Un `dt_s` négatif ou non fini est ignoré.
    pub fn step(&mut self, dt_s: f32) {
        if !dt_s.is_finite() || dt_s <= 0.0 {
            return;
        }
        let d = dt_s.min(MAX_STEP);
        for c in 0..CHANNELS {
            if self.rising[c] {
                self.level[c] += self.rise[c] * d;
                if self.level[c] >= self.peak[c] {
                    self.level[c] = self.peak[c];
                    self.rising[c] = false;
                }
            } else if self.level[c] > 0.0 {
                self.level[c] = (self.level[c] - self.fall[c] * d).max(0.0);
            }
        }
    }

    /// Lumière ajoutée par la voie, déjà bornée (0 → plafond de la voie).
    #[must_use]
    pub const fn value(&self, channel: Channel) -> f32 {
        self.level[channel.index()]
    }
}
