//! Le tirage en deux temps : le rang (probabilités de base et garde-fous), puis le plan dans le rang (poids × énergie ×
//! fraîcheur × humeur du morceau).

use super::tables::{
    BREAK_BONUS, BREAK_SECONDS, BUILD_BONUS, BUILD_SECONDS, COCKPIT_CAP, DROP_BONUS, PROJECTOR_WEIGHT, lookup,
};
use super::{Editor, PickMode};
use crate::choreo::ShowSide;
use crate::shot::ShotSpec;
use crate::types::ShotTier;
use crate::world::CinemaFrame;

/// Probabilité de base de chaque rang (`TIER_P` de Sillage), dans l'ordre de [`ShotTier::ALL`].
pub const TIER_P: [f32; 4] = [0.7, 0.22, 0.07, 0.01];
/// Un drop doit tomber sur un plan énergique : l'énergie vue par le tirage ne descend pas sous ce plancher.
const DROP_ENERGY_FLOOR: f32 = 0.85;
/// Largeur (énergie) de la cloche qui pondère un plan hors de sa plage.
const ENERGY_SIGMA: f32 = 0.25;
/// Plancher de la pondération d'énergie : un plan hors plage reste possible, jamais exclu.
const ENERGY_FLOOR: f32 = 0.03;

/// Pondération gaussienne : 1 dans la plage d'énergie du plan, qui chute en dehors.
fn energy_fit(spec: &dyn ShotSpec, energy: f32) -> f32 {
    let (low, high) = spec.energy();
    let d = if energy < low {
        low - energy
    } else if energy > high {
        energy - high
    } else {
        0.0
    };
    (-0.5 * (d / ENERGY_SIGMA).powi(2)).exp().max(ENERGY_FLOOR)
}

impl Editor {
    pub(super) fn draw(
        &mut self,
        frame: &CinemaFrame<'_>,
        show: &dyn ShowSide,
        catalog: &[Box<dyn ShotSpec>],
        mode: PickMode,
        level: u8,
        current: Option<usize>,
    ) -> Option<usize> {
        let mut pool = std::mem::take(&mut self.pool);
        pool.clear();
        let found = self.draw_in(&mut pool, frame, show, catalog, mode, level, current);
        self.pool = pool;
        found
    }

    #[allow(clippy::too_many_arguments)] // Le contexte d'un tirage, passé tel quel.
    fn draw_in(
        &mut self,
        pool: &mut Vec<(usize, f32)>,
        frame: &CinemaFrame<'_>,
        show: &dyn ShowSide,
        catalog: &[Box<dyn ShotSpec>],
        mode: PickMode,
        level: u8,
        current: Option<usize>,
    ) -> Option<usize> {
        let strict = level == 0;
        let now = frame.time;
        let energy = if mode == PickMode::Drop {
            frame.intensity.max(DROP_ENERGY_FLOOR)
        } else {
            frame.intensity
        };
        let in_build = now - self.build_at < BUILD_SECONDS;
        let in_break = now - self.break_at < BREAK_SECONDS;
        let cars_only =
            (mode == PickMode::Start && level < 2) || (strict && mode == PickMode::Ended && self.after_special);
        let active = frame.show.active;
        let mut count = [0_u32; 4];

        for (index, spec) in catalog.iter().enumerate() {
            let spec = spec.as_ref();
            if level < 2 && Some(index) == current {
                continue;
            }
            if strict && self.recent.contains(&index) {
                continue;
            }
            if !self.allowed(frame, show, spec, index, level, false) {
                continue;
            }
            if strict && self.breather > 0 && spec.tier() != ShotTier::Common {
                continue;
            }
            if cars_only && (!spec.follows_car() || spec.tier() != ShotTier::Common) {
                continue;
            }
            if mode == PickMode::Drop && matches!(spec.tier(), ShotTier::Rare | ShotTier::Epic) {
                continue;
            }
            let id = spec.id();
            let mut w = spec.weight().max(0.0) * energy_fit(spec, energy);
            if active {
                w *= self.freshness(frame, show, index) * lookup(PROJECTOR_WEIGHT, id).unwrap_or(1.0);
                if self.cockpit == Some(index) {
                    let share = self.screen.share(index, now);
                    if share >= COCKPIT_CAP {
                        continue;
                    }
                    w *= (1.0 - 0.8 * (share / COCKPIT_CAP)) as f32;
                }
            }
            if in_build {
                w *= lookup(BUILD_BONUS, id).unwrap_or(1.0);
            }
            if in_break {
                w *= lookup(BREAK_BONUS, id).unwrap_or(1.0);
            }
            if mode == PickMode::Drop && !(active && lookup(PROJECTOR_WEIGHT, id).is_some()) {
                w *= lookup(DROP_BONUS, id).unwrap_or(1.0);
            }
            if w <= 0.0 {
                continue;
            }
            pool.push((index, w));
            count[spec.tier().index()] += 1;
        }
        if pool.is_empty() {
            return None;
        }

        let due = self.car_streak >= self.streak_goal;
        let tier_weight = |tier: ShotTier| -> f32 {
            if count[tier.index()] == 0 {
                return 0.0;
            }
            let mut p = TIER_P[tier.index()];
            match tier {
                ShotTier::Uncommon => p *= (if due { 1.8 } else { 1.0 }) * (if in_build { 1.5 } else { 1.0 }),
                ShotTier::Rare => p *= (if due { 1.3 } else { 1.0 }) * (if in_break { 2.0 } else { 1.0 }),
                ShotTier::Epic if in_break && frame.intensity < 0.4 => p *= 12.0,
                _ => {}
            }
            p
        };
        let total: f32 = ShotTier::ALL.iter().map(|&t| tier_weight(t)).sum();
        let mut roll = self.rng.next_f32() * total;
        let mut chosen = ShotTier::Common;
        for tier in ShotTier::ALL {
            let p = tier_weight(tier);
            if p == 0.0 {
                continue;
            }
            chosen = tier;
            roll -= p;
            if roll <= 0.0 {
                break;
            }
        }

        let in_tier = |index: usize| catalog.get(index).is_some_and(|s| s.tier() == chosen);
        let sum: f32 = pool.iter().filter(|&&(i, _)| in_tier(i)).map(|&(_, w)| w).sum();
        let mut roll = self.rng.next_f32() * sum;
        let mut found = None;
        for &(index, w) in pool.iter() {
            if !in_tier(index) {
                continue;
            }
            found = Some(index);
            roll -= w;
            if roll <= 0.0 {
                break;
            }
        }
        found
    }
}
