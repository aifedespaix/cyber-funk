//! Densité visuelle `Δ ∈ [0, 1]` (DA §4.2, chantier C1) : une seule grandeur que lisent tous les systèmes, dérivée de
//! ce que le show publie déjà (`intensity`, `Section`, état du drop). Elle ne remplace pas les portes du look : elle
//! les module.
//!
//! ```text
//! cible   = table[section]·(0.7 + 0.3·intensity)
//! Δ      += (cible − Δ)·(1 − exp(−Δt/τ)) ,  τ = 2·T_b à la montée, 8·T_b à la descente
//! fatigue : Δ > 0.8 depuis plus de 32 temps → plafond = 1 − 0.3·sat((t_haut − 32·T_b)/(32·T_b))
//! pré-drop: Δ → 0.15 pendant le dernier temps avant la frappe                                   [L]
//! ```
//!
//! Tout se compte en temps (`T_b = 60/BPM`) : la même musique donne la même courbe à toute cadence d'image.
//!
//! # Pré-drop et limiteur
//!
//! Le creux d'avant-drop est une variation de luminance brutale (des fenêtres s'éteignent d'un coup). Il passe donc
//! par un [`FlashLimiter`] dédié, voie `Sweep` : pas de creux si les flashs sont coupés ou en mode respiration, ni si
//! le limiteur refuse le départ (cadence, écart minimal) ; et `Δ` ne varie jamais plus vite que `1/MIN_FADE` par
//! seconde, montée comme descente. La frappe n'est pas connue à l'avance dans la musique en direct : le chef du
//! spectacle déclare un creux possible quand une montée a armé un drop et que la fin de phrase approche (voir
//! [`DensityInput::strike_in_beats`]).

use cf_analysis::Section;

use crate::limiter::{Channel, FlashLimiter, FlashRequest, MAX_SWEEP_PEAK, MIN_FADE};

/// Valeur de `Δ` à l'ouverture.
pub const INITIAL: f32 = 0.3;
/// `Δ` pendant le creux d'avant-drop.
pub const PREDROP_LEVEL: f32 = 0.15;
/// `Δ` pendant une mesure d'épure imposée (apnée).
pub const EPURE_LEVEL: f32 = 0.12;
/// Seuil de fatigue : au-dessus, le compteur de temps hauts tourne.
pub const FATIGUE_THRESHOLD: f32 = 0.8;
/// Seuil « bas » de la respiration imposée.
pub const BREATH_THRESHOLD: f32 = 0.4;
/// Réglages de la courbe (valeurs de départ de DA §4.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DensityTuning {
    /// `table[section]` : calme, groove, sommet.
    pub table: [f32; 3],
    /// Constante de temps de montée (en temps).
    pub rise_beats: f32,
    /// Constante de temps de descente (en temps).
    pub fall_beats: f32,
    /// Constante de temps de montée juste après la frappe (en temps) : un drop ne se fait pas attendre.
    pub strike_rise_beats: f32,
    /// Durée (en temps) pendant laquelle la montée rapide de la frappe s'applique.
    pub strike_beats: f32,
    /// Temps passés haut avant que la fatigue ne commence.
    pub fatigue_after_beats: f32,
    /// Temps de la rampe de fatigue.
    pub fatigue_ramp_beats: f32,
    /// Perte maximale du plafond par fatigue.
    pub fatigue_loss: f32,
    /// Temps sans passage sous [`BREATH_THRESHOLD`] avant une mesure d'épure.
    pub breath_after_beats: f32,
    /// Durée d'une mesure d'épure (en temps).
    pub epure_beats: f32,
    /// Longueur normale du creux d'avant-drop (en temps).
    pub predrop_beats: f32,
    /// Longueur maximale du creux quand le climax manque de contraste (une mesure).
    pub predrop_max_beats: f32,
    /// Contraste minimal climax / moyenne des 8 mesures précédentes.
    pub contrast: f32,
    /// Plafond de `Δ` en mode respiration.
    pub breathing_cap: f32,
}

