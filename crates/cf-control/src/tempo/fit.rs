//! Historique d'horodatages à taille fixe et droite des moindres carrés « instant en fonction du rang ».
//!
//! Pourquoi une régression plutôt qu'une moyenne des intervalles : la moyenne de `n` intervalles ne dépend que du
//! premier et du dernier horodatage, donc la gigue de deux impulsions seulement fixe l'erreur. La pente de la droite
//! utilise toutes les impulsions : son écart type décroît en `n^-3/2` au lieu de `n^-1`. Avec une gigue de ±2 ms sur
//! deux mesures d'horloge (192 impulsions), l'erreur sur le tempo reste sous le centième de BPM.

/// Une droite ajustée sur les derniers horodatages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Fit {
    /// Pente : durée estimée entre deux horodatages consécutifs (s).
    pub interval_s: f64,
    /// Instant ajusté du dernier horodatage (s) : débarrassé de la gigue, il sert d'ancre de phase.
    pub last_s: f64,
    /// Écart quadratique moyen des horodatages autour de la droite (s) : mesure de la gigue.
    pub rms_s: f64,
    /// Nombre d'horodatages ajustés.
    pub count: usize,
}

/// Les `N` derniers horodatages, dans un tableau fixe : aucune allocation, même en régime établi.
#[derive(Debug, Clone)]
pub(crate) struct History<const N: usize> {
    times: [f64; N],
    /// Prochaine case écrite.
    head: usize,
    /// Nombre de cases valides, ≤ `N`.
    len: usize,
}

impl<const N: usize> History<N> {
    pub const fn new() -> Self {
        Self {
            times: [0.0; N],
            head: 0,
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub fn push(&mut self, time_s: f64) {
        self.times[self.head] = time_s;
        self.head = (self.head + 1) % N;
        self.len = (self.len + 1).min(N);
    }

    /// Ne garde que les `keep` horodatages les plus récents.
    pub fn keep_last(&mut self, keep: usize) {
        self.len = self.len.min(keep);
    }

    /// Le `k`-ième horodatage en partant du plus ancien des `m` derniers.
    fn nth_of_last(&self, m: usize, k: usize) -> f64 {
        self.times[(self.head + N - m + k) % N]
    }

    /// Dernier horodatage poussé.
    pub fn last(&self) -> Option<f64> {
        (self.len > 0).then(|| self.times[(self.head + N - 1) % N])
    }

    /// Droite des moindres carrés sur les `m` derniers horodatages (`m` borné par ce qui est connu). `None` sous deux
    /// points.
    pub fn fit_last(&self, m: usize) -> Option<Fit> {
        let m = m.min(self.len);
        if m < 2 {
            return None;
        }
        // Instants relatifs au plus ancien : les horloges comptent en secondes depuis le démarrage, les écarts sont de
        // l'ordre de la milliseconde ; soustraire d'abord garde toute la précision des `f64`.
        let origin = self.nth_of_last(m, 0);
        let count = m as f64;
        let x_mean = (count - 1.0) / 2.0;
        let mut t_sum = 0.0;
        for k in 0..m {
            t_sum += self.nth_of_last(m, k) - origin;
        }
        let t_mean = t_sum / count;
        let (mut sxy, mut sxx) = (0.0, 0.0);
        for k in 0..m {
            let dx = k as f64 - x_mean;
            sxy += dx * (self.nth_of_last(m, k) - origin - t_mean);
            sxx += dx * dx;
        }
        let interval_s = sxy / sxx;
        let mut sq = 0.0;
        for k in 0..m {
            let predicted = t_mean + interval_s * (k as f64 - x_mean);
            let residual = self.nth_of_last(m, k) - origin - predicted;
            sq += residual * residual;
        }
        Some(Fit {
            interval_s,
            last_s: origin + t_mean + interval_s * (count - 1.0 - x_mean),
            rms_s: (sq / count).sqrt(),
            count: m,
        })
    }
}

/// Écart type relatif de la pente d'une droite ajustée sur `m` points de gigue `rms_s`, rapporté à `interval_s`
/// (`σ_pente = σ · √(12 / (m (m² − 1)))`).
pub(crate) fn slope_rel_sigma(rms_s: f64, interval_s: f64, m: usize) -> f64 {
    let m = m as f64;
    if m < 2.0 || interval_s <= 0.0 {
        return f64::INFINITY;
    }
    rms_s / interval_s * (12.0 / (m * (m * m - 1.0))).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_line_is_recovered() {
        let mut h = History::<8>::new();
        for k in 0..12 {
            h.push(100.0 + 0.5 * f64::from(k));
        }
        assert_eq!(h.len(), 8);
        let Some(fit) = h.fit_last(8) else {
            unreachable!("huit points connus")
        };
        assert!((fit.interval_s - 0.5).abs() < 1e-12);
        assert!((fit.last_s - 105.5).abs() < 1e-9);
        assert!(fit.rms_s < 1e-9);
        assert_eq!(h.last(), Some(105.5));
    }

    #[test]
    fn keep_last_and_too_few_points() {
        let mut h = History::<8>::new();
        h.push(1.0);
        assert!(h.fit_last(8).is_none());
        h.push(2.0);
        h.push(4.0);
        h.keep_last(2);
        let Some(fit) = h.fit_last(8) else {
            unreachable!("deux points gardés")
        };
        assert_eq!(fit.count, 2);
        assert!((fit.interval_s - 2.0).abs() < 1e-12);
        h.clear();
        assert_eq!(h.last(), None);
    }
}
