//! Les portes des cascades (`stunts/permit.ts`) : rangs et liste blanche du look, porte et délai de l'Ascension, ordre
//! de l'opérateur.

use super::kind::StuntKind;
use super::script::{StuntScript, StuntShow};
use crate::pilot::rand::Rand;

/// Entre deux Ascensions (s) : une toutes les 30 à 40 minutes au plus.
const COOLDOWN: (f64, f64) = (1800.0, 2400.0);
/// Un ordre de l'opérateur attend jusqu'à ce temps (s) un temps fort où la route est libre.
const LATCH: f64 = 40.0;

/// Ce que les cascades demandent au chef du spectacle : l'ordre « Ascension » de l'opérateur et la mémoire
/// d'anti-répétition (`freshness`). Sans spectacle : [`NoLink`].
pub trait StuntLink {
    /// Vrai une seule fois après un ordre « Ascension » encore vivant (`take("ascension")`).
    fn take_ascension(&mut self) -> bool;
    /// Coefficient à multiplier au poids de tirage de `id` (`stunt:<slug>`), 1 pour un élément frais.
    fn freshness_weight(&self, id: &str, now: f64) -> f64;
    /// Marque `id` comme utilisé à l'instant `now`.
    fn freshness_mark(&mut self, id: &str, now: f64);
    /// Ralentit le monde à `scale` (0,1 → 1) pendant `seconds` (`show.requestTimeScale`). Sans effet par défaut.
    fn request_time_scale(&mut self, _scale: f32, _seconds: f32) {}
}

/// Pas de spectacle : aucun ordre, aucune mémoire.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoLink;

impl StuntLink for NoLink {
    fn take_ascension(&mut self) -> bool {
        false
    }

    fn freshness_weight(&self, _id: &str, _now: f64) -> f64 {
        1.0
    }

    fn freshness_mark(&mut self, _id: &str, _now: f64) {}
}

/// Le niveau en cours permet-il des cascades ? (Rien aux niveaux 0 et 1 : leurs listes sont vides.) Et jamais quand le
/// look interdit de quitter le sol (`max_floor` à 0) : la voiture ne vole jamais au niveau 0.
#[must_use]
pub fn level_open(show: &StuntShow<'_>) -> bool {
    let gates = show.gates;
    gates.max_floor > 0 && !gates.stunt_tiers.is_empty() && !gates.stunt_kinds.is_empty()
}

/// Cette cascade passe-t-elle les portes ? Sans spectacle, tout passe sauf l'Ascension. L'Ascension a sa propre porte.
#[must_use]
pub fn permitted(script: &dyn StuntScript, show: Option<&StuntShow<'_>>, ascension_ready: bool) -> bool {
    let tier_ok = |show: &StuntShow<'_>| script.tier().allowed_by(&show.gates.stunt_tiers);
    if script.kind() == StuntKind::Ascension {
        return show.is_some_and(|s| s.gates.ascension && ascension_ready && tier_ok(s));
    }
    let Some(show) = show else { return true };
    show.gates
        .stunt_kinds
        .iter()
        .any(|k| k.as_str() == script.kind().as_str())
        && tier_ok(show)
}

/// Délai de l'Ascension et ordre de l'opérateur (qui ignore la porte `ascension`, mais jamais `available`).
#[derive(Debug, Clone, Copy)]
pub struct AscensionGate {
    next_at: f64,
    queued_until: f64,
}

impl Default for AscensionGate {
    fn default() -> Self {
        Self {
            next_at: f64::NEG_INFINITY,
            queued_until: f64::NEG_INFINITY,
        }
    }
}

impl AscensionGate {
    /// Relève l'ordre de l'opérateur (il expire vite côté spectacle : on le garde ici).
    pub fn poll(&mut self, link: &mut dyn StuntLink, now: f64) {
        if link.take_ascension() {
            self.queued_until = now + LATCH;
        }
    }

    /// Un ordre de l'opérateur attend un temps fort.
    #[must_use]
    pub fn queued(&self, now: f64) -> bool {
        now < self.queued_until
    }

    /// Le délai entre deux Ascensions est écoulé.
    #[must_use]
    pub fn ready(&self, now: f64) -> bool {
        now >= self.next_at
    }

    /// Une Ascension part : le délai repart, l'ordre est honoré.
    pub fn started(&mut self, now: f64, rng: &mut Rand) {
        self.next_at = now + rng.range(COOLDOWN.0, COOLDOWN.1);
        self.queued_until = f64::NEG_INFINITY;
    }
}
