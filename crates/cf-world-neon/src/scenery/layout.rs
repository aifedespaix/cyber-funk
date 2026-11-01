//! La ville d'une rangée : quels immeubles, où, de quelle taille (`Scenery.layout` de `scenery.ts`).
//!
//! Tout est une fonction pure de l'index de rangée : « le décor est infini et ne bouge que lorsqu'on franchit une
//! rangée ». Même index, même ville, sur toutes les images et toutes les machines (à l'ulp de `sin` près, voir
//! [`crate::rng::hash`]). Aucune allocation : une [`Row`] est un bloc de tableaux de taille fixe.
//!
//! Le monde ne passe pas sa graine ici : Sillage tire la ville de l'index de rangée seul, et les données de référence
//! en dépendent (voir `tests/golden/scenery.json`). Deux mondes de graines différentes ont donc la même ville ; ce
//! qui change d'un monde à l'autre, ce sont les événements, pas les immeubles.

use super::config::SceneryConfig;
use crate::facade::temper_of;
use crate::rng::hash;
use crate::route::is_near_bridge;

/// Distance entre deux rangées, en m (`SPACING`). Égale à la longueur d'un tronçon du streaming.
pub const SPACING_M: f64 = 26.0;
/// Hauteur d'un étage, en m (`FLOOR_HEIGHT` de `building-material.ts`).
pub const FLOOR_HEIGHT_M: f32 = 2.4;
/// Les tours plongent sous la route jusqu'ici, en m (`BASE_DEPTH`) : la nappe de brouillard les avale bien avant.
pub const BASE_DEPTH_M: f32 = FLOOR_HEIGHT_M * 160.0;
/// Emplacements par rangée : six couronnes, chacune de chaque côté de la route.
pub const SLOTS: usize = LAYERS.len() * 2;
/// Pièces par emplacement : corps, couronne, antenne.
pub const PARTS: usize = 3;
/// Pièces au plus par rangée.
pub const MAX_PARTS: usize = SLOTS * PARTS;
/// Boîtes de collision au plus par rangée : le corps et la couronne de chaque tour.
pub const MAX_HITBOXES: usize = SLOTS * 2;
/// Nombre de teintes du cycle prismatique.
pub const TINT_CYCLE_LEN: usize = crate::route::bridges::TINT_CYCLE_LEN;

/// Couronnes de premier plan : trapues, collées à la route, et deux couronnes de tours plus hautes.
const NEAR_LAYERS: usize = 3;
/// Marge de recherche des collisions autour d'une rangée : décalage aléatoire (3 m) + demi-profondeur maximale (14 m).
const ROW_REACH_M: f64 = 17.0;

/// Une couronne d'immeubles : où elle commence, ses plages de tailles, la part d'emplacements vides.
struct Layer {
    /// Distance à l'axe de la route du bord intérieur, en m.
    x: f64,
    /// Étalement aléatoire du bord intérieur, en m.
    spread: f64,
    width: [f64; 2],
    depth: [f64; 2],
    height: [f64; 2],
    /// Part d'emplacements laissés vides.
    skip: f64,
    /// Part de tours qui portent un projecteur.
    spots: f64,
    /// Couronne d'arrière-plan : un corps seul, sans couronne, ombrage simplifié.
    far: bool,
}

