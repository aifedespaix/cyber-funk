//! Le plan de quartiers de la route : quel quartier (centre néon, banlieue, résidentiel) habille chaque rangée, et ce
//! que cela change dans la génération de la ville (`Row::generate`).
//!
//! Tout est pur et sans allocation à la lecture. Le plan est une suite de segments croissants le long de la route
//! (index de rangée croissant = devant la voiture) ; seul [`SceneryConfig::push_district`] l'écrit, et seulement au-delà
//! des rangées déjà générées : le passé ne bouge jamais, donc pas de `replan_all` en régime établi.
//!
//! Le tirage du mélange utilise la graine d'emplacement fournie par l'appelant (celle des immeubles), avec un sel
//! propre : les tirages historiques ne sont pas décalés.

use cf_show::district::{DistrictId, DistrictKind};

use super::config::SceneryConfig;
use crate::facade::{TEMPER_COUNT, temper_weights};
use crate::rng::hash;

/// Marge, en rangées, entre la dernière rangée générée et le début d'un nouveau segment (spec, précision 2).
pub const LEAD_ROWS: i64 = 2;

/// Sel du tirage « ancien ou nouveau » dans une zone de dégradé.
const BLEND_SALT: f64 = 97.0;

/// Un segment : à partir de `start` (index de rangée) la route appartient à `id`, avec un dégradé depuis le segment
/// précédent sur `blend_rows` rangées.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Segment {
    start: i64,
    id: DistrictId,
    blend_rows: u32,
}

/// La suite des quartiers le long de la route. Défaut : un seul segment `Neon` variante 0 partout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistrictPlan {
    /// Croissant en `start` ; le premier commence à `i64::MIN` et n'est jamais retiré.
    segments: Vec<Segment>,
}

impl Default for DistrictPlan {
    fn default() -> Self {
        Self {
            segments: vec![Segment {
                start: i64::MIN,
                id: DistrictId {
                    kind: DistrictKind::Neon,
                    variant: 0,
                },
                blend_rows: 0,
            }],
        }
    }
}

impl DistrictPlan {
    /// Rang, dans la liste, du segment qui couvre la rangée `index`.
    fn segment_of(&self, index: i64) -> usize {
        self.segments.partition_point(|s| s.start <= index).saturating_sub(1)
    }

    /// Le segment du rang `k` (le premier existe toujours : `k` vient de [`Self::segment_of`] ou vaut `k - 1 >= 0`).
    fn segment(&self, k: usize) -> Segment {
        self.segments.get(k).copied().unwrap_or(Segment {
            start: i64::MIN,
            id: DistrictId {
                kind: DistrictKind::Neon,
                variant: 0,
            },
            blend_rows: 0,
        })
    }

    /// Le quartier de la rangée `index`, sans mélange (la cible du segment, dégradé terminé ou non).
    #[must_use]
    pub fn district_at(&self, index: i64) -> DistrictId {
        self.segment(self.segment_of(index)).id
    }

    /// Le quartier d'un emplacement de la rangée `index`. Dans la zone de dégradé d'un segment (`blend_rows` rangées
    /// depuis son début), l'emplacement est « nouveau » avec une probabilité qui monte linéairement de 0 (début) à 1
    /// (fin), tirée par hachage de `slot_seed` : un même emplacement donne toujours le même quartier.
    #[must_use]
    pub fn blend_at(&self, index: i64, slot_seed: f64) -> DistrictId {
        let k = self.segment_of(index);
        let seg = self.segment(k);
        if k > 0 && seg.blend_rows > 0 {
            let rel = index - seg.start;
            if rel < i64::from(seg.blend_rows) {
                let p = rel as f64 / f64::from(seg.blend_rows);
                if hash(slot_seed, BLEND_SALT) >= p {
                    return self.segment(k - 1).id;
                }
            }
        }
        seg.id
    }

    /// Fin du dernier dégradé planifié (index de rangée), `i64::MIN` pour le plan par défaut.
    #[must_use]
    pub fn last_planned_index(&self) -> i64 {
        let last = self.segment(self.segments.len().saturating_sub(1));
        last.start.saturating_add(i64::from(last.blend_rows))
    }

