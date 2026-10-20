//! Les ponts (`bridges.ts`) : l'emplacement des immeubles-arches sur la route.
//!
//! « Immeubles-arches : un tablier fenêtré qui enjambe la route, posé sur deux piliers de part et d'autre. Rares : une
//! cellule de 1 900 m en contient une, décalée au hasard, soit un passage toutes les 55 à 100 secondes de route. »
//!
//! Ce module ne porte que le **plan** (où sont les arches, quelle place elles prennent) : il est pur, déterministe et
//! sans allocation. Le maillage et le matériau de façade des arches sont de J5-02 ; le pilote (J5-06) et la caméra
//! (J6) lisent le plan pour esquiver et cadrer.

use crate::rng::hash;

/// Longueur d'une cellule : une arche par cellule, en m.
pub const CELL: f64 = 1900.0;
/// Décalage de la première cellule : pas d'arche au départ, en m.
pub const FIRST_OFFSET: f64 = 700.0;
/// Amplitude du décalage aléatoire dans la cellule, en m.
pub const JITTER: f64 = 600.0;
/// Arête extérieure des piliers, en m depuis l'axe.
pub const SPAN_HALF: f64 = 19.5;
/// Largeur d'un pilier, en m.
pub const PILLAR_WIDTH: f64 = 8.5;
/// Altitude du dessous du tablier, en m.
pub const DECK_BOTTOM: f64 = 24.0;
/// Épaisseur du tablier, en m.
pub const DECK_HEIGHT: f64 = 11.0;
/// Profondeur de l'arche le long de la route, en m.
pub const DEPTH: f64 = 16.0;
/// Distance devant la voiture à partir de laquelle une arche est affichée, en m (`VISIBLE` de `bridges.ts`) : la ville
/// la fait « pousser » au loin, dans la brume.
pub const VISIBLE: f64 = 520.0;
/// Distance derrière la voiture jusqu'où une arche reste affichée, en m (le `carZ + 40` de `update`).
pub const KEEP_BEHIND: f64 = 40.0;
/// Arches affichées en même temps (`SLOTS`).
pub const SLOTS: usize = 2;
/// Nombre de teintes du cycle prismatique (`prismaticCycle` : cyan, azur, violet, cyan, rose, glace).
pub const TINT_CYCLE_LEN: usize = 6;

/// Profondeur (z absolu, négatif vers l'avant) de l'arche de la cellule `cell`.
#[must_use]
pub fn bridge_z(cell: i64) -> f64 {
    -(cell as f64 * CELL + FIRST_OFFSET + hash(cell as f64, 41.0) * JITTER)
}

/// Tranche d'altitude du tablier (m) et demi-épaisseur de l'arche le long de la route : pour décider d'une esquive en
/// vol.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BridgeSpan {
    /// Dessous du tablier, m.
    pub lo: f64,
    /// Dessus du tablier, m.
    pub hi: f64,
    /// Demi-profondeur de l'arche le long de la route, m.
    pub half_depth: f64,
}

/// L'emprise verticale d'une arche (`bridgeSpan` de `bridges.ts`).
#[must_use]
pub fn bridge_span() -> BridgeSpan {
    BridgeSpan {
        lo: DECK_BOTTOM,
        hi: DECK_BOTTOM + DECK_HEIGHT,
        half_depth: DEPTH / 2.0,
    }
}

/// Vrai si `z` tombe dans l'emprise d'une arche, marge comprise (la ville s'écarte pour elle).
#[must_use]
pub fn is_near_bridge(z: f64, margin: f64) -> bool {
    let cell = (-z / CELL).floor() as i64;
    (cell - 1..=cell + 1)
        .filter(|&c| c >= 0)
        .any(|c| (z - bridge_z(c)).abs() < DEPTH / 2.0 + margin)
}

/// Une arche à afficher : sa cellule, sa place sur la route et ses tirages (teinte, graine des fenêtres).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bridge {
    /// Cellule de la route (0 = la première).
    pub cell: i64,
    /// Profondeur (z absolu, m).
    pub z: f64,
    /// Rang de la teinte dans le cycle prismatique (`TINT_CYCLE_LEN` teintes).
    pub tint: usize,
    /// Graine des fenêtres, 0 à 999.
    pub seed: u32,
}

impl Bridge {
    /// L'arche de la cellule `cell`.
    #[must_use]
    pub fn of_cell(cell: i64) -> Self {
        let c = cell as f64;
        Self {
            cell,
            z: bridge_z(cell),
            tint: ((hash(c, 42.0) * TINT_CYCLE_LEN as f64).floor() as usize).min(TINT_CYCLE_LEN - 1),
            seed: (hash(c, 43.0) * 1000.0).floor() as u32,
        }
    }
}

/// Les arches à afficher autour de la voiture, par rang croissant de cellule. Tableau de taille fixe : aucune
/// allocation, à recalculer à chaque image sans coût.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BridgeWindow {
    slots: [Option<Bridge>; SLOTS],
}

impl BridgeWindow {
    /// Les arches visibles depuis `car_z` (`update` de `bridges.ts`) : celles des cellules voisines qui ne sont ni à plus
    /// de [`KEEP_BEHIND`] derrière la voiture, ni à plus de [`VISIBLE`] devant. Au plus [`SLOTS`] ; les suivantes sont
    /// ignorées.
    #[must_use]
    pub fn around(car_z: f64) -> Self {
        let cell = ((-car_z / CELL).floor() as i64).max(0);
        let mut window = Self::default();
        let mut next = 0;
        for c in (cell - 1).max(0)..=cell + 1 {
            let z = bridge_z(c);
            if z > car_z + KEEP_BEHIND || z < car_z - VISIBLE {
                continue;
            }
            if let Some(slot) = window.slots.get_mut(next) {
                *slot = Some(Bridge::of_cell(c));
                next += 1;
            }
        }
        window
    }

