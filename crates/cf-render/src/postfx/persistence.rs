//! Persistance rétinienne (DA §3.1) : les émissifs laissent une traînée qui fuit vers le point de fuite.
//!
//! ```text
//! masque = smoothstep(θ_p, 2·θ_p, luma(C))      uv' = pf + (uv − pf)·(1 − ζ)      ζ = 0.004·vitesse/v*
//! H_t    = max(C·masque, H_{t−1}(uv')·exp(−Δt/τ_p))
//! C_out  = C + 0.6·(H_t − C·masque)⁺            τ_p = 0.06 + 0.2·env(E_f ; 50 ms, 600 ms)   (s)
//! ```
//!
//! L'historique `H` vit dans deux textures demi-résolution allouées à l'ouverture (`persist_node`). Il est lu **avant**
//! le flash du spectacle, qui s'ajoute dans la passe d'étalonnage : la persistance ne prolonge donc jamais un flash.
//! Ce module porte les nombres et la mémoire de la remise à zéro à chaque coupe de plan.

use cf_analysis::filters::AttackRelease;

/// Seuil de luminance au-dessus duquel un pixel laisse une trace (`θ_p`) : les émissifs E2 et plus.
pub const THRESHOLD: f32 = 1.0;
/// Poids de la traînée ajoutée à l'image.
pub const TRAIL_GAIN: f32 = 0.6;
/// `ζ` par unité de vitesse normalisée (`vitesse/v*`).
pub const ZETA_PER_SPEED: f32 = 0.004;
/// Point de fuite en UV (y vers le bas) : légèrement au-dessus du centre, là où la route disparaît.
pub const VANISHING_POINT: [f32; 2] = [0.5, 0.45];
const TAU_BASE: f32 = 0.06;
const TAU_FRESH: f32 = 0.2;
/// Plafond du pas de temps, pour qu'un gel ne fige pas une traînée géante.
const MAX_DT: f32 = 0.1;

/// `τ_p` (s) pour une fraîcheur filtrée `fresh` (0..1).
#[must_use]
pub fn tau(fresh: f32) -> f32 {
    TAU_BASE + TAU_FRESH * fresh.clamp(0.0, 1.0)
}

/// `ζ` : fraction de la distance au point de fuite parcourue à chaque image, bornée à `0..=0.02`.
#[must_use]
pub fn zeta(speed_ratio: f32) -> f32 {
    if speed_ratio.is_finite() {
        (ZETA_PER_SPEED * speed_ratio.max(0.0)).min(0.02)
    } else {
        0.0
    }
}

/// Facteur de décroissance de l'historique sur `dt` secondes.
#[must_use]
pub fn decay(tau_s: f32, dt: f32) -> f32 {
    (-dt.clamp(0.0, MAX_DT) / tau_s.max(1e-3)).exp()
}

/// Ce que la passe GPU lit chaque image.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PersistenceFrame {
    pub decay: f32,
    pub zeta: f32,
    /// 1 : ignorer l'historique (coupe de plan, premier affichage) ; 0 : le lire.
    pub reset: f32,
    pub enabled: bool,
}

/// État de la persistance : fraîcheur filtrée et mémoire de la dernière coupe vue.
#[derive(Debug, Clone, Copy)]
pub struct Persistence {
    fresh: AttackRelease,
    seen_cuts: Option<u64>,
    clear: bool,
}

impl Default for Persistence {
    fn default() -> Self {
        // `env(E_f ; 50 ms, 600 ms)`.
        Self {
            fresh: AttackRelease::new(0.05, 0.6),
            seen_cuts: None,
            clear: true,
        }
    }
}

impl Persistence {
    /// À appeler à chaque image. `cuts` : compteur de coupes de plan ; `fresh` : fraîcheur du spectacle (0..1) ;
    /// `speed_ratio` : vitesse normalisée ; `enabled` : faux aux paliers de qualité qui coupent l'effet.
    pub fn update(&mut self, cuts: u64, fresh: f32, speed_ratio: f32, dt: f32, enabled: bool) -> PersistenceFrame {
        let dt = dt.clamp(0.0, MAX_DT);
        let env = self.fresh.step(fresh.clamp(0.0, 1.0), dt);
        // Toute coupe (ou tout palier où l'effet est coupé) vide l'historique à l'image suivante.
        if self.seen_cuts.is_some_and(|seen| seen != cuts) || !enabled {
            self.clear = true;
        }
        self.seen_cuts = Some(cuts);
        let reset = if self.clear { 1.0 } else { 0.0 };
        self.clear = false;
        PersistenceFrame {
            decay: decay(tau(env), dt),
            zeta: zeta(speed_ratio),
            reset,
            enabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn la_trace_dure_plus_longtemps_quand_le_spectacle_est_frais() {
        assert!((tau(0.0) - 0.06).abs() < 1e-6);
        assert!((tau(1.0) - 0.26).abs() < 1e-6);
        assert!(decay(tau(1.0), DT) > decay(tau(0.0), DT));
    }

    #[test]
    fn les_traces_fuient_plus_vite_quand_on_roule_vite() {
        assert_eq!(zeta(0.0), 0.0);
        assert!(zeta(1.0) > zeta(0.5));
        assert!(zeta(1000.0) <= 0.02 && zeta(f32::NAN) == 0.0);
    }

    #[test]
    fn la_decroissance_ne_depend_pas_de_la_cadence() {
        let tau_s = 0.06;
        let a = decay(tau_s, 1.0 / 30.0);
        let b = decay(tau_s, 1.0 / 60.0).powi(2);
        assert!((a - b).abs() < 1e-5);
    }

    #[test]
    fn l_historique_est_vide_au_premier_affichage_et_a_chaque_coupe() {
        let mut p = Persistence::default();
        assert_eq!(p.update(0, 0.0, 0.5, DT, true).reset, 1.0);
        assert_eq!(p.update(0, 0.0, 0.5, DT, true).reset, 0.0);
        assert_eq!(p.update(1, 0.0, 0.5, DT, true).reset, 1.0);
        assert_eq!(p.update(1, 0.0, 0.5, DT, true).reset, 0.0);
        // Coupée par la qualité : l'historique est vidé, et le reste tant que l'effet est coupé.
        assert_eq!(p.update(1, 0.0, 0.5, DT, false).reset, 1.0);
        assert_eq!(p.update(1, 0.0, 0.5, DT, true).reset, 0.0);
    }

    /// La traînée d'un flash isolé : l'historique `H` d'un pixel qui s'éteint décroît sans jamais remonter, et la
    /// part ajoutée à l'image (`0.6·H`) tombe sous 2 % en moins de deux secondes : aucun flash prolongé.
    #[test]
    fn un_flash_ne_laisse_pas_de_traine_prolongee() {
        let mut h = 5.0_f32;
        let d = decay(tau(1.0), DT);
        let mut t = 0.0;
        let mut previous = h;
        while TRAIL_GAIN * h > 0.02 && t < 5.0 {
            h *= d;
            assert!(h <= previous);
            previous = h;
            t += DT;
        }
        assert!(t < 2.0, "traînée encore visible après {t} s");
    }
}
