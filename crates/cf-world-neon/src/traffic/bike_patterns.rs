//! Figures des motos (`bike-patterns.ts`), dans le repère de la voiture du joueur : `x` = position latérale sur la
//! route (la chaussée fait ±9 m), `z` = distance DEVANT la voiture, en mètres. Chaque figure garde ses motos à plus de
//! 14 m devant et à des `z` ou des phases assez différents pour qu'elles ne se croisent jamais au même endroit au même
//! moment. L'événement des motos (J7-04/05) les consomme.

use std::f32::consts::TAU;

/// Où se trouve une moto dans la figure.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Slot {
    /// Position latérale, m.
    pub x: f32,
    /// Distance devant la voiture, m.
    pub z: f32,
}

/// Une figure : `t` (s), moto `i` sur `n`.
pub type Pattern = fn(t: f32, i: usize, n: usize) -> Slot;

/// Slalom en file : chacune zigzague, un peu en retard sur la précédente.
fn weave(t: f32, i: usize, _n: usize) -> Slot {
    Slot {
        x: 5.2 * (1.15 * t - i as f32 * 0.8).sin(),
        z: 24.0 + i as f32 * 8.0,
    }
}

/// Tresse : voisines en opposition de phase, elles se croisent à des distances différentes.
fn braid(t: f32, i: usize, _n: usize) -> Slot {
    let fi = i as f32;
    Slot {
        x: 5.0 * (t + fi * std::f32::consts::PI).sin(),
        z: 22.0 + fi * 8.0 + 2.5 * (1.6 * t + fi).sin(),
    }
}

/// Manège : elles tournent sur une ellipse devant la voiture.
fn ring(t: f32, i: usize, n: usize) -> Slot {
    let a = 1.3 * t + (i as f32 * TAU) / n as f32;
    Slot {
        x: 4.8 * a.cos(),
        z: 34.0 + 13.0 * a.sin(),
    }
}

/// Formation en V qui balance d'un bord à l'autre.
fn vee(t: f32, i: usize, n: usize) -> Slot {
    let mid = (n as f32 - 1.0) / 2.0;
    let d = i as f32 - mid;
    Slot {
        x: d * 2.6 + 2.8 * (0.8 * t).sin(),
        z: 22.0 + d.abs() * 7.0,
    }
}

/// Un huit couché : toutes suivent la même courbe, décalées d'un n-ième de tour.
fn eight(t: f32, i: usize, n: usize) -> Slot {
    let a = 0.9 * t + (i as f32 * TAU) / n as f32;
    Slot {
        x: 5.0 * a.sin(),
        z: 30.0 + 10.0 * (2.0 * a).sin(),
    }
}

/// Les cinq figures, dans l'ordre de `PATTERNS`.
pub const PATTERNS: [Pattern; 5] = [weave, braid, ring, vee, eight];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_motos_restent_sur_la_chaussee_et_devant_la_voiture() {
        for (p, pattern) in PATTERNS.iter().enumerate() {
            for n in 2..=5 {
                for step in 0..600 {
                    let t = step as f32 * 0.05;
                    for i in 0..n {
                        let slot = pattern(t, i, n);
                        assert!(slot.x.abs() <= 9.0, "figure {p}, moto {i}/{n}, t={t} : x={}", slot.x);
                        assert!(slot.z >= 14.0, "figure {p}, moto {i}/{n}, t={t} : z={}", slot.z);
                    }
                }
            }
        }
    }

    #[test]
    fn deux_motos_ne_se_posent_jamais_au_meme_point() {
        for (p, pattern) in PATTERNS.iter().enumerate() {
            let count = 5;
            for step in 0..2000 {
                let time = step as f32 * 0.02;
                for i in 0..count {
                    for j in (i + 1)..count {
                        let (first, second) = (pattern(time, i, count), pattern(time, j, count));
                        let gap = (first.x - second.x).hypot(first.z - second.z);
                        assert!(gap > 1.2, "figure {p}, motos {i} et {j} à {gap} m, t={time}");
                    }
                }
            }
        }
    }
}
