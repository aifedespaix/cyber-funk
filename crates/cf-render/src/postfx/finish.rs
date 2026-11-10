//! Les formules de l'image finale de DA §3.1, en Rust : aberration ciblée, grain multiplicatif, halation.
//!
//! Le shader (`grade.wgsl`, `grain.wgsl`) les calcule en WGSL ; ce module en est le **miroir** de référence, testable
//! sans carte graphique (un test vérifie aussi que le shader de grain n'emploie aucun `sin`).

/// Plafond de l'aberration chromatique, en UV (DA §3.1 : `A ≤ 0.01`).
pub const ABERRATION_MAX: f32 = 0.01;
/// Rayon (en `r = 2·|uv − centre|`) en dessous duquel l'aberration est nulle : le sujet reste net.
pub const ABERRATION_CLEAR_RADIUS: f32 = 0.35;
/// Grain au calme / au sommet (DA §3.1).
pub const GRAIN_CALM: f32 = 0.06;
pub const GRAIN_PEAK: f32 = 0.03;
/// Cadence du grain (images de grain par seconde).
pub const GRAIN_FPS: f32 = 24.0;
/// Halation : seuil des hautes lumières, force, teinte chaude et rayon (part de la largeur).
pub const HALATION_THRESHOLD: f32 = 1.5;
pub const HALATION_GAIN: f32 = 0.25;
pub const HALATION_TINT: [f32; 3] = [1.0, 0.35, 0.15];
pub const HALATION_SIGMA: f32 = 0.004;

#[must_use]
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// `A(r, Y) = A0·smoothstep(0.35, 1, r)²·(0.3 + 0.7·smoothstep(0.5, 2, Y))·(1 + 3·env(TR))`, bornée à
/// [`ABERRATION_MAX`]. `r = 2·|uv − centre|` (0 au centre, 1 au milieu d'un bord), `y` la luminance de la source.
#[must_use]
pub fn targeted_aberration(a0: f32, r: f32, y: f32, transient_env: f32) -> f32 {
    let edge = smoothstep(ABERRATION_CLEAR_RADIUS, 1.0, r);
    let highlight = 0.3 + 0.7 * smoothstep(0.5, 2.0, y);
    (a0 * edge * edge * highlight * (1.0 + 3.0 * transient_env.clamp(0.0, 1.0))).clamp(0.0, ABERRATION_MAX)
}

/// Hachage entier `lowbias32` (Chris Wellons) : aucun `sin`, identique sur tous les pilotes.
#[must_use]
pub fn lowbias32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

/// Bruit uniforme 0..1 du pixel `(x, y)` à l'image de grain `frame`.
#[must_use]
pub fn grain_noise(x: u32, y: u32, frame: u32) -> f32 {
    let h = lowbias32(x ^ lowbias32(y.wrapping_add(0x9e37_79b9) ^ lowbias32(frame)));
    (h >> 8) as f32 / 16_777_216.0
}

/// `c' = c·(1 + G·(n − 0.5)·w(Y'))`, `w(Y') = 0.25 + 3·Y'·(1 − Y')` : maximal dans les demi-teintes, multiplicatif,
/// donc un pixel à 0 reste à 0. `luma` est la luminance affichée du pixel.
#[must_use]
pub fn grain_factor(noise: f32, luma: f32, gain: f32) -> f32 {
    let y = luma.clamp(0.0, 1.0);
    1.0 + gain * (noise - 0.5) * (0.25 + 3.0 * y * (1.0 - y))
}

/// Numéro de l'image de grain à l'instant `t` (s).
#[must_use]
pub fn grain_frame(t: f64) -> u32 {
    (t.max(0.0) * f64::from(GRAIN_FPS)).floor() as u32
}

/// Force du grain : `GRAIN_CALM` au calme, `GRAIN_PEAK` au sommet, interpolée par `peak` (0..1).
#[must_use]
pub fn grain_gain(peak: f32) -> f32 {
    GRAIN_CALM + (GRAIN_PEAK - GRAIN_CALM) * peak.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_aberration_est_nulle_dans_le_disque_central() {
        for i in 0..=35 {
            let r = i as f32 / 100.0;
            for y in [0.0, 1.0, 10.0] {
                assert_eq!(targeted_aberration(ABERRATION_MAX, r, y, 1.0), 0.0, "r = {r}");
            }
        }
        assert!(targeted_aberration(0.006, 1.2, 2.0, 0.0) > 0.0);
    }

    #[test]
    fn l_aberration_est_bornee_et_monte_avec_les_hautes_lumieres_et_les_transitoires() {
        let sombre = targeted_aberration(0.006, 1.0, 0.0, 0.0);
        let clair = targeted_aberration(0.006, 1.0, 3.0, 0.0);
        let frappe = targeted_aberration(0.006, 1.0, 3.0, 1.0);
        assert!(sombre < clair && clair < frappe);
        assert!((clair / sombre - 1.0 / 0.3).abs() < 1e-3);
        assert!(targeted_aberration(1.0, 1.4, 9.0, 1.0) <= ABERRATION_MAX);
    }

    #[test]
    fn le_grain_est_multiplicatif_les_noirs_restent_noirs() {
        for frame in 0..48 {
            for x in 0..16 {
                let n = grain_noise(x, 3, frame);
                assert!((0.0..1.0).contains(&n));
                assert_eq!(0.0 * grain_factor(n, 0.0, GRAIN_CALM), 0.0);
            }
        }
    }

    #[test]
    fn le_grain_est_centre_et_maximal_dans_les_demi_teintes() {
        let n = 200_000;
        let mean: f64 = (0..n).map(|i| f64::from(grain_noise(i % 512, i / 512, 7))).sum::<f64>() / f64::from(n);
        assert!((mean - 0.5).abs() < 0.01, "{mean}");
        let amp = |l: f32| (grain_factor(1.0, l, GRAIN_CALM) - 1.0).abs();
        assert!(amp(0.5) > amp(0.0) && amp(0.5) > amp(1.0));
        assert!((amp(0.5) - 0.03 * 1.0).abs() < 1e-3);
    }

    #[test]
    fn le_grain_change_a_24_images_par_seconde() {
        assert_eq!(grain_frame(0.0), 0);
        assert_eq!(grain_frame(1.0 / 24.0 - 1e-6), 0);
        assert_eq!(grain_frame(1.0 / 24.0 + 1e-6), 1);
        assert!(grain_gain(1.0) < grain_gain(0.0));
    }

    #[test]
    fn le_shader_de_grain_n_emploie_aucun_sin() {
        let source = include_str!("../../../../assets/shaders/cf-render/grain.wgsl");
        let code: String = source.lines().filter(|l| !l.trim_start().starts_with("//")).collect();
        assert!(!code.contains("sin("), "le hachage du grain doit être entier");
    }
}