const LAYERS: [Layer; 6] = [
    // Premier plan : trapus, collés à la route. Puis deux couronnes de tours plus hautes.
    Layer {
        x: 13.5,
        spread: 3.0,
        width: [8.0, 13.0],
        depth: [8.0, 12.0],
        height: [9.0, 28.0],
        skip: 0.22,
        spots: 0.55,
        far: false,
    },
    Layer {
        x: 34.0,
        spread: 6.0,
        width: [10.0, 18.0],
        depth: [10.0, 16.0],
        height: [20.0, 50.0],
        skip: 0.25,
        spots: 0.35,
        far: false,
    },
    Layer {
        x: 58.0,
        spread: 10.0,
        width: [12.0, 22.0],
        depth: [12.0, 20.0],
        height: [38.0, 85.0],
        skip: 0.1,
        spots: 0.0,
        far: false,
    },
    // Arrière-plan : trois couronnes de plus en plus hautes et larges, qui dessinent la ligne d'horizon de la ville.
    // Elles sont presque noyées dans la brume : un corps seul, sans couronne, avec un ombrage simplifié (tier 3).
    Layer {
        x: 98.0,
        spread: 14.0,
        width: [16.0, 30.0],
        depth: [16.0, 24.0],
        height: [70.0, 140.0],
        skip: 0.12,
        spots: 0.0,
        far: true,
    },
    Layer {
        x: 150.0,
        spread: 20.0,
        width: [20.0, 36.0],
        depth: [18.0, 26.0],
        height: [100.0, 200.0],
        skip: 0.1,
        spots: 0.0,
        far: true,
    },
    Layer {
        x: 214.0,
        spread: 30.0,
        width: [26.0, 46.0],
        depth: [20.0, 28.0],
        height: [140.0, 280.0],
        skip: 0.1,
        spots: 0.0,
        far: true,
    },
];

/// Ce qu'une pièce est dans le matériau de façade (`aInfo.z` de Sillage).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tier {
    /// Corps d'une tour proche : fenêtres une à une, ruissellement, arêtes.
    #[default]
    Body = 0,
    /// Couronne en retrait : prolonge les étages du corps.
    Crown = 1,
    /// Antenne : un trait dont la pointe bat avec le kick.
    Antenna = 2,
    /// Tour d'arrière-plan : plages de fenêtres, ni ruissellement, ni arêtes.
    Far = 3,
}

impl Tier {
    /// Le type lu dans le tag d'une pièce (`None` hors 0..=3, jamais produit par [`Part::tag`]).
    #[must_use]
    pub fn from_bits(bits: u32) -> Self {
        match bits & 3 {
            0 => Self::Body,
            1 => Self::Crown,
            2 => Self::Antenna,
            _ => Self::Far,
        }
    }
}

/// Une pièce : un pavé posé sur la route. Les positions sont celles de l'instance de Sillage.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Part {
    /// Type de pièce.
    pub tier: Tier,
    /// Centre du pavé en x (axe de la route = 0), en m.
    pub x: f32,
    /// Altitude de la base, en m (négative pour un corps qui plonge sous la route).
    pub y: f32,
    /// Position le long de la route **relative à la rangée** (`−r × SPACING` en absolu), en m : le décalage aléatoire.
    pub z: f32,
    /// Largeur, hauteur, profondeur, en m.
    pub size: [f32; 3],
    /// Rang de la teinte dans le cycle prismatique (`0..TINT_CYCLE_LEN`).
    pub tint: u8,
    /// Graine des fenêtres (`0..1010`), `seed01` de Sillage.
    pub seed: u16,
    /// Étage de la ceinture lumineuse + 1 (0 : aucune), l'étage des projecteurs de Sillage quantifié.
    pub spot_floor: u8,
    /// Indice du tempérament de l'immeuble (`0..TEMPER_COUNT`) ; 0 : le rendu d'avant les tempéraments (arches comprises).
    pub temper: u8,
}

impl Part {
    /// Le tag envoyé au shader (`MeshTag`) : graine (10 bits), teinte (4), type (2), étage du projecteur + 1 (8),
    /// tempérament (3, bits 24..26).
    #[must_use]
    pub fn tag(&self) -> u32 {
        u32::from(self.seed) & 0x3FF
            | (u32::from(self.tint) & 0xF) << 10
            | (self.tier as u32) << 14
            | u32::from(self.spot_floor) << 16
            | (u32::from(self.temper) & 0x7) << 24
    }
}

/// Une boîte de collision : l'emprise d'une tour (corps ou couronne) et son sommet. Positions absolues.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Hitbox {
    /// x min, x max, en m.
    pub x: [f32; 2],
    /// z min, z max, en m (absolu).
    pub z: [f64; 2],
    /// Altitude du sommet, en m.
    pub top: f32,
}

