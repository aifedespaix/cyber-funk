//! Règles du gouverneur : gardes, fenêtre de changement, choix du prochain quartier.

use super::{Decision, DistrictId, DistrictKind, GovernorSnapshot, StayReason, TransitionKind};
use cf_analysis::Section;

/// Temps minimum passé dans un quartier, en secondes.
pub const MIN_DWELL_S: f64 = 150.0;
/// Temps maximum : passé ce délai, le premier moment autorisé déclenche un changement.
pub const MAX_DWELL_S: f64 = 480.0;
/// Durée maximale d'une transition, en secondes.
pub const TRANSITION_MAX_S: f64 = 10.0;
/// Nombre de mesures de 4 temps interdites après un drop.
pub const AFTER_DROP_BEATS: u32 = 8;
/// Durée interdite après un drop quand le tempo est inconnu, en secondes.
pub const AFTER_DROP_FALLBACK_S: f64 = 16.0;

/// Taille de l'historique exposé.
const HISTORY_LEN: usize = 5;
/// Part du tirage guidée par le penchant sonore (le reste est uniforme).
const BIAS_SHARE: f64 = 0.75;
/// Facteur appliqué à la probabilité du type précédent (évite le va-et-vient sans l'interdire).
const DAMPING: f64 = 0.35;
/// Poids plancher d'un type, pour qu'aucun ne soit jamais exclu par le penchant.
const WEIGHT_FLOOR: f64 = 0.1;

/// Générateur xorshift64* local (pas de dépendance de plus, résultat identique partout).
#[derive(Debug, Clone)]
struct XorShift(u64);

impl XorShift {
    fn new(seed: u64) -> Self {
        // Passage par splitmix64 : une graine nulle ou voisine donne quand même un état sain.
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        Self(if z == 0 { 0x2545_F491_4F6C_DD1D } else { z })
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Flottant dans [0, 1).
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Entier dans [0, n), `n` > 0.
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
}

/// Le gouverneur de quartiers.
#[derive(Debug, Clone)]
pub struct Governor {
    rng: XorShift,
    variants: [u8; 3],
    current: DistrictId,
    history: Vec<DistrictId>,
    /// Dernière variante utilisée par type, pour ne jamais la répéter.
    last_variant: [Option<u8>; 3],
    frozen: bool,
    /// Début du quartier courant ; fixé au premier instantané.
    last_change_s: Option<f64>,
    /// Début de la transition en cours.
    transition_since_s: Option<f64>,
    last_drop_s: Option<f64>,
    /// Dernier temps vu, pour détecter une horloge audio qui recule.
    last_seen_s: Option<f64>,
    /// Transition du dernier changement (dégradé par défaut).
    transition: TransitionKind,
}

impl Governor {
    /// Nouveau gouverneur dans le centre néon, variante 0. `variants_per_kind` suit l'ordre de
    /// [`DistrictKind::ALL`] ; un type à 0 variante compte pour 1.
    #[must_use]
    pub fn new(seed: u64, variants_per_kind: [u8; 3]) -> Self {
        let current = DistrictId {
            kind: DistrictKind::Neon,
            variant: 0,
        };
        let mut last_variant = [None; 3];
        last_variant[current.kind.index()] = Some(current.variant);
        Self {
            rng: XorShift::new(seed),
            variants: variants_per_kind.map(|n| n.max(1)),
            current,
            history: vec![current],
            last_variant,
            frozen: false,
            last_change_s: None,
            transition_since_s: None,
            last_drop_s: None,
            last_seen_s: None,
            transition: TransitionKind::Gradient,
        }
    }

    /// Quartier courant.
    #[must_use]
    pub fn current(&self) -> DistrictId {
        self.current
    }

    /// Les 5 derniers quartiers, le courant en dernier.
    #[must_use]
    pub fn history(&self) -> &[DistrictId] {
        &self.history
    }

    /// Gèle ou dégèle l'automatique (n'empêche pas [`Self::force`]).
    pub fn set_frozen(&mut self, frozen: bool) {
        self.frozen = frozen;
    }

    /// Transition du dernier changement, pour que la colle sache quoi jouer.
    #[must_use]
    pub fn current_transition(&self) -> TransitionKind {
        self.transition
    }

    /// Forçage de debug : contourne les règles et remplace une transition en cours.
    pub fn force(&mut self, to: DistrictId, transition: TransitionKind, now_s: f64) {
        self.enter(to, transition, now_s);
    }

    /// La colle signale que le dégradé est fini ; sinon il se termine seul à [`TRANSITION_MAX_S`].
    pub fn transition_done(&mut self, _now_s: f64) {
        self.transition_since_s = None;
    }

