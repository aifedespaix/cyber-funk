//! Les points d'attache des projecteurs de la ville visible (`scenery.spots` de `scenery.ts`).
//!
//! Sillage les ramasse en re-tirant toute la ville à chaque changement de rangée. Ici la fenêtre de [`ROWS`] rangées
//! autour de la voiture est mise en cache : seule la rangée qui entre est re-tirée (`Row::generate`, sur la pile), le reste
//! est recopié. Un changement de réglages de la ville (qualité, zones dégagées) re-tire tout. Aucune allocation en
//! régime établi : les tampons gardent leur capacité.

use crate::scenery::{Row, SPACING_M, SceneryConfig, SpotAnchor};
use crate::world::BEHIND;

use super::field::MAX_BEAMS;

/// Rangées de la fenêtre de ville (`ROWS` de `scenery.ts`) : `BEHIND` derrière la voiture, la rangée courante, le reste devant.
pub const ROWS: usize = 34;
/// [`ROWS`] en `i64`, pour les index de rangées (signés : la route file vers les z négatifs).
const ROWS_I64: i64 = 34;

#[derive(Debug, Clone, Default)]
struct CachedRow {
    /// Index de la rangée en cache, `None` tant qu'elle n'est pas tirée.
    index: Option<i64>,
    spots: Vec<SpotAnchor>,
}

/// Le cache des points d'attache.
#[derive(Debug, Clone)]
pub struct SpotWindow {
    rows: Vec<CachedRow>,
    config: Option<SceneryConfig>,
    out: Vec<SpotAnchor>,
}

impl Default for SpotWindow {
    fn default() -> Self {
        Self {
            rows: vec![CachedRow::default(); ROWS],
            config: None,
            out: Vec::with_capacity(MAX_BEAMS),
        }
    }
}

impl SpotWindow {
    /// Index de la première rangée de la fenêtre quand la voiture est en `car_z` (absolu, m).
    #[must_use]
    pub fn base_row(car_z: f64) -> i64 {
        (-car_z / SPACING_M).floor() as i64 - i64::from(BEHIND)
    }

    /// Les points d'attache de la fenêtre autour de la voiture, dans l'ordre des rangées, au plus [`MAX_BEAMS`].
    pub fn refresh(&mut self, car_z: f64, config: &SceneryConfig) -> &[SpotAnchor] {
        if self.config.as_ref() != Some(config) {
            // Réglages changés : tout est à re-tirer. `clone_from` réutilise la capacité des zones dégagées.
            match &mut self.config {
                Some(old) => old.clone_from(config),
                None => self.config = Some(config.clone()),
            }
            for row in &mut self.rows {
                row.index = None;
            }
        }
        let base = Self::base_row(car_z);
        self.out.clear();
        for index in base..base + ROWS_I64 {
            let cached = &mut self.rows[index.rem_euclid(ROWS_I64) as usize];
            if cached.index != Some(index) {
                cached.index = Some(index);
                cached.spots.clear();
                cached.spots.extend_from_slice(Row::generate(index, config).spots());
            }
            let room = MAX_BEAMS - self.out.len();
            self.out.extend(cached.spots.iter().take(room));
        }
        &self.out
    }
}

#[cfg(test)]
mod tests {
    use crate::scenery::Clearing;

    use super::*;

    #[test]
    fn la_fenetre_donne_les_memes_points_que_le_tirage_direct() {
        let config = SceneryConfig::default();
        let mut window = SpotWindow::default();
        let car_z = -5000.0;
        let got = window.refresh(car_z, &config).to_vec();
        let base = SpotWindow::base_row(car_z);
        let mut expected = Vec::new();
        for row in base..base + ROWS_I64 {
            expected.extend_from_slice(Row::generate(row, &config).spots());
        }
        expected.truncate(MAX_BEAMS);
        assert_eq!(got, expected);
        assert!(!got.is_empty(), "une ville a des projecteurs");
    }

    #[test]
    fn avancer_ne_retire_que_la_rangee_qui_entre() {
        let config = SceneryConfig::default();
        let mut window = SpotWindow::default();
        window.refresh(-5000.0, &config);
        let before: Vec<Option<i64>> = window.rows.iter().map(|r| r.index).collect();
        window.refresh(-5000.0 - SPACING_M, &config);
        let changed = window
            .rows
            .iter()
            .zip(&before)
            .filter(|(now, was)| now.index != **was)
            .count();
        assert_eq!(changed, 1, "une seule rangée est re-tirée par pas de rangée");
    }

    #[test]
    fn un_changement_de_reglages_retire_tout() {
        let mut config = SceneryConfig::default();
        let mut window = SpotWindow::default();
        let car_z = -2000.0;
        let before = window.refresh(car_z, &config).len();
        // Une zone dégagée sur toute la fenêtre efface les tours des deux côtés.
        config.set_clearings(&[Clearing {
            z0: car_z - 1000.0,
            z1: car_z + 500.0,
            side: 0,
        }]);
        let after = window.refresh(car_z, &config).len();
        assert!(before > 0 && after < before, "{before} -> {after}");
    }
}
