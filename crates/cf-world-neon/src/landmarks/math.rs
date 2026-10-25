//! Les petits utilitaires de `landmarks/math.ts`.

/// Borne `x` dans `0..=1`.
#[must_use]
pub fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// `smoothstep` de GLSL, bornes dans l'ordre (`edge0 < edge1`) : 0 avant `edge0`, 1 après `edge1`.
#[must_use]
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp01((x - edge0) / (edge1 - edge0));
    t * t * (3.0 - 2.0 * t)
}

/// Rapproche `current` de `target` indépendamment du nombre d'images par seconde (`damp` de Sillage).
#[must_use]
pub fn damp(current: f32, target: f32, rate: f32, dt: f32) -> f32 {
    current + (target - current) * (1.0 - (-rate * dt).exp())
}

/// Le `hash(seed, salt)` de `random.ts` en `f32` : les voxels de l'arche et les glyphes qui jaillissent en tirent leur
/// place. Même fonction que [`crate::rng::hash`].
#[must_use]
pub fn hash(seed: u32, salt: u32) -> f32 {
    crate::rng::hash(f64::from(seed), f64::from(salt)) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoothstep_borne_et_lisse() {
        assert!(smoothstep(0.0, 1.0, -1.0).abs() < f32::EPSILON);
        assert!((smoothstep(0.0, 1.0, 2.0) - 1.0).abs() < f32::EPSILON);
        assert!((smoothstep(0.0, 1.0, 0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn damp_ne_depend_pas_du_pas() {
        let one = damp(0.0, 1.0, 3.0, 0.1);
        let two = damp(damp(0.0, 1.0, 3.0, 0.05), 1.0, 3.0, 0.05);
        assert!((one - two).abs() < 1e-6);
    }

    #[test]
    fn hash_reste_dans_zero_un() {
        for n in 0..2000 {
            let h = hash(n, 3);
            assert!((0.0..=1.0).contains(&h));
        }
    }
}
