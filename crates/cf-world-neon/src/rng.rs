//! Hasard déterministe de Sillage (`random.ts`) : même graine, même résultat, sur toutes les images et toutes les
//! machines (à l'ulp de `sin` près).

/// Hash déterministe dans [0, 1) : repris tel quel de `hash(seed, salt)` de `random.ts`.
///
/// Le `sin` de Sillage est gardé (et non remplacé par un mélangeur entier) pour que les positions tirées par les
/// graines de Sillage, ponts en tête, retombent aux mêmes endroits : les données de référence en dépendent.
#[must_use]
pub fn hash(seed: f64, salt: f64) -> f64 {
    let x = (seed * 127.1 + salt * 311.7).sin() * 43_758.545_3;
    x - x.floor()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dans_zero_un_et_deterministe() {
        for seed in 0..500 {
            for salt in [41.0, 42.0, 43.0] {
                let h = hash(f64::from(seed), salt);
                assert!((0.0..1.0).contains(&h), "{h}");
                assert!((h - hash(f64::from(seed), salt)).abs() < f64::EPSILON);
            }
        }
    }
}