/// Le point d'attache d'un projecteur (`SpotAnchor` de `beams.ts`) : sur la façade d'une tour, côté route.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpotAnchor {
    /// Position en x (axe de la route = 0), en m.
    pub x: f32,
    /// Altitude, en m.
    pub y: f32,
    /// Position le long de la route, en m (absolu).
    pub z: f64,
    /// −1 : tour de gauche, 1 : tour de droite.
    pub side: i8,
    /// Phase du balayage, en rad.
    pub phase: f32,
    /// Rang de la teinte du faisceau dans le cycle prismatique.
    pub tint: u8,
}

/// La ville d'une rangée. Tableaux de taille fixe : en générer une n'alloue pas.
#[derive(Debug, Clone)]
pub struct Row {
    /// Index de la rangée : son ancrage est à `z = −index × SPACING`.
    pub index: i64,
    parts: [Part; MAX_PARTS],
    part_count: usize,
    hitboxes: [Hitbox; MAX_HITBOXES],
    hitbox_count: usize,
    spots: [SpotAnchor; SLOTS],
    spot_count: usize,
}

impl Row {
    /// Les pièces à afficher.
    #[must_use]
    pub fn parts(&self) -> &[Part] {
        &self.parts[..self.part_count]
    }

    /// Les boîtes de collision.
    #[must_use]
    pub fn hitboxes(&self) -> &[Hitbox] {
        &self.hitboxes[..self.hitbox_count]
    }

    /// Les points d'attache des projecteurs.
    #[must_use]
    pub fn spots(&self) -> &[SpotAnchor] {
        &self.spots[..self.spot_count]
    }

    fn push_part(&mut self, part: Part) {
        // `MAX_PARTS` = emplacements × pièces : une tour n'en a jamais plus, la garde ne sert qu'à ne pas paniquer.
        if let Some(slot) = self.parts.get_mut(self.part_count) {
            *slot = part;
            self.part_count += 1;
        }
    }

    fn push_hitbox(&mut self, x: f64, z: f64, w: f64, d: f64, top: f64) {
        if let Some(slot) = self.hitboxes.get_mut(self.hitbox_count) {
            *slot = Hitbox {
                x: [(x - w / 2.0) as f32, (x + w / 2.0) as f32],
                z: [z - d / 2.0, z + d / 2.0],
                top: top as f32,
            };
            self.hitbox_count += 1;
        }
    }

    /// Génère la ville de la rangée `index` (`layout` de `scenery.ts`, pour une rangée).
    #[must_use]
    pub fn generate(index: i64, config: &SceneryConfig) -> Self {
        let mut row = Self {
            index,
            parts: [Part::default(); MAX_PARTS],
            part_count: 0,
            hitboxes: [Hitbox::default(); MAX_HITBOXES],
            hitbox_count: 0,
            spots: [SpotAnchor::default(); SLOTS],
            spot_count: 0,
        };
        for slot in 0..SLOTS {
            row.generate_slot(slot, config);
        }
        row
    }

