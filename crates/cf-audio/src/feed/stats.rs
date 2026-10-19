//! Compteurs partagés d'un flux et signalement des pannes du backend.
//!
//! Tout est atomique : le callback temps réel écrit, les autres threads lisent, personne n'attend personne.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

/// Panne signalée par le backend d'un flux (callback d'erreur de `cpal`, ou source synthétique).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FeedFault {
    /// Le périphérique a disparu (interface USB débranchée).
    DeviceLost = 1,
    /// Le flux doit être reconstruit (changement de configuration côté système).
    Invalidated = 2,
    /// Autre erreur du backend, considérée comme fatale pour ce flux.
    Backend = 3,
}

impl FeedFault {
    const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::DeviceLost),
            2 => Some(Self::Invalidated),
            3 => Some(Self::Backend),
            _ => None,
        }
    }
}

impl fmt::Display for FeedFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::DeviceLost => "périphérique perdu",
            Self::Invalidated => "flux invalidé",
            Self::Backend => "erreur du backend audio",
        })
    }
}

/// Compteurs partagés d'un flux, écrits par le callback (atomiques, `Relaxed`) et lus par le superviseur de capture,
/// le thread d'analyse et l'interface opérateur.
#[derive(Debug, Default)]
pub struct FeedStats {
    pub(super) written: AtomicU64,
    pub(super) dropped: AtomicU64,
    pub(super) callbacks: AtomicU64,
    xruns: AtomicU64,
    fault: AtomicU8,
}

impl FeedStats {
    /// Échantillons mono écrits dans le ring depuis l'ouverture du flux.
    #[must_use]
    pub fn written(&self) -> u64 {
        self.written.load(Ordering::Relaxed)
    }

    /// Échantillons jetés parce que le ring était plein (le thread d'analyse a pris du retard).
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Nombre d'appels du callback. Le superviseur s'en sert pour détecter un flux figé.
    #[must_use]
    pub fn callbacks(&self) -> u64 {
        self.callbacks.load(Ordering::Relaxed)
    }

    /// Débordements signalés par le backend (`Xrun` : le pilote a perdu des trames avant notre callback).
    #[must_use]
    pub fn xruns(&self) -> u64 {
        self.xruns.load(Ordering::Relaxed)
    }

    /// Première panne signalée, s'il y en a une. Un flux en panne ne livrera plus rien : il faut le rouvrir.
    #[must_use]
    pub fn fault(&self) -> Option<FeedFault> {
        FeedFault::from_code(self.fault.load(Ordering::Acquire))
    }
}

/// Signale les pannes d'un flux depuis le callback d'erreur du backend. Clonable, sans allocation à l'usage.
#[derive(Debug, Clone)]
pub struct FaultReporter {
    pub(super) stats: Arc<FeedStats>,
}

impl FaultReporter {
    /// Enregistre une panne. Seule la première est gardée : c'est elle qui explique les suivantes.
    pub fn report(&self, fault: FeedFault) {
        // Échec = une panne est déjà enregistrée ; on la garde volontairement.
        let _ = self
            .stats
            .fault
            .compare_exchange(0, fault as u8, Ordering::AcqRel, Ordering::Acquire);
    }

    /// Compte un débordement du pilote (non fatal).
    pub fn report_xrun(&self) {
        self.stats.xruns.fetch_add(1, Ordering::Relaxed);
    }
}
