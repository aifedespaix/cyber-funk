//! La fenêtre de tronçons : quels tronçons vivent autour du point suivi, et dans quel emplacement du pool.
//!
//! Logique pure (sans Bevy). Le tronçon d'index `i` occupe toujours l'emplacement `i mod N` : quand la fenêtre avance
//! d'un tronçon, celui qui sort derrière (`first`) cède son emplacement à celui qui entre devant (`first + N`). C'est
//! l'anneau de `scenery.ts` (« tout est recalculé à partir de l'index de rangée »), généralisé à n'importe quel monde.

use bevy::math::DVec3;

/// Géométrie du streaming d'un monde. Lue une fois, au démarrage.
///
/// Repère de Sillage : on avance vers −z. Le tronçon d'index `i` couvre z ∈ [−(i+1)·L, −i·L] (absolu, m).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamLayout {
    /// Longueur d'un tronçon le long de la route, en m (> 0).
    pub chunk_length_m: f32,
    /// Tronçons gardés derrière celui du point suivi.
    pub behind: u32,
    /// Tronçons gardés devant celui du point suivi.
    pub ahead: u32,
    /// Graine du monde : avec l'index, elle détermine tout le contenu d'un tronçon.
    pub seed: u64,
}

impl StreamLayout {
    /// Plus grande fenêtre acceptée : au-delà, c'est presque sûrement une erreur de réglage.
    pub const MAX_CHUNKS: u32 = 4_096;

    /// Nombre de tronçons vivants : derrière + celui du point suivi + devant.
    #[must_use]
    pub fn count(&self) -> u32 {
        self.behind.saturating_add(1).saturating_add(self.ahead)
    }

    /// La géométrie est-elle utilisable ? (longueur finie et positive, fenêtre raisonnable)
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.chunk_length_m.is_finite() && self.chunk_length_m > 0.0 && self.count() <= Self::MAX_CHUNKS
    }

    /// Index du tronçon qui contient la distance `distance_m` parcourue le long de la route (`distance = −z`).
    #[must_use]
    pub fn chunk_at(&self, distance_m: f64) -> i64 {
        (distance_m / f64::from(self.chunk_length_m)).floor() as i64
    }

    /// Emplacement du pool occupé par le tronçon `index`.
    #[must_use]
    pub fn slot_of(&self, index: i64) -> usize {
        index.rem_euclid(i64::from(self.count().max(1))) as usize
    }

    /// Ce qu'un monde reçoit pour remplir le tronçon `index`.
    #[must_use]
    pub fn chunk(&self, index: i64) -> ChunkInfo {
        ChunkInfo {
            index,
            seed: chunk_seed(self.seed, index),
            anchor: DVec3::new(0.0, 0.0, -(index as f64) * f64::from(self.chunk_length_m)),
            length_m: self.chunk_length_m,
        }
    }
}

/// Un tronçon à remplir : son index, sa graine, son ancrage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChunkInfo {
    /// Rang du tronçon le long de la route (0 au départ, croît vers −z, négatif derrière le départ).
    pub index: i64,
    /// Graine déterministe du tronçon (`chunk_seed(graine du monde, index)`).
    pub seed: u64,
    /// Position absolue (m) de l'entrée du tronçon, `(0, 0, −index·L)`. Les pièces sont placées relativement à elle :
    /// en repère local, le tronçon couvre z ∈ [−L, 0].
    pub anchor: DVec3,
    /// Longueur du tronçon, en m.
    pub length_m: f32,
}

/// Graine d'un tronçon : mélange de la graine du monde et de l'index (`SplitMix64`), stable d'une plateforme à l'autre.
#[must_use]
pub fn chunk_seed(world_seed: u64, index: i64) -> u64 {
    splitmix64(world_seed ^ splitmix64(index as u64))
}

