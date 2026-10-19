//! Surcharges d'un look par un cue : on réutilise « La ville qui monte » en changeant juste l'allure ou la palette.
//!
//! Chaque champ absent du fichier garde la valeur du look. Les surcharges sont appliquées une fois, quand le cue est
//! préparé (jamais par image).

use serde::{Deserialize, Serialize};

use super::ids::{EventName, ShotId, ShotTier, StuntKind, StuntTier};
use super::look::{CutGrid, LayerMix, Look, PaletteRef};

/// Ce qu'un cue change au look qu'il référence. Tout est facultatif ; vide = le look tel quel.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookOverrides {
    /// Remplace les opacités des couches (les quatre ensemble).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layers: Option<LayerMix>,
    /// Remplace la source de palette.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<PaletteRef>,
    /// Remplace la rotation de teinte, en degrés, -180 à 180.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hue_shift_deg: Option<f32>,
    /// Remplace des portes, champ par champ.
    #[serde(default, skip_serializing_if = "GateOverrides::is_empty")]
    pub gates: GateOverrides,
}

impl LookOverrides {
    /// Aucune surcharge.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.layers.is_none() && self.palette.is_none() && self.hue_shift_deg.is_none() && self.gates.is_empty()
    }

    /// Le look avec ces surcharges appliquées (copie : à faire à la préparation du cue, pas par image).
    #[must_use]
    pub fn apply(&self, look: &Look) -> Look {
        let mut out = look.clone();
        if let Some(layers) = self.layers {
            out.layers = layers;
        }
        if let Some(palette) = &self.palette {
            out.palette = palette.clone();
        }
        if let Some(hue) = self.hue_shift_deg {
            out.hue_shift_deg = hue;
        }
        self.gates.apply(&mut out);
        out
    }
}

/// Surcharges des portes, champ par champ ; mêmes noms, unités et plages que [`super::look::ShowGates`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateOverrides {
    /// Étage de vol le plus haut permis, 0 à 6.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_floor: Option<u8>,
    /// Multiplicateur de la fréquence des envols, 0 à 4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flight_rate: Option<f32>,
    /// Les envols n'ont lieu que sur un drop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flight_on_drop_only: Option<bool>,
    /// Figures en vol permises.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tricks: Option<bool>,
    /// Rangs de cascades permis (remplace toute la liste).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stunt_tiers: Option<Vec<StuntTier>>,
    /// Liste blanche des cascades (remplace toute la liste).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stunt_kinds: Option<Vec<StuntKind>>,
    /// Multiplicateur de l'attente entre deux cascades, 0,1 à 4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stunt_gap_scale: Option<f32>,
    /// Rangs de plans permis (remplace toute la liste).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shot_tiers: Option<Vec<ShotTier>>,
    /// Plans interdits (remplace toute la liste).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shot_block: Option<Vec<ShotId>>,
    /// Grille de coupe, en temps (les trois sections ensemble).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_grid: Option<CutGrid>,
    /// Salves de coupes rapides sur les drops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub salvo: Option<bool>,
    /// Liste blanche des événements du trafic (remplace toute la liste).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<EventName>>,
    /// Multiplicateur de la fréquence des événements, 0 à 4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_rate: Option<f32>,
    /// Lancers d'astéroïdes par minute, 0 à 60.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meteor_rate: Option<f32>,
    /// Pluies d'astéroïdes permises.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meteor_shower: Option<bool>,
    /// Multiplicateur d'allure, 0,5 à 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cruise: Option<f32>,
    /// Densité du trafic au sol, 0 à 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground_traffic: Option<f32>,
    /// Effets, champ par champ.
    #[serde(default, skip_serializing_if = "FxOverrides::is_empty")]
    pub fx: FxOverrides,
    /// L'Ascension est permise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ascension: Option<bool>,
}

impl GateOverrides {
    /// Aucune surcharge de porte.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn apply(&self, look: &mut Look) {
        let g = &mut look.gates;
        set(&mut g.max_floor, self.max_floor);
        set(&mut g.flight_rate, self.flight_rate);
        set(&mut g.flight_on_drop_only, self.flight_on_drop_only);
        set(&mut g.tricks, self.tricks);
        set_list(&mut g.stunt_tiers, self.stunt_tiers.as_ref());
        set_list(&mut g.stunt_kinds, self.stunt_kinds.as_ref());
        set(&mut g.stunt_gap_scale, self.stunt_gap_scale);
        set_list(&mut g.shot_tiers, self.shot_tiers.as_ref());
        set_list(&mut g.shot_block, self.shot_block.as_ref());
        set(&mut g.cut_grid, self.cut_grid);
        set(&mut g.salvo, self.salvo);
        set_list(&mut g.events, self.events.as_ref());
        set(&mut g.event_rate, self.event_rate);
        set(&mut g.meteor_rate, self.meteor_rate);
        set(&mut g.meteor_shower, self.meteor_shower);
        set(&mut g.cruise, self.cruise);
        set(&mut g.ground_traffic, self.ground_traffic);
        set(&mut g.fx.rgb_split, self.fx.rgb_split);
        set(&mut g.fx.flash, self.fx.flash);
        set(&mut g.fx.city_pulse, self.fx.city_pulse);
        set(&mut g.ascension, self.ascension);
    }
}

/// Surcharges des effets, champ par champ ; chacun de 0 à 1.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FxOverrides {
    /// Décalage RVB, 0 à 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rgb_split: Option<f32>,
    /// Flashs, 0 à 1 (plafonnés ensuite par le limiteur).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flash: Option<f32>,
    /// Pulsation de la ville, 0 à 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city_pulse: Option<f32>,
}

impl FxOverrides {
    /// Aucune surcharge d'effet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rgb_split.is_none() && self.flash.is_none() && self.city_pulse.is_none()
    }
}

fn set<T>(slot: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *slot = value;
    }
}

fn set_list<T: Clone>(slot: &mut Vec<T>, value: Option<&Vec<T>>) {
    if let Some(value) = value {
        slot.clone_from(value);
    }
}