    /// Les arches, dans l'ordre des emplacements.
    pub fn iter(&self) -> impl Iterator<Item = Bridge> + '_ {
        self.slots.iter().flatten().copied()
    }

    /// Les emplacements, vides compris (un moteur de rendu qui cache les emplacements libres).
    #[must_use]
    pub fn slots(&self) -> &[Option<Bridge>; SLOTS] {
        &self.slots
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_arche_par_cellule_dans_sa_fenetre() {
        for cell in 0..400 {
            let z = bridge_z(cell);
            let from = cell as f64 * CELL + FIRST_OFFSET;
            assert!((-z - from) >= 0.0 && (-z - from) < JITTER, "cellule {cell} : z = {z}");
        }
    }

    #[test]
    fn l_ecart_entre_deux_arches_tient_les_55_a_100_secondes() {
        // « un passage toutes les 55 à 100 secondes de route » : entre 1 300 m et 2 500 m d'écart.
        for cell in 0..400 {
            let gap = bridge_z(cell) - bridge_z(cell + 1);
            assert!((CELL - JITTER..=CELL + JITTER).contains(&gap), "écart {gap}");
        }
    }

    #[test]
    fn proximite_d_une_arche() {
        let z = bridge_z(3);
        assert!(is_near_bridge(z, 0.0));
        assert!(is_near_bridge(z + DEPTH / 2.0 - 0.1, 0.0));
        assert!(!is_near_bridge(z + DEPTH / 2.0 + 0.1, 0.0));
        assert!(is_near_bridge(z + DEPTH / 2.0 + 0.1, 1.0), "la marge élargit l'emprise");
        assert!(!is_near_bridge(0.0, 20.0), "rien au départ");
    }

    #[test]
    fn la_fenetre_suit_la_voiture() {
        let z0 = bridge_z(0);
        // Trop loin devant : pas encore affichée ; à 520 m ou moins devant, elle apparaît.
        assert_eq!(BridgeWindow::around(z0 + VISIBLE + 1.0).iter().count(), 0);
        let near: Vec<_> = BridgeWindow::around(z0 + VISIBLE - 1.0).iter().collect();
        assert_eq!(near.len(), 1);
        assert_eq!(near[0].cell, 0);
        // Dépassée de 40 m ou moins : encore là ; au-delà : disparue.
        assert_eq!(BridgeWindow::around(z0 - KEEP_BEHIND + 1.0).iter().count(), 1);
        assert_eq!(BridgeWindow::around(z0 - KEEP_BEHIND - 1.0).iter().count(), 0);
        // Un départ « avant » le début de la route (z > 0) ne plante pas.
        assert_eq!(BridgeWindow::around(50.0).iter().count(), 0);
    }

    #[test]
    fn tirages_dans_leurs_bornes() {
        for cell in 0..400 {
            let bridge = Bridge::of_cell(cell);
            assert!(bridge.tint < TINT_CYCLE_LEN);
            assert!(bridge.seed < 1000);
        }
    }

    #[test]
    fn l_emprise_verticale_est_celle_de_sillage() {
        let span = bridge_span();
        assert!((span.lo - 24.0).abs() < f64::EPSILON && (span.hi - 35.0).abs() < f64::EPSILON);
        assert!((span.half_depth - 8.0).abs() < f64::EPSILON);
    }

    /// Les positions, tirages et emprises de `bridges.ts`, calculés par le fichier d'origine
    /// (`tools/sillage-golden/road`).
    #[test]
    fn les_arches_comme_sillage() {
        let golden: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/golden/route.json")).expect("route.json valide");
        let bridges = &golden["bridges"];
        for entry in bridges["cells"].as_array().expect("liste") {
            let cell = entry["cell"].as_i64().expect("entier");
            let bridge = Bridge::of_cell(cell);
            let z = entry["z"].as_f64().expect("nombre");
            // `sin` en `f64` : l'écart à Sillage reste très en deçà du millimètre.
            assert!((bridge.z - z).abs() < 1e-6, "cellule {cell} : {} contre {z}", bridge.z);
            assert_eq!(
                i64::try_from(bridge.tint).expect("petit"),
                entry["tint"].as_i64().expect("entier"),
                "teinte {cell}"
            );
            assert_eq!(
                i64::from(bridge.seed),
                entry["seed"].as_i64().expect("entier"),
                "graine {cell}"
            );
        }
        for entry in bridges["near"].as_array().expect("liste") {
            let z = entry[0].as_f64().expect("nombre");
            let margin = entry[1].as_f64().expect("nombre");
            assert_eq!(
                is_near_bridge(z, margin),
                entry[2].as_bool().expect("booléen"),
                "z = {z}, marge {margin}"
            );
        }
        let span = bridge_span();
        let expected = &bridges["span"];
        assert!((span.lo - expected["lo"].as_f64().expect("nombre")).abs() < 1e-12);
        assert!((span.hi - expected["hi"].as_f64().expect("nombre")).abs() < 1e-12);
        assert!((span.half_depth - expected["halfDepth"].as_f64().expect("nombre")).abs() < 1e-12);
    }
}