    pub(super) fn push(&mut self, last_generated: i64, from_index: i64, id: DistrictId, blend_rows: u32) -> bool {
        if from_index == i64::MIN || from_index < last_generated.saturating_add(LEAD_ROWS) {
            return false;
        }
        // Les segments annoncés plus loin n'ont encore donné aucune rangée : on les remplace.
        self.segments.retain(|s| s.start < from_index);
        self.segments.push(Segment {
            start: from_index,
            id,
            blend_rows,
        });
        true
    }
}

impl SceneryConfig {
    /// Annonce que la route devient `to` à partir de la rangée `from_index`, avec un dégradé de `blend_rows` rangées.
    /// `last_generated` est l'index de la dernière rangée générée connue. Refuse (`false`, plan intact) un
    /// `from_index` plus proche que [`LEAD_ROWS`] de celle-ci. Ne modifie aucune rangée déjà générée : l'appelant n'a
    /// rien à re-tirer.
    pub fn push_district(&mut self, last_generated: i64, from_index: i64, to: DistrictId, blend_rows: u32) -> bool {
        self.districts.push(last_generated, from_index, to, blend_rows)
    }

    /// Fin du dernier dégradé planifié, `i64::MIN` si le plan est celui par défaut.
    #[must_use]
    pub fn last_planned_index(&self) -> i64 {
        self.districts.last_planned_index()
    }

    /// Active (ou non) les réglages de quartier dans la génération. Faux : comportement historique (Neon partout).
    /// Renvoie vrai si l'état change (à re-tirer comme [`Self::set_quality`]).
    pub fn set_districts_active(&mut self, on: bool) -> bool {
        std::mem::replace(&mut self.districts_active, on) != on
    }

    /// Type de quartier d'un emplacement ; `Neon` quand les quartiers sont inactifs (aucun tirage de plus).
    pub(super) fn kind_at(&self, index: i64, slot_seed: f64) -> DistrictKind {
        if self.districts_active {
            self.districts.blend_at(index, slot_seed).kind
        } else {
            DistrictKind::Neon
        }
    }

    /// Le plan de quartiers.
    #[must_use]
    pub fn districts(&self) -> &DistrictPlan {
        &self.districts
    }

    /// Les quartiers sont-ils appliqués à la génération ?
    #[must_use]
    pub fn districts_active(&self) -> bool {
        self.districts_active
    }
}

/// Ce qu'un type de quartier change dans la ville.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DistrictLook {
    /// Facteur sur la hauteur des immeubles (1 : inchangé).
    pub height_scale: f64,
    /// Part d'emplacements laissés vides en plus de celle de la couronne (0..1).
    pub skip_chance: f64,
    /// Poids de tirage de chaque tempérament (somme 1).
    pub temper_weights: [f64; TEMPER_COUNT],
}

/// Les réglages d'un type de quartier. `Neon` est neutre : hauteur ×1, aucun vide ajouté, poids du tirage historique
/// (il n'est d'ailleurs pas consulté, pour que le chemin historique reste intact).
///
/// Banlieue et résidentiel : réglage initial, à vérifier à l'œil. Banlieue : immeubles à ~70 % de la hauteur, plus
/// aérée, tempéraments calmes et ambre (0 et 3). Résidentiel : ~45 %, encore plus aéré, calmes et glacés (1, 2).
#[must_use]
pub fn district_look(kind: DistrictKind) -> DistrictLook {
    match kind {
        DistrictKind::Neon => DistrictLook {
            height_scale: 1.0,
            skip_chance: 0.0,
            temper_weights: temper_weights(),
        },
        DistrictKind::Suburb => DistrictLook {
            height_scale: 0.7,
            skip_chance: 0.12,
            temper_weights: [0.55, 0.10, 0.05, 0.22, 0.03, 0.0, 0.0, 0.05],
        },
        DistrictKind::Residential => DistrictLook {
            height_scale: 0.45,
            skip_chance: 0.22,
            temper_weights: [0.25, 0.30, 0.20, 0.15, 0.04, 0.0, 0.0, 0.06],
        },
    }
}

/// Le tempérament pour un tirage `t` dans [0, 1) selon des poids (de somme 1).
#[must_use]
pub fn weighted_temper(weights: &[f64; TEMPER_COUNT], t: f64) -> u8 {
    let mut acc = 0.0;
    for (i, w) in weights.iter().enumerate() {
        acc += w;
        if t < acc {
            return i as u8;
        }
    }
    // Arrondi cumulé : le dernier tempérament de poids non nul reçoit le reliquat.
    weights.iter().rposition(|&w| w > 0.0).unwrap_or(0) as u8
}