impl Default for DensityTuning {
    fn default() -> Self {
        Self {
            table: [0.35, 0.65, 1.0],
            rise_beats: 2.0,
            fall_beats: 8.0,
            strike_rise_beats: 0.25,
            strike_beats: 2.0,
            fatigue_after_beats: 32.0,
            fatigue_ramp_beats: 32.0,
            fatigue_loss: 0.3,
            breath_after_beats: 64.0,
            epure_beats: 4.0,
            predrop_beats: 1.0,
            predrop_max_beats: 4.0,
            contrast: 2.0,
            breathing_cap: 0.5,
        }
    }
}

/// Les cinq états de `Δ` (tableau de DA §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DensityBand {
    /// 0–0,15.
    Apnea,
    /// 0,15–0,35.
    Calm,
    /// 0,35–0,6.
    #[default]
    Groove,
    /// 0,6–0,8.
    Rise,
    /// 0,8–1.
    Climax,
}

impl DensityBand {
    /// Bornes hautes des quatre premiers états.
    const EDGES: [f32; 4] = [0.15, 0.35, 0.6, 0.8];
    /// Marge d'hystérésis : un état n'est quitté qu'au-delà de la frontière de cette marge.
    const MARGIN: f32 = 0.02;

    const ALL: [Self; 5] = [Self::Apnea, Self::Calm, Self::Groove, Self::Rise, Self::Climax];

    /// Rang 0..4.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// État de `delta`, avec hystérésis par rapport à l'état précédent.
    #[must_use]
    pub fn of(delta: f32, previous: Self) -> Self {
        let i = previous.index();
        let low = if i == 0 {
            f32::NEG_INFINITY
        } else {
            Self::EDGES[i - 1] - Self::MARGIN
        };
        let high = if i == 4 {
            f32::INFINITY
        } else {
            Self::EDGES[i] + Self::MARGIN
        };
        if (low..high).contains(&delta) {
            return previous;
        }
        let rank = Self::EDGES.iter().filter(|&&edge| delta >= edge).count();
        Self::ALL[rank]
    }

    /// Part des fenêtres allumées de DA §4.2 (5, 20, 35, 45, 60 %), pour les tests et le panneau opérateur.
    #[must_use]
    pub const fn lit_fraction(self) -> f32 {
        [0.05, 0.20, 0.35, 0.45, 0.60][self.index()]
    }
}

/// Ce que le chef du spectacle publie de la densité à chaque image.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "deux drapeaux d'état indépendants publiés tels quels"
)]
pub struct DensityState {
    /// Densité visuelle courante, 0..1.
    pub delta: f32,
    /// État dans lequel elle tombe.
    pub band: DensityBand,
    /// Creux d'avant-drop en cours.
    pub predrop: bool,
    /// Mesure d'épure imposée en cours.
    pub epure: bool,
}

impl DensityState {
    /// État d'ouverture.
    pub const NEUTRAL: Self = Self {
        delta: INITIAL,
        band: DensityBand::Groove,
        predrop: false,
        epure: false,
    };
}

impl Default for DensityState {
    fn default() -> Self {
        Self::NEUTRAL
    }
}

/// Entrées d'une image.
#[derive(Debug, Clone, Copy)]
pub struct DensityInput {
    /// Durée de l'image (s).
    pub dt: f32,
    /// Horloge du show (s), pour le limiteur.
    pub now: f64,
    /// Tempo (BPM) ; `None` : 120.
    pub bpm: Option<f32>,
    /// Section de la musique.
    pub section: Section,
    /// Intensité lissée, 0..1.
    pub intensity: f32,
    /// Compte de kicks de l'analyse (la phrase en est dérivée).
    pub count: u32,
    /// Temps restants avant la frappe attendue du drop, si un drop est armé et que la frappe est proche ; `None` sinon.
    pub strike_in_beats: Option<f32>,
    /// Le drop frappe sur cette image.
    pub drop_now: bool,
    /// Flashs permis.
    pub flashes: bool,
    /// Mode respiration.
    pub breathing: bool,
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "quatre bascules d'état indépendantes du creux et de l'épure"
)]
/// Calcule `Δ`. Aucune allocation : l'état tient dans des tableaux de taille fixe.
#[derive(Debug, Clone)]
pub struct DensityConductor {
    tuning: DensityTuning,
    state: DensityState,
    /// Densité voulue, avant plafond de fatigue ou de respiration (celle que lisse la cible).
    raw: f32,
    /// Temps écoulés au-dessus du seuil de fatigue.
    high_beats: f32,
    /// Temps écoulés sans passage sous le seuil de respiration.
    steady_beats: f32,
    epure_pending: bool,
    epure_left: f32,
    strike_left: f32,
    predrop_active: bool,
    predrop_denied: bool,
    /// Le creux de ce drop armé a déjà eu lieu : un seul par frappe.
    predrop_used: bool,
    phrase: Option<u32>,
    /// Temps écoulés depuis le début de la mesure en cours, somme de `Δ` sur la mesure, moyennes des 8 dernières.
    bar_beats: f32,
    bar_sum: f32,
    bars: [f32; 8],
    bar_count: usize,
    limiter: FlashLimiter,
}