    /// Décision pour cet instantané.
    pub fn tick(&mut self, snap: &GovernorSnapshot) -> Decision {
        let now = snap.now_s;
        // Horloge audio revenue en arrière (changement de source, morceau relancé) : on repart de maintenant.
        if self.last_seen_s.is_some_and(|seen| now < seen) {
            self.last_change_s = self.last_change_s.map(|_| now);
            self.transition_since_s = self.transition_since_s.map(|_| now);
            self.last_drop_s = self.last_drop_s.map(|_| now);
        }
        self.last_seen_s = Some(now);
        let since = *self.last_change_s.get_or_insert(now);
        if snap.drop_now {
            self.last_drop_s = Some(now);
        }
        if self.transition_since_s.is_some_and(|t| now - t >= TRANSITION_MAX_S) {
            self.transition_since_s = None;
        }

        let stay = |reason| Decision::Stay { reason };
        if !snap.show_present {
            return stay(StayReason::NoShow);
        }
        if self.frozen {
            return stay(StayReason::Frozen);
        }
        if self.transition_since_s.is_some() {
            return stay(StayReason::Transitioning);
        }
        if snap.section == Section::Peak {
            return stay(StayReason::PeakSection);
        }
        if snap.drop_imminent {
            return stay(StayReason::DropImminent);
        }
        if self
            .last_drop_s
            .is_some_and(|d| now - d < after_drop_window_s(snap.bpm))
        {
            return stay(StayReason::AfterDrop);
        }
        let dwell = now - since;
        if dwell < MIN_DWELL_S {
            return stay(StayReason::TooSoon);
        }
        let open = snap.section == Section::Calm || snap.section_changed || snap.event_ended || dwell >= MAX_DWELL_S;
        if !open {
            return stay(StayReason::NoWindow);
        }

        let to = self.pick(snap);
        self.enter(to, TransitionKind::Gradient, now);
        Decision::Change {
            to,
            transition: TransitionKind::Gradient,
        }
    }

    /// Installe un nouveau quartier courant et démarre sa transition.
    fn enter(&mut self, to: DistrictId, transition: TransitionKind, now_s: f64) {
        self.current = to;
        self.transition = transition;
        self.history.push(to);
        if self.history.len() > HISTORY_LEN {
            self.history.remove(0);
        }
        self.last_variant[to.kind.index()] = Some(to.variant);
        self.last_change_s = Some(now_s);
        self.transition_since_s = Some(now_s);
    }

    /// Choisit le prochain quartier (type puis variante). Seul le type courant est exclu : avec trois
    /// types, exclure aussi le précédent figerait l'ordre et neutraliserait le penchant sonore ; le
    /// précédent est donc seulement pondéré à la baisse.
    fn pick(&mut self, snap: &GovernorSnapshot) -> DistrictId {
        let previous = self.history.len().checked_sub(2).map(|i| self.history[i].kind);
        let kind = self
            .pick_kind(snap, &[self.current.kind], previous)
            .or_else(|| self.pick_kind(snap, &[], None))
            .unwrap_or(self.current.kind);
        let variant = self.pick_variant(kind);
        DistrictId { kind, variant }
    }

    /// Tire un type hors de `excluded`, selon le penchant sonore ; `damped` voit sa probabilité réduite
    /// ([`DAMPING`]). `None` si tout est exclu.
    pub(super) fn pick_kind(
        &mut self,
        snap: &GovernorSnapshot,
        excluded: &[DistrictKind],
        damped: Option<DistrictKind>,
    ) -> Option<DistrictKind> {
        let allowed: Vec<DistrictKind> = DistrictKind::ALL
            .into_iter()
            .filter(|k| !excluded.contains(k))
            .collect();
        if allowed.is_empty() {
            return None;
        }
        let total: f64 = allowed.iter().map(|&k| kind_score(k, snap)).sum();
        let uniform = 1.0 / allowed.len() as f64;
        let probs: Vec<f64> = allowed
            .iter()
            .map(|&k| {
                let p = BIAS_SHARE * kind_score(k, snap) / total + (1.0 - BIAS_SHARE) * uniform;
                if Some(k) == damped { p * DAMPING } else { p }
            })
            .collect();
        let mut r = self.rng.next_f64() * probs.iter().sum::<f64>();
        for (&kind, p) in allowed.iter().zip(&probs) {
            if r < *p {
                return Some(kind);
            }
            r -= p;
        }
        allowed.last().copied()
    }

    /// Variante du type, différente de la dernière utilisée par ce type quand il en a plusieurs.
    fn pick_variant(&mut self, kind: DistrictKind) -> u8 {
        let count = self.variants[kind.index()];
        let last = self.last_variant[kind.index()];
        let choices: Vec<u8> = (0..count).filter(|v| Some(*v) != last).collect();
        if choices.is_empty() {
            return last.unwrap_or(0);
        }
        choices[self.rng.below(choices.len())]
    }
}

/// Durée interdite après un drop : 8 mesures de 4 temps si le tempo est connu.
fn after_drop_window_s(bpm: Option<f32>) -> f64 {
    match bpm {
        Some(b) if b > 1.0 => f64::from(AFTER_DROP_BEATS) * 4.0 * 60.0 / f64::from(b),
        _ => AFTER_DROP_FALLBACK_S,
    }
}

/// Penchant sonore : intensité haute vers le néon, basses lourdes et peu d'aigus vers la banlieue,
/// calme vers le résidentiel.
fn kind_score(kind: DistrictKind, snap: &GovernorSnapshot) -> f64 {
    let clamp = |v: f32| f64::from(v.clamp(0.0, 1.0));
    let (i, b, h) = (clamp(snap.intensity), clamp(snap.bass), clamp(snap.high));
    let raw = match kind {
        DistrictKind::Neon => i,
        DistrictKind::Suburb => b * (1.0 - h),
        DistrictKind::Residential => 1.0 - i,
    };
    raw.max(WEIGHT_FLOOR)
}
