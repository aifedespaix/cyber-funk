//! Fenêtre glissante des temps d'image : moyenne et percentile sans allocation en régime établi.

/// Les `capacity` derniers temps d'image (en ms), dans un anneau préalloué.
///
/// Le percentile trie une copie de travail préallouée : aucune allocation après `new`, et la fenêtre reste lisible à
/// l'œil (600 images = 10 s à 60 i/s) sans que le p99 soit dominé par un accroc isolé d'il y a une minute.
#[derive(Debug, Clone)]
pub struct FrameWindow {
    samples: Vec<f32>,
    scratch: Vec<f32>,
    next: usize,
    len: usize,
}

impl FrameWindow {
    /// Fenêtre de `capacity` images (au moins une).
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            samples: vec![0.0; capacity],
            scratch: Vec::with_capacity(capacity),
            next: 0,
            len: 0,
        }
    }

    /// Ajoute un temps d'image ; le plus ancien sort quand la fenêtre est pleine. Les valeurs non finies ou
    /// négatives sont ignorées (une image sans durée mesurable n'est pas une mesure).
    pub fn push(&mut self, ms: f32) {
        if !ms.is_finite() || ms < 0.0 {
            return;
        }
        if let Some(slot) = self.samples.get_mut(self.next) {
            *slot = ms;
        }
        self.next = (self.next + 1) % self.samples.len();
        self.len = (self.len + 1).min(self.samples.len());
    }

    /// Nombre de mesures dans la fenêtre.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Vrai tant qu'aucune mesure n'a été ajoutée.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Vide la fenêtre (changement de scène, début d'une session de mesure).
    pub fn clear(&mut self) {
        self.next = 0;
        self.len = 0;
    }

    /// Moyenne des mesures, ou `None` si la fenêtre est vide.
    #[must_use]
    pub fn mean(&self) -> Option<f32> {
        if self.len == 0 {
            return None;
        }
        // Tant que la fenêtre n'est pas pleine, les mesures occupent le début de l'anneau.
        let sum: f32 = self.samples.iter().take(self.len).sum();
        Some(sum / self.len as f32)
    }

    /// Percentile `p` (0 à 100, méthode du rang le plus proche), ou `None` si la fenêtre est vide.
    pub fn percentile(&mut self, p: f32) -> Option<f32> {
        if self.len == 0 {
            return None;
        }
        self.scratch.clear();
        self.scratch.extend(self.samples.iter().take(self.len));
        self.scratch.sort_unstable_by(f32::total_cmp);
        let rank = ((p.clamp(0.0, 100.0) / 100.0) * self.len as f32).ceil() as usize;
        self.scratch.get(rank.saturating_sub(1).min(self.len - 1)).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_moyenne_et_le_percentile_suivent_les_mesures() {
        let mut window = FrameWindow::new(100);
        assert_eq!(window.mean(), None);
        assert_eq!(window.percentile(99.0), None);
        for _ in 0..99 {
            window.push(10.0);
        }
        window.push(110.0);
        assert!((window.mean().unwrap_or_default() - 11.0).abs() < 1e-4);
        assert_eq!(window.percentile(99.0), Some(10.0), "rang 99 sur 100 : l'accroc est le centième");
        assert_eq!(window.percentile(100.0), Some(110.0));
    }

    #[test]
    fn la_fenetre_oublie_les_plus_anciennes_mesures() {
        let mut window = FrameWindow::new(4);
        for ms in [100.0, 100.0, 1.0, 1.0, 1.0, 1.0] {
            window.push(ms);
        }
        assert_eq!(window.len(), 4);
        assert_eq!(window.mean(), Some(1.0));
    }

    #[test]
    fn les_valeurs_invalides_sont_ignorees() {
        let mut window = FrameWindow::new(4);
        window.push(f32::NAN);
        window.push(-1.0);
        window.push(f32::INFINITY);
        assert!(window.is_empty());
    }
}
