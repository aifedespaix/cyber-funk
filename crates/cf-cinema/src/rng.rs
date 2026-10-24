//! Aléatoire à graine du réalisateur (`cinema/rng.ts`) : permet de rejouer une séquence à l'identique.

/// mulberry32 : petit générateur à graine, suffisant pour tirer des variantes de plans. Même suite que Sillage pour une
/// même graine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CinemaRng {
    state: u32,
}

impl CinemaRng {
    /// Générateur de graine `seed`.
    #[must_use]
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// Prochain tirage, 0 (inclus) → 1 (exclu).
    pub fn next_f32(&mut self) -> f32 {
        self.next_f64() as f32
    }

    /// Prochain tirage en `f64`, 0 (inclus) → 1 (exclu) : la valeur exacte de Sillage.
    pub fn next_f64(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x6d2b_79f5);
        let mut t = self.state;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 0x3d));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }

    /// Un nombre entre `a` et `b`.
    pub fn between(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.next_f32()
    }

    /// Un côté au hasard : −1 (gauche) ou 1 (droite).
    pub fn coin(&mut self) -> f32 {
        if self.next_f64() < 0.5 { -1.0 } else { 1.0 }
    }

    /// Un élément au hasard (`None` si la liste est vide).
    pub fn pick<'a, T>(&mut self, list: &'a [T]) -> Option<&'a T> {
        if list.is_empty() {
            return None;
        }
        let index = ((self.next_f64() * list.len() as f64) as usize).min(list.len() - 1);
        list.get(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Valeurs de `mulberry32(42)` relevées dans Sillage (Bun) : `[0.6011037519201636, 0.44829055899754167,
    /// 0.8524657934904099]`.
    #[test]
    fn meme_suite_que_sillage() {
        let mut rng = CinemaRng::new(42);
        let expected = [
            0.601_103_751_920_163_6,
            0.448_290_558_997_541_67,
            0.852_465_793_490_409_9,
        ];
        for e in expected {
            assert!((rng.next_f64() - e).abs() < 1e-15);
        }
    }

    #[test]
    fn pick_reste_dans_la_liste() {
        let mut rng = CinemaRng::new(7);
        let list = [1, 2, 3];
        for _ in 0..100 {
            assert!(rng.pick(&list).is_some());
        }
        assert!(rng.pick::<u8>(&[]).is_none());
    }
}