/// Tempo supposé sans analyse.
const FALLBACK_BPM: f32 = 120.0;

impl Default for DensityConductor {
    fn default() -> Self {
        Self::new(DensityTuning::default())
    }
}

impl DensityConductor {
    /// Conducteur neuf.
    #[must_use]
    pub fn new(tuning: DensityTuning) -> Self {
        Self {
            tuning,
            state: DensityState::NEUTRAL,
            raw: INITIAL,
            high_beats: 0.0,
            steady_beats: 0.0,
            epure_pending: false,
            epure_left: 0.0,
            strike_left: 0.0,
            predrop_active: false,
            predrop_denied: false,
            predrop_used: false,
            phrase: None,
            bar_beats: 0.0,
            bar_sum: 0.0,
            bars: [INITIAL; 8],
            bar_count: 0,
            limiter: FlashLimiter::new(),
        }
    }

    /// Dernier état publié.
    #[must_use]
    pub const fn state(&self) -> &DensityState {
        &self.state
    }

    /// Réglages en vigueur.
    #[must_use]
    pub const fn tuning(&self) -> &DensityTuning {
        &self.tuning
    }

    /// Nouveau set : tout repart de l'état d'ouverture.
    pub fn reset(&mut self) {
        *self = Self::new(self.tuning);
    }

    /// Cible brute `table[section]·(0.7 + 0.3·intensity)`.
    #[must_use]
    pub fn target(&self, section: Section, intensity: f32) -> f32 {
        let row = match section {
            Section::Calm => self.tuning.table[0],
            Section::Groove => self.tuning.table[1],
            Section::Peak => self.tuning.table[2],
        };
        let intensity = if intensity.is_finite() {
            intensity.clamp(0.0, 1.0)
        } else {
            0.0
        };
        row * (0.7 + 0.3 * intensity)
    }

    /// Moyenne de `Δ` sur les (jusqu'à) 8 dernières mesures complètes.
    #[must_use]
    pub fn recent_mean(&self) -> f32 {
        let n = self.bar_count.min(8);
        if n == 0 {
            return self.state.delta;
        }
        self.bars[..n].iter().sum::<f32>() / n as f32
    }

    /// Longueur du creux d'avant-drop (en temps) : un temps, étendue jusqu'à une mesure si le climax prévu ne vaut
    /// pas deux fois la moyenne des 8 mesures précédentes (règle 1 de DA §4.2).
    #[must_use]
    pub fn predrop_window(&self, intensity: f32) -> f32 {
        let peak = self.target(Section::Peak, intensity);
        let ratio = peak / self.recent_mean().max(0.05);
        let lack = saturate((self.tuning.contrast - ratio) / self.tuning.contrast);
        self.tuning.predrop_beats + (self.tuning.predrop_max_beats - self.tuning.predrop_beats) * lack
    }

