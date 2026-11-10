//! La matrice de routage : lecture en RON, validation, matrice par défaut (P2 §2.3).

use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::types::{Curve, Detector, Route, Smooth};
use crate::audio::routing::Target;
use cf_analysis::SPECTRUM_BANDS;

/// Matrice par défaut, qui reproduit P2 §2.2 (constantes de temps) et §2.3 (cartographie d'impact).
const DEFAULT_RON: &str = include_str!("default_routing.ron");

/// Plafond du nombre de routes : de quoi garder la matrice lisible et son coût borné.
pub const MAX_ROUTES: usize = 64;

/// Pourquoi une matrice est refusée.
#[derive(Debug, Error)]
pub enum RoutingError {
    /// Fichier illisible.
    #[error("matrice de routage illisible : {0}")]
    Io(#[from] std::io::Error),
    /// RON invalide.
    #[error("matrice de routage invalide : {0}")]
    Parse(#[from] ron::error::SpannedError),
    /// Trop de routes.
    #[error("{0} routes, le maximum est {MAX_ROUTES}")]
    TooMany(usize),
    /// Une valeur hors domaine.
    #[error("route {index} ({target:?}) : {reason}")]
    Invalid {
        /// Rang de la route.
        index: usize,
        /// Cible de la route.
        target: Target,
        /// Ce qui ne va pas.
        reason: &'static str,
    },
}

/// L'ensemble des routes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingMatrix {
    /// Les routes, appliquées dans l'ordre ; deux routes sur la même cible se combinent par le maximum.
    pub routes: Vec<Route>,
}

impl RoutingMatrix {
    /// La matrice par défaut.
    ///
    /// # Panics
    ///
    /// Jamais pour un binaire sain : le texte est celui du dépôt et son test le valide.
    #[must_use]
    #[allow(clippy::expect_used, reason = "texte embarqué vérifié par le test `default_matrix_is_valid`")]
    pub fn default_matrix() -> Self {
        Self::from_ron(DEFAULT_RON).expect("matrice de routage par défaut")
    }

    /// Lit et valide une matrice RON.
    ///
    /// # Errors
    ///
    /// [`RoutingError`] si le texte est invalide ou une route hors domaine.
    pub fn from_ron(text: &str) -> Result<Self, RoutingError> {
        let matrix: Self = ron::from_str(text)?;
        matrix.validate()?;
        Ok(matrix)
    }

    /// Lit un fichier.
    ///
    /// # Errors
    ///
    /// [`RoutingError`] comme [`Self::from_ron`], ou une erreur de lecture.
    pub fn load(path: &Path) -> Result<Self, RoutingError> {
        Self::from_ron(&std::fs::read_to_string(path)?)
    }

    /// Vérifie que chaque nombre est fini et dans son domaine.
    ///
    /// # Errors
    ///
    /// [`RoutingError::Invalid`] pour la première route fautive.
    pub fn validate(&self) -> Result<(), RoutingError> {
        if self.routes.len() > MAX_ROUTES {
            return Err(RoutingError::TooMany(self.routes.len()));
        }
        for (index, route) in self.routes.iter().enumerate() {
            if let Err(reason) = validate_route(route) {
                return Err(RoutingError::Invalid { index, target: route.target, reason });
            }
        }
        Ok(())
    }
}

fn time_ok(t: f32) -> bool {
    t.is_finite() && (0.0..=60.0).contains(&t)
}

fn validate_route(r: &Route) -> Result<(), &'static str> {
    match r.detector {
        Detector::Envelope => {}
        Detector::Peak { decay_s, hold_s } => {
            if !time_ok(decay_s) || !time_ok(hold_s) {
                return Err("crête : durées hors 0..60 s");
            }
        }
        Detector::Flux { z } => {
            if !z.is_finite() || !(-8.0..=16.0).contains(&z) {
                return Err("flux : seuil z hors -8..16");
            }
        }
        Detector::Spectrum { bins } => {
            if bins != SPECTRUM_BANDS {
                return Err("spectre : le nombre de bandes doit être 32");
            }
        }
    }
    if let Curve::Pow(g) = r.curve
        && !(g.is_finite() && (0.1..=8.0).contains(&g))
    {
        return Err("courbe : γ hors 0,1..8");
    }
    match r.smooth {
        Smooth::None => {}
        Smooth::AttackRelease { attack_s, release_s } => {
            if !time_ok(attack_s) || !time_ok(release_s) {
                return Err("lissage : durées hors 0..60 s");
            }
        }
        Smooth::AttackReleaseSpring { attack_s, release_s, hz, zeta } => {
            if !time_ok(attack_s) || !time_ok(release_s) || !spring_ok(hz, zeta) {
                return Err("lissage : durées ou ressort hors domaine");
            }
        }
        Smooth::OneEuro { f_min, beta } => {
            if !(f_min.is_finite() && f_min > 0.0 && f_min <= 60.0 && beta.is_finite() && (0.0..=100.0).contains(&beta)) {
                return Err("One-Euro : f_min ou β hors domaine");
            }
        }
        Smooth::Spring { hz, zeta } => {
            if !spring_ok(hz, zeta) {
                return Err("ressort : fréquence ou amortissement hors domaine");
            }
        }
    }
    if !(r.gain.is_finite() && (0.0..=16.0).contains(&r.gain)) {
        return Err("gain hors 0..16");
    }
    if !(r.clamp.0.is_finite() && r.clamp.1.is_finite() && r.clamp.0 <= r.clamp.1) {
        return Err("bornes de sortie inversées ou non finies");
    }
    if !(r.slew.up.is_finite() && r.slew.down.is_finite() && r.slew.up > 0.0 && r.slew.down > 0.0) {
        return Err("borne de pente nulle, négative ou non finie");
    }
    Ok(())
}

fn spring_ok(hz: f32, zeta: f32) -> bool {
    hz.is_finite() && (0.1..=30.0).contains(&hz) && zeta.is_finite() && (0.05..=2.0).contains(&zeta)
}
