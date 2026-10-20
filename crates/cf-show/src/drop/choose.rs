//! Les tirages du chef d'orchestre : titre, cascade, plan caméra, tous pondérés par la fraîcheur.
//!
//! L'ordre des appels au RNG est celui de `drop.ts` : à graine égale, la suite des choix est stable.

use std::sync::Arc;

use rand_core::Rng;

use super::titles::DropTitles;
use super::{DropConductor, DropInput, MAX_LEVEL, ShowLevel};
use crate::freshness::Freshness;
use crate::model::{ShotId, ShowGates, StuntKind};

/// Plans qui se coupent bien sur un drop, avec leur rang (lu dans les fiches de `cinema/shots` de Sillage). Le
/// réalisateur (J6) vérifie ces identifiants contre le catalogue réel.
const DROP_SHOTS: [(&str, &str); 7] = [
    ("chase", "common"),
    ("side-track", "common"),
    ("bumper", "common"),
    ("orbit", "common"),
    ("wheel", "uncommon"),
    ("flyby", "uncommon"),
    ("strobe", "uncommon"),
];
/// Plan de cascade, pris dès que le rang « uncommon » est permis.
const STUNT_SHOT: &str = "stunt-cam";
/// Cascade rare, qui a sa propre porte (`ascension`).
const ASCENSION: &str = "ascension";
/// Un drop sur 4 à 6 porte un carton-titre, plus souvent aux niveaux hauts (voir `draw_gap`).
const TITLE_MIN_LEVEL: ShowLevel = 2;
/// Chance de l'Ascension sur un drop, quand elle est permise et reposée.
const ASCENSION_CHANCE: f64 = 0.2;
/// Poids de fraîcheur au-dessus duquel l'Ascension est « reposée ».
const ASCENSION_RESTED: f64 = 0.9;

/// Réel uniforme dans `[0, 1)`, 53 bits (équivalent de `Math.random`, mais avec la graine du show).
pub(super) fn unit<R: Rng>(rng: &mut R) -> f64 {
    (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64
}

/// Écrit `prefix` + `id` dans le tampon `buffer` (sans allouer une fois sa capacité atteinte) et le renvoie.
pub(super) fn key<'a>(buffer: &'a mut String, prefix: &str, id: &str) -> &'a str {
    buffer.clear();
    buffer.push_str(prefix);
    buffer.push_str(id);
    buffer
}

/// Déclare à la fraîcheur tous les plans de drop et tous les titres, à zéro usage.
pub(super) fn register_known(titles: &DropTitles, freshness: &mut Freshness) {
    let shots: Vec<String> = DROP_SHOTS.iter().map(|(id, _)| format!("shot:{id}")).collect();
    freshness.known(&shots);
    let keys: Vec<&str> = titles.keys().collect();
    freshness.known(&keys);
}

fn shot_allowed(gates: &ShowGates, id: &str, tier: &str) -> bool {
    gates.shot_tiers.iter().any(|t| t.as_str() == tier) && !gates.shot_block.iter().any(|b| b.as_str() == id)
}

impl<R: Rng> DropConductor<R> {
    /// Écart (en drops) jusqu'au prochain titre : 4 à 6, raccourci aux niveaux 4 et 5.
    pub(super) fn draw_gap(&mut self, level: ShowLevel) -> i32 {
        4 + (self.unit() * 3.0).floor() as i32 - i32::from(level >= MAX_LEVEL) - i32::from(level >= 4)
    }