/// Mélangeur `SplitMix64` : une entrée, une sortie bien répartie sur 64 bits.
#[must_use]
pub fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Les tronçons vivants : `[first, first + count)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChunkWindow {
    layout: StreamLayout,
    first: Option<i64>,
}

impl ChunkWindow {
    /// Fenêtre vide : la première mise à jour assigne tous les emplacements.
    #[must_use]
    pub fn new(layout: StreamLayout) -> Self {
        Self { layout, first: None }
    }

    /// La géométrie de la fenêtre.
    #[must_use]
    pub fn layout(&self) -> &StreamLayout {
        &self.layout
    }

    /// Premier index vivant, `None` avant la première mise à jour.
    #[must_use]
    pub fn first(&self) -> Option<i64> {
        self.first
    }

    /// Recale la fenêtre sur la distance parcourue par le point suivi (`distance = −z`, m) et appelle
    /// `assign(emplacement, index)` pour chaque tronçon qui vient d'y entrer. Renvoie le nombre de tronçons assignés.
    ///
    /// Le travail est borné par la taille de la fenêtre, même après un saut de 400 km (reprise, téléportation).
    pub fn update(&mut self, distance_m: f64, mut assign: impl FnMut(usize, i64)) -> u32 {
        let count = i64::from(self.layout.count());
        let first = self
            .layout
            .chunk_at(distance_m)
            .saturating_sub(i64::from(self.layout.behind));
        let entering = match self.first {
            Some(old) if old == first => return 0,
            // Petit pas en avant : entrent les index au-delà de l'ancien bord avant.
            Some(old) if first > old && first - old < count => (old + count)..(first + count),
            // Petit pas en arrière : entrent les index en deçà de l'ancien bord arrière.
            Some(old) if first < old && old - first < count => first..old,
            // Premier remplissage ou saut plus grand que la fenêtre : tout est réassigné.
            _ => first..(first + count),
        };
        self.first = Some(first);
        let mut assigned = 0;
        for index in entering {
            assign(self.layout.slot_of(index), index);
            assigned += 1;
        }
        assigned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> StreamLayout {
        StreamLayout {
            chunk_length_m: 26.0,
            behind: 15,
            ahead: 18,
            seed: 7,
        }
    }

    #[test]
    fn la_fenetre_de_sillage_compte_34_troncons() {
        let layout = layout();
        assert_eq!(layout.count(), 34);
        // 15 rangées derrière (390 m) et 19 devant en comptant celle du point suivi (494 m) : `scenery.ts`.
        assert!((f64::from(layout.behind) * 26.0 - 390.0).abs() < f64::EPSILON);
    }

    #[test]
    fn premier_remplissage_puis_recyclage_un_par_un() {
        let mut window = ChunkWindow::new(layout());
        let mut seen = Vec::new();
        assert_eq!(window.update(0.0, |slot, index| seen.push((slot, index))), 34);
        assert_eq!(seen.first(), Some(&(layout().slot_of(-15), -15)));
        seen.clear();
        // Même tronçon : rien.
        assert_eq!(window.update(25.0, |_, _| unreachable!()), 0);
        // Un tronçon plus loin : le tronçon −15 sort, le 19 entre dans son emplacement.
        assert_eq!(window.update(26.5, |slot, index| seen.push((slot, index))), 1);
        assert_eq!(seen, vec![(layout().slot_of(-15), 19)]);
    }

    #[test]
    fn marche_arriere_et_saut() {
        let mut window = ChunkWindow::new(layout());
        window.update(1_000.0, |_, _| {});
        let mut entered = Vec::new();
        assert_eq!(window.update(1_000.0 - 3.0 * 26.0, |_, index| entered.push(index)), 3);
        let first = window.first().expect("fenêtre remplie");
        assert_eq!(entered, vec![first, first + 1, first + 2]);
        // Saut de 400 km : tout est réassigné, une seule fois chacun.
        let mut slots = [0_u32; 34];
        assert_eq!(window.update(401_000.0, |slot, _| slots[slot] += 1), 34);
        assert!(slots.iter().all(|&n| n == 1));
    }

    #[test]
    fn un_index_par_emplacement() {
        // Propriété de l'anneau : à tout instant, les index vivants occupent des emplacements distincts.
        let layout = layout();
        let mut window = ChunkWindow::new(layout);
        let mut live = vec![i64::MIN; 34];
        let mut distance = -500.0;
        for step in 0..5_000 {
            distance += if step % 7 == 0 { -11.0 } else { 23.7 };
            window.update(distance, |slot, index| live[slot] = index);
            let first = window.first().expect("fenêtre remplie");
            let mut sorted = live.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, (first..first + 34).collect::<Vec<_>>());
        }
    }

    #[test]
    fn graines_stables_et_distinctes() {
        assert_eq!(chunk_seed(7, 12), chunk_seed(7, 12));
        assert_ne!(chunk_seed(7, 12), chunk_seed(7, 13));
        assert_ne!(chunk_seed(7, 12), chunk_seed(8, 12));
        let chunk = layout().chunk(10);
        assert!((chunk.anchor.z + 260.0).abs() < f64::EPSILON);
    }
}
