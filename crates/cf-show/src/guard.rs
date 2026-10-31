//! Isolation des pannes : un système qui plante ne doit pas arrêter le show (skill `live-safety`).
//!
//! [`Guard`] exécute un bloc sous `catch_unwind`. Après `limit` pannes consécutives, il se désactive : le bloc
//! n'est plus appelé et l'appelant (journal, bandeau opérateur) est prévenu une seule fois. Le show continue sans
//! ce système. Fonctionne tant que le profil compile avec `panic = "unwind"`.

use std::panic::{AssertUnwindSafe, catch_unwind};

/// Ce qui s'est passé à un appel de [`Guard::run`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardOutcome {
    /// Le bloc s'est exécuté sans panique.
    Ran,
    /// Le bloc a paniqué ; il sera rappelé à l'image suivante.
    Panicked {
        /// Pannes consécutives, celle-ci comprise.
        streak: u32,
    },
    /// Le bloc vient de paniquer pour la dernière fois : désactivé jusqu'à [`Guard::reset`]. Renvoyé une fois.
    Disabled,
    /// Le bloc est désactivé, il n'a pas été appelé.
    Skipped,
}

/// Garde d'un système : compte les pannes consécutives et le désactive au-delà de la limite.
#[derive(Debug, Clone)]
pub struct Guard {
    limit: u32,
    streak: u32,
    total: u32,
    disabled: bool,
}

impl Guard {
    /// Garde qui tolère `limit` pannes consécutives (au moins 1).
    #[must_use]
    pub fn new(limit: u32) -> Self {
        Self {
            limit: limit.max(1),
            streak: 0,
            total: 0,
            disabled: false,
        }
    }

    /// Exécute `f` si le système n'est pas désactivé, en attrapant une panique éventuelle.
    pub fn run(&mut self, f: impl FnOnce()) -> GuardOutcome {
        if self.disabled {
            return GuardOutcome::Skipped;
        }
        if catch_unwind(AssertUnwindSafe(f)).is_ok() {
            self.streak = 0;
            return GuardOutcome::Ran;
        }
        self.streak = self.streak.saturating_add(1);
        self.total = self.total.saturating_add(1);
        if self.streak >= self.limit {
            self.disabled = true;
            return GuardOutcome::Disabled;
        }
        GuardOutcome::Panicked { streak: self.streak }
    }

    /// Vrai si le système est désactivé.
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// Pannes depuis le début (compteur borné, pour le panneau de diagnostic).
    #[must_use]
    pub fn total_panics(&self) -> u32 {
        self.total
    }

    /// Réactive le système (action explicite de l'opérateur).
    pub fn reset(&mut self) {
        self.streak = 0;
        self.disabled = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::panic, reason = "panique voulue : c'est ce que la garde doit absorber")]
    fn boom() {
        panic!("panne injectée");
    }

    #[test]
    fn a_panic_is_absorbed_then_the_system_is_disabled() {
        let mut guard = Guard::new(3);
        assert_eq!(guard.run(boom), GuardOutcome::Panicked { streak: 1 });
        assert_eq!(
            guard.run(|| {}),
            GuardOutcome::Ran,
            "une réussite remet la série à zéro"
        );
        assert_eq!(guard.run(boom), GuardOutcome::Panicked { streak: 1 });
        assert_eq!(guard.run(boom), GuardOutcome::Panicked { streak: 2 });
        assert_eq!(guard.run(boom), GuardOutcome::Disabled);
        assert!(guard.is_disabled());
        let mut called = false;
        assert_eq!(guard.run(|| called = true), GuardOutcome::Skipped);
        assert!(!called);
        guard.reset();
        assert_eq!(guard.run(|| called = true), GuardOutcome::Ran);
        assert!(called);
        assert_eq!(guard.total_panics(), 4);
    }
}