    /// Avance d'une image.
    pub fn update(&mut self, input: &DensityInput) -> &DensityState {
        let dt = if input.dt.is_finite() {
            input.dt.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let bpm = input.bpm.filter(|b| b.is_finite() && *b > 20.0).unwrap_or(FALLBACK_BPM);
        let beat_s = 60.0 / bpm;
        let beats = dt / beat_s;
        let t = self.tuning;

        self.limiter.set_policy(input.flashes, input.breathing);
        self.limiter.step(dt);

        // Respiration imposée : au prochain début de phrase après 64 temps sans passage sous 0,4, une mesure d'épure.
        let phrase = input.count / 16;
        let phrase_start = self.phrase.is_some_and(|p| p != phrase);
        self.phrase = Some(phrase);
        if self.state.delta < BREATH_THRESHOLD {
            self.steady_beats = 0.0;
        } else {
            self.steady_beats += beats;
            if self.steady_beats > t.breath_after_beats {
                self.epure_pending = true;
            }
        }
        if self.epure_pending && phrase_start && !input.drop_now {
            self.epure_pending = false;
            self.epure_left = t.epure_beats;
            self.steady_beats = 0.0;
        }
        let epure = self.epure_left > 0.0;
        self.epure_left = (self.epure_left - beats).max(0.0);

        // Creux d'avant-drop.
        let window = self.predrop_window(input.intensity);
        let wants_predrop = input.strike_in_beats.is_some_and(|b| b <= window) && !input.drop_now;
        if wants_predrop && !self.predrop_active && !self.predrop_denied && !self.predrop_used {
            let request = FlashRequest {
                channel: Channel::Sweep,
                peak: MAX_SWEEP_PEAK,
                attack_s: MIN_FADE,
                decay_s: MIN_FADE,
            };
            if self.limiter.trigger(request, input.now) {
                self.predrop_active = true;
            } else {
                self.predrop_denied = true;
            }
        }
        if !wants_predrop || !input.flashes || input.breathing {
            self.predrop_used |= self.predrop_active;
            self.predrop_active = false;
            if !wants_predrop {
                self.predrop_denied = false;
            }
        }
        if input.drop_now {
            self.predrop_used = false;
            self.strike_left = t.strike_beats;
        }

        // Cible puis lissage en temps musical.
        let mut target = self.target(input.section, input.intensity);
        if epure {
            target = target.min(EPURE_LEVEL);
        }
        if self.predrop_active {
            target = PREDROP_LEVEL;
        }
        let rising = target > self.raw;
        let tau_beats = if !rising {
            t.fall_beats
        } else if self.strike_left > 0.0 {
            t.strike_rise_beats
        } else {
            t.rise_beats
        };
        self.strike_left = (self.strike_left - beats).max(0.0);
        let mut delta = self.raw;
        // Le creux d'avant-drop est une rampe courte (la descente lente de 8 temps le rendrait invisible).
        let ramp = if self.predrop_active || epure {
            1.0 - (-beats / 0.25_f32).exp()
        } else {
            1.0 - (-beats / tau_beats).exp()
        };
        delta += (target - delta) * ramp;
        self.raw = delta;

        // Fatigue : plafond qui descend après 32 temps au-dessus de 0,8. Le compte porte sur la densité *voulue*
        // (avant plafond) : sinon le plafond ferait retomber Δ sous 0,8, remettrait le compte à zéro et se lèverait.
        if delta > FATIGUE_THRESHOLD {
            self.high_beats += beats;
        } else {
            self.high_beats = 0.0;
        }
        let tired = saturate((self.high_beats - t.fatigue_after_beats) / t.fatigue_ramp_beats);
        let mut ceiling = 1.0 - t.fatigue_loss * tired;
        if input.breathing {
            ceiling = ceiling.min(t.breathing_cap);
        }
        delta = delta.min(ceiling);

        // Borne de pente : jamais plus vite que 1/MIN_FADE par seconde.
        let step = dt / MIN_FADE;
        delta = (self.state.delta + (delta - self.state.delta).clamp(-step, step)).clamp(0.0, 1.0);

        // Moyenne par mesure (4 temps).
        self.bar_sum += delta * beats;
        self.bar_beats += beats;
        if self.bar_beats >= 4.0 {
            let mean = self.bar_sum / self.bar_beats;
            self.bars.rotate_right(1);
            self.bars[0] = mean;
            self.bar_count = (self.bar_count + 1).min(8);
            self.bar_sum = 0.0;
            self.bar_beats = 0.0;
        }

        self.state = DensityState {
            delta,
            band: DensityBand::of(delta, self.state.band),
            predrop: self.predrop_active,
            epure,
        };
        &self.state
    }
}

/// Borne dans 0..1 (un nombre non fini vaut 0).
fn saturate(x: f32) -> f32 {
    if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.0 }
}
