//! Ajustement d'une grille régulière (tempo + phase) sur des kicks détectés.
//!
//! La détection en direct donne un tempo arrondi et des kicks légèrement bruités. Sur une piste entière, une
//! régression des instants de kick sur leur numéro de temps donne le tempo à mieux que 0,01 BPM : la dérive de phase
//! en fin de piste reste de l'ordre de la milliseconde.

/// Tolérance de rattachement d'un kick à un temps, en fraction de période, au premier passage.
const FIRST_PASS_TOLERANCE: f64 = 0.25;

/// Tolérance aux passages suivants : les kicks hors grille (syncopes, faux positifs) ne tirent plus la régression.
const REFINE_TOLERANCE: f64 = 0.12;

/// Kicks du premier ajustement : assez pour fixer la phase, assez peu pour que l'erreur de l'indice de tempo ne
/// décale pas les derniers de plus d'un quart de temps (24 temps à 0,5 % près : 0,12 temps).
const INITIAL_KICKS: usize = 16;

/// Écart quadratique maximal (en périodes) d'une fenêtre de départ : au-delà, ce n'est pas une grille.
const SEED_RMS: f64 = 0.1;

/// Nombre de passes de régression par fenêtre.
const PASSES: usize = 4;

/// Moins de kicks retenus que cela : pas de grille fiable.
const MIN_KICKS: usize = 8;

/// Résultat de l'ajustement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Fit {
    /// Période d'un temps (s).
    pub period_s: f64,
    /// Instant d'un temps entier (le temps numéro 0 de l'ajustement), en s.
    pub origin_s: f64,
    /// Écart quadratique moyen des kicks retenus à la grille (s).
    pub rms_s: f64,
    /// Nombre de kicks retenus.
    pub kept: usize,
}

/// Ajuste `t = origin + n · period` sur les instants de `kicks` (croissants), départ au tempo `bpm_hint`.
///
/// Le tempo détecté en direct est arrondi : appliqué tel quel à toute la piste, il décalerait les derniers kicks de
/// plusieurs temps et les rattacherait au mauvais numéro. On cherche donc d'abord une courte fenêtre de kicks bien
/// réguliers (une intro peut être vide ou syncopée), on l'ajuste, puis on élargit la fenêtre des deux côtés en
/// doublant : chaque ajustement prédit assez bien la suite pour la rattacher.
pub(crate) fn fit_grid(kicks: &[f64], bpm_hint: f64) -> Option<Fit> {
    if kicks.len() < MIN_KICKS || !(bpm_hint.is_finite() && bpm_hint > 0.0) {
        return None;
    }
    let (mut lo, mut hi, mut best) = seed(kicks, 60.0 / bpm_hint)?;
    while lo > 0 || hi < kicks.len() {
        let half = (hi - lo) / 2 + 1;
        lo = lo.saturating_sub(half);
        hi = (hi + half).min(kicks.len());
        best = converge(&kicks[lo..hi], best.origin_s, best.period_s, REFINE_TOLERANCE)?;
    }
    Some(best)
}

/// Meilleure fenêtre de [`INITIAL_KICKS`] kicks (le plus de kicks sur la grille, puis le plus régulier) :
/// `(début, fin, ajustement)`. `None` si aucune fenêtre ne ressemble à une grille.
fn seed(kicks: &[f64], period: f64) -> Option<(usize, usize, Fit)> {
    let width = INITIAL_KICKS.min(kicks.len());
    (0..=kicks.len() - width)
        .step_by(width / 2)
        .filter_map(|lo| {
            let window = &kicks[lo..lo + width];
            let fit = converge(window, circular_phase(window, period), period, FIRST_PASS_TOLERANCE)?;
            (fit.kept * 2 >= width && fit.rms_s < SEED_RMS * fit.period_s).then_some((lo, lo + width, fit))
        })
        .max_by(|a, b| a.2.kept.cmp(&b.2.kept).then(b.2.rms_s.total_cmp(&a.2.rms_s)))
}