    #[allow(clippy::too_many_lines)] // une seule fonction, comme `layout` de Sillage : les tirages se lisent de haut en bas
    fn generate_slot(&mut self, slot: usize, config: &SceneryConfig) {
        let layer_index = slot >> 1;
        let layer = &LAYERS[layer_index];
        let layer_on = layer_index < NEAR_LAYERS || layer_index - NEAR_LAYERS < config.far_layers();
        let side: i8 = if slot.is_multiple_of(2) { -1 } else { 1 };
        let seed = self.index as f64 * 13.37 + slot as f64 * 71.1;

        // La ville s'écarte devant le pied d'une arche.
        let row_z = -(self.index as f64) * SPACING_M;
        if !layer_on
            || hash(seed, 1.0) < layer.skip
            || (layer_index == 0 && is_near_bridge(row_z, 12.0))
            || config.cleared(row_z, side)
        {
            return;
        }

        let w = lerp(layer.width, hash(seed, 2.0));
        let d = lerp(layer.depth, hash(seed, 3.0));
        let h = lerp(layer.height, hash(seed, 4.0).powf(1.4));
        let center_x = f64::from(side) * (layer.x + hash(seed, 5.0) * layer.spread + w / 2.0);
        let jitter = (hash(seed, 6.0) - 0.5) * 6.0;
        let z = row_z + jitter;
        let tint = cycle_index(hash(seed, 7.0));
        let seed01 = (hash(seed, 8.0) * 1000.0).floor() as u16;

        // Étage des projecteurs, quantifié sur la grille d'étages.
        let mut spot_floor = 0;
        if hash(seed, 9.0) < layer.spots {
            let floor = ((0.45 + hash(seed, 10.0) * 0.45) * (h / f64::from(FLOOR_HEIGHT_M))).floor();
            spot_floor = (floor as u8).saturating_add(1);
            if let Some(spot) = self.spots.get_mut(self.spot_count) {
                *spot = SpotAnchor {
                    x: (center_x - f64::from(side) * (w / 2.0)) as f32,
                    y: ((floor + 0.5) * f64::from(FLOOR_HEIGHT_M)) as f32,
                    z,
                    side,
                    phase: (hash(seed, 11.0) * std::f64::consts::TAU) as f32,
                    tint: cycle_index(hash(seed, 12.0)),
                };
                self.spot_count += 1;
            }
        }

        // Un seul tempérament par immeuble : le corps, la couronne et l'antenne partagent la même façade.
        let temper = temper_of(self.index, seed01);
        let part = |tier, y: f64, size: [f64; 3], seed: u16, spot_floor| Part {
            tier,
            x: center_x as f32,
            y: y as f32,
            z: jitter as f32,
            size: size.map(|s| s as f32),
            tint,
            seed,
            spot_floor,
            temper,
        };
        let antenna = |y: f64, width: f64, height: f64| part(Tier::Antenna, y, [width, height, width], seed01, 0);

        self.push_hitbox(center_x, z, w, d, h);
        // Le corps plonge sous la route : de la route on ne voit jamais son pied.
        let base = f64::from(BASE_DEPTH_M);
        let body_tier = if layer.far { Tier::Far } else { Tier::Body };
        self.push_part(part(body_tier, -base, [w, h + base, d], seed01, spot_floor));

        if layer.far {
            // Pas de couronne au loin ; une antenne de temps en temps pour piquer la ligne d'horizon.
            if hash(seed, 17.0) < 0.3 {
                self.push_part(antenna(h, 1.2, 14.0 + hash(seed, 18.0) * 26.0));
            }
            return;
        }

        // Couronne en retrait sur une tour sur deux ; antenne sur certaines.
        if hash(seed, 13.0) < 0.6 && h > 22.0 {
            let ch = h * (0.14 + hash(seed, 14.0) * 0.18);
            self.push_part(part(Tier::Crown, h, [w * 0.62, ch, d * 0.62], seed01 + 3, 0));
            self.push_hitbox(center_x, z, w * 0.62, d * 0.62, h + ch);
            if hash(seed, 15.0) < 0.55 {
                self.push_part(antenna(h + ch, 0.6, 12.0 + hash(seed, 16.0) * 16.0));
            }
        } else if hash(seed, 17.0) < 0.35 {
            self.push_part(antenna(h, 0.6, 8.0 + hash(seed, 18.0) * 10.0));
        }
    }

    /// Les rangées dont les tours peuvent toucher la sphère de centre z `z` et de rayon `reach_m` : de la première à la
    /// dernière, bornes comprises.
    #[must_use]
    pub fn rows_near(z: f64, reach_m: f64) -> std::ops::RangeInclusive<i64> {
        let margin = ROW_REACH_M + reach_m.max(0.0);
        let first = ((-z - margin) / SPACING_M).floor() as i64;
        let last = ((-z + margin) / SPACING_M).ceil() as i64;
        first..=last
    }
}

fn lerp(range: [f64; 2], t: f64) -> f64 {
    range[0] + (range[1] - range[0]) * t
}

/// Rang dans le cycle prismatique d'un tirage de 0 à 1 (`cycle[floor(hash × cycle.length)]`).
fn cycle_index(t: f64) -> u8 {
    ((t * TINT_CYCLE_LEN as f64).floor() as usize).min(TINT_CYCLE_LEN - 1) as u8
}