    /// Le titre du drop qui frappe, ou rien : le compte se fait sur les vrais drops, avec la fraîcheur pour varier.
    pub(super) fn choose_title(&mut self, freshness: &mut Freshness, input: &DropInput<'_>) -> Option<Arc<str>> {
        if input.level < TITLE_MIN_LEVEL {
            return None;
        }
        self.until_title -= 1;
        if self.until_title > 0 {
            return None;
        }
        self.until_title = self.draw_gap(input.level);
        let now = input.now;
        let entries = self.titles.entries();
        let total: f64 = entries
            .iter()
            .filter(|e| e.min <= input.level)
            .map(|e| freshness.weight(&e.key, now))
            .sum();
        let mut roll = unit(&mut self.rng) * total;
        for entry in entries.iter().filter(|e| e.min <= input.level) {
            roll -= freshness.weight(&entry.key, now);
            if roll <= 0.0 {
                freshness.mark(&entry.key, now);
                return Some(entry.text(input.language));
            }
        }
        None
    }

    pub(super) fn choose_stunt(&mut self, freshness: &Freshness, input: &DropInput<'_>) -> Option<StuntKind> {
        let gates = input.gates;
        if gates.stunt_kinds.is_empty() {
            return None;
        }
        let now = input.now;
        if gates.ascension
            && self.unit() < ASCENSION_CHANCE
            && freshness.weight(key(&mut self.key, "stunt:", ASCENSION), now) > ASCENSION_RESTED
        {
            return StuntKind::new(ASCENSION).ok();
        }
        if self.unit() > 0.25 + 0.55 * f64::from(input.heat()) {
            return None;
        }
        let mut weight = |kind: &StuntKind| freshness.weight(key(&mut self.key, "stunt:", kind.as_str()), now);
        let total: f64 = gates.stunt_kinds.iter().map(&mut weight).sum();
        let mut roll = unit(&mut self.rng) * total;
        for kind in &gates.stunt_kinds {
            roll -= weight(kind);
            if roll <= 0.0 {
                return Some(kind.clone());
            }
        }
        gates.stunt_kinds.last().cloned()
    }

    pub(super) fn choose_shot(&mut self, freshness: &Freshness, input: &DropInput<'_>, stunt: bool) -> Option<ShotId> {
        let gates = input.gates;
        let now = input.now;
        let stunt_cam_ok = gates.shot_tiers.iter().any(|t| t.as_str() == "uncommon")
            && !gates.shot_block.iter().any(|b| b.as_str() == STUNT_SHOT);
        if stunt && stunt_cam_ok {
            return ShotId::new(STUNT_SHOT).ok();
        }
        let mut weight = |id: &str| freshness.weight(key(&mut self.key, "shot:", id), now);
        let total: f64 = DROP_SHOTS
            .iter()
            .filter(|(id, tier)| shot_allowed(gates, id, tier))
            .map(|(id, _)| weight(id))
            .sum();
        if total <= 0.0 {
            return None;
        }
        let mut roll = unit(&mut self.rng) * total;
        let mut last = None;
        for (id, _) in DROP_SHOTS.iter().filter(|(id, tier)| shot_allowed(gates, id, tier)) {
            last = Some(*id);
            roll -= weight(id);
            if roll <= 0.0 {
                break;
            }
        }
        last.and_then(|id| ShotId::new(id).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_ids_are_valid_slugs() {
        for (id, tier) in DROP_SHOTS {
            assert!(ShotId::new(id).is_ok(), "{id}");
            assert!(crate::model::ShotTier::new(tier).is_ok(), "{tier}");
        }
        assert!(ShotId::new(STUNT_SHOT).is_ok());
        assert!(StuntKind::new(ASCENSION).is_ok());
    }

    #[test]
    fn unit_stays_in_half_open_range() {
        use rand_core::SeedableRng;
        let mut rng = rand_pcg::Pcg32::seed_from_u64(7);
        for _ in 0..10_000 {
            let v = unit(&mut rng);
            assert!((0.0..1.0).contains(&v), "{v}");
        }
    }

    #[test]
    fn key_reuses_the_buffer() {
        let mut buffer = String::new();
        assert_eq!(key(&mut buffer, "shot:", "chase"), "shot:chase");
        assert_eq!(key(&mut buffer, "stunt:", "x"), "stunt:x");
    }
}