/// [`PASSES`] passes de [`refit`], la première avec `tolerance`, les suivantes avec la tolérance serrée.
fn converge(kicks: &[f64], mut origin: f64, mut period: f64, mut tolerance: f64) -> Option<Fit> {
    let mut best = None;
    for _ in 0..PASSES {
        let (fit, next) = refit(kicks, origin, period, tolerance)?;
        (origin, period) = next;
        best = Some(fit);
        tolerance = REFINE_TOLERANCE;
    }
    best
}

/// Une passe : rattache chaque kick au temps le plus proche de la grille courante, ignore ceux qui en sont loin, et
/// ajuste droite et écart. Rend l'ajustement et `(origine, période)`.
fn refit(kicks: &[f64], origin: f64, period: f64, tolerance: f64) -> Option<(Fit, (f64, f64))> {
    let pairs: Vec<(f64, f64)> = kicks
        .iter()
        .filter_map(|&t| {
            let n = ((t - origin) / period).round();
            ((t - origin - n * period).abs() < tolerance * period).then_some((n, t))
        })
        .collect();
    if pairs.len() < MIN_KICKS {
        return None;
    }
    let (slope, intercept) = regression(&pairs)?;
    if slope <= 0.0 {
        return None;
    }
    let kept = pairs.len();
    let rms_s = (pairs
        .iter()
        .map(|&(n, t)| (t - intercept - n * slope).powi(2))
        .sum::<f64>()
        / kept as f64)
        .sqrt();
    Some((
        Fit {
            period_s: slope,
            origin_s: intercept,
            rms_s,
            kept,
        },
        (intercept, slope),
    ))
}

/// Phase moyenne des kicks modulo la période (moyenne circulaire : insensible au saut de 0 à `period`).
fn circular_phase(kicks: &[f64], period: f64) -> f64 {
    let (sin, cos) = kicks.iter().fold((0.0, 0.0), |(s, c), &t| {
        let angle = std::f64::consts::TAU * (t / period).fract();
        (s + angle.sin(), c + angle.cos())
    });
    sin.atan2(cos).rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU * period
}

/// Droite des moindres carrés `t = intercept + slope · n` ; `None` si tous les `n` sont égaux.
fn regression(pairs: &[(f64, f64)]) -> Option<(f64, f64)> {
    let count = pairs.len() as f64;
    let mean_n = pairs.iter().map(|p| p.0).sum::<f64>() / count;
    let mean_t = pairs.iter().map(|p| p.1).sum::<f64>() / count;
    let var = pairs.iter().map(|p| (p.0 - mean_n).powi(2)).sum::<f64>();
    if var < 1e-9 {
        return None;
    }
    let cov = pairs.iter().map(|p| (p.0 - mean_n) * (p.1 - mean_t)).sum::<f64>();
    let slope = cov / var;
    Some((slope, mean_t - slope * mean_n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_a_tempo_finer_than_the_hint() {
        // 127,3 BPM exact, kicks bruités de ±5 ms, indice donné à 127 BPM.
        let period = 60.0 / 127.3;
        let kicks: Vec<f64> = (0..400)
            .map(|n| 0.31 + f64::from(n) * period + if n % 2 == 0 { 0.005 } else { -0.005 })
            .collect();
        let fit = fit_grid(&kicks, 127.0).expect("grille");
        assert!((60.0 / fit.period_s - 127.3).abs() < 0.01, "{}", 60.0 / fit.period_s);
        let drift = (fit.origin_s + 399.0 * fit.period_s) - (0.31 + 399.0 * period);
        assert!(drift.abs() < 0.005, "{drift}");
    }

    #[test]
    fn outliers_do_not_pull_the_grid() {
        let period = 0.5;
        let mut kicks: Vec<f64> = (0..200).map(|n| 1.0 + f64::from(n) * period).collect();
        // Des kicks à contretemps.
        for n in (5..200).step_by(7) {
            kicks.push(1.0 + f64::from(n) * period + 0.25);
        }
        kicks.sort_by(f64::total_cmp);
        let fit = fit_grid(&kicks, 120.0).expect("grille");
        assert!((fit.period_s - 0.5).abs() < 1e-3, "{}", fit.period_s);
        assert!(fit.rms_s < 0.002, "{}", fit.rms_s);
    }

    #[test]
    fn too_few_kicks_give_no_grid() {
        assert!(fit_grid(&[1.0, 1.5, 2.0], 120.0).is_none());
    }
}
