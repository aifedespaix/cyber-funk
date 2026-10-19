//! Anti-répétition sur la longue durée (portage de `show/freshness.ts`).

use std::collections::HashMap;

/// Après ce délai (s) sans usage, un élément a retrouvé tout son poids.
const RECOVERY_SECONDS: f64 = 600.0;
/// Plancher du poids juste après un usage : il n'est jamais exclu, seulement repoussé.
const JUST_USED: f64 = 0.2;
/// Bonus d'un élément jamais (ou rarement) utilisé.
const NEW_BONUS: f64 = 0.4;

#[derive(Debug, Clone, Copy)]
struct Usage {
    count: u32,
    /// Instant (s) du dernier usage.
    last: f64,
}

impl Usage {
    const UNUSED: Self = Self {
        count: 0,
        last: f64::NEG_INFINITY,
    };
}

/// Anti-répétition sur la longue durée, partagée par la caméra, les cascades, les événements et les
/// immeubles-vitrines (identifiants préfixés `shot:`, `stunt:`, `event:`, `landmark:`).
///
/// `weight` donne un coefficient à multiplier au poids de tirage : il remonte avec le temps écoulé depuis le dernier
/// usage, et baisse avec le nombre d'usages comparé à la moyenne (un élément trop joué cède la place aux autres).
/// Aucune allocation par appel (hors première rencontre d'un id). Le temps est injecté (`now`, en secondes).
#[derive(Debug, Clone)]
pub struct Freshness {
    records: HashMap<String, Usage>,
    total: u64,
    recovery_s: f64,
}

impl Default for Freshness {
    fn default() -> Self {
        Self::new(&[] as &[&str])
    }
}

impl Freshness {
    /// Crée la mémoire avec des ids déjà connus (zéro usage) et la durée de récupération par défaut.
    pub fn new<S: AsRef<str>>(ids: &[S]) -> Self {
        Self::with_recovery(ids, RECOVERY_SECONDS)
    }

    /// Comme [`Freshness::new`], avec une durée de récupération `recovery_s` (s) explicite.
    pub fn with_recovery<S: AsRef<str>>(ids: &[S], recovery_s: f64) -> Self {
        let mut freshness = Self {
            records: HashMap::new(),
            total: 0,
            recovery_s,
        };
        freshness.known(ids);
        freshness
    }

    /// Déclare des ids à zéro usage : la moyenne se calcule alors sur tout le catalogue, pas seulement sur ce qui a
    /// servi.
    pub fn known<S: AsRef<str>>(&mut self, ids: &[S]) {
        for id in ids {
            if !self.records.contains_key(id.as_ref()) {
                self.records.insert(id.as_ref().to_owned(), Usage::UNUSED);
            }
        }
    }

    /// Coefficient > 0 à multiplier au poids de l'élément ; ~1,4 s'il est neuf, ~0,2 juste après usage.
    #[must_use]
    pub fn weight(&self, id: &str, now: f64) -> f64 {
        let Some(record) = self.records.get(id).filter(|r| r.count > 0) else {
            return 1.0 + NEW_BONUS;
        };
        let age = (now - record.last).max(0.0);
        let rested = JUST_USED + (1.0 - JUST_USED) * (age / self.recovery_s).min(1.0);
        let average = self.total as f64 / self.records.len() as f64;
        let relative = if average > 0.0 {
            f64::from(record.count) / average
        } else {
            1.0
        };
        let share = if relative <= 1.0 {
            1.0 + (1.0 - relative) * NEW_BONUS
        } else {
            1.0 / relative
        };
        rested * share
    }

    /// Note un usage à l'instant `now` (s).
    pub fn mark(&mut self, id: &str, now: f64) {
        if !self.records.contains_key(id) {
            self.records.insert(id.to_owned(), Usage::UNUSED);
        }
        if let Some(record) = self.records.get_mut(id) {
            record.count += 1;
            record.last = now;
        }
        self.total += 1;
    }

    /// Efface la mémoire (nouveau set).
    pub fn reset(&mut self) {
        self.records.clear();
        self.total = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn unknown_or_unused_is_boosted() {
        let f = Freshness::new(&["shot:a"]);
        assert!(near(f.weight("shot:a", 0.0), 1.4));
        assert!(near(f.weight("shot:inconnu", 0.0), 1.4));
    }

    #[test]
    fn weight_drops_right_after_mark_then_recovers() {
        let mut f = Freshness::new(&["shot:a", "shot:b"]);
        f.mark("shot:a", 10.0);
        let just_after = f.weight("shot:a", 10.0);
        let later = f.weight("shot:a", 10.0 + 300.0);
        let rested = f.weight("shot:a", 10.0 + 10_000.0);
        assert!(just_after < later && later < rested, "{just_after} {later} {rested}");
        // Juste après : plancher 0,2 ; a seul a servi donc relative = 2 -> share 0,5.
        assert!(near(just_after, 0.2 * 0.5));
        assert!(just_after < f.weight("shot:b", 10.0));
        // Totalement reposé mais toujours sur-utilisé : 1 / relative.
        assert!(near(rested, 0.5));
    }

    #[test]
    fn overused_yields_to_others() {
        let mut f = Freshness::new(&["a", "b"]);
        for t in 0..3 {
            f.mark("a", f64::from(t));
        }
        f.mark("b", 2.0);
        assert!(f.weight("a", 1000.0) < f.weight("b", 1000.0));
    }

    #[test]
    fn mark_unknown_registers_it_and_reset_clears() {
        let mut f = Freshness::default();
        f.mark("event:x", 0.0);
        assert!(f.weight("event:x", 0.0) < 1.0);
        f.reset();
        assert!(near(f.weight("event:x", 0.0), 1.4));
    }

    #[test]
    fn age_never_negative() {
        let mut f = Freshness::new(&["a"]);
        f.mark("a", 100.0);
        assert!(near(f.weight("a", 50.0), f.weight("a", 100.0)));
    }
}
