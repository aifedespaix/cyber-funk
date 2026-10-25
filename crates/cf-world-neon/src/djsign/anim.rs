//! Les courbes d'animation de l'enseigne du DJ (`dj-sign-anim.ts`) : allumage, vague, grésillement, chute. Pures.

use crate::rng::hash;

/// Allumage d'un néon : il bégaie avant de tenir (paliers de [`STEP_S`] secondes).
///
/// Écart avec Sillage (`0, .9, .1, .5, .05, 1, .6, 1, .85, 1` toutes les 0,06 s) : ce bégaiement faisait trois flashs en
/// 0,36 s par lettre, donc plus de 3 par seconde dès que deux kicks s'enchaînaient (règle 7 de `CLAUDE.md`). Ici un seul
/// flash par lettre, et deux allumages sont espacés d'au moins [`MIN_IGNITE_GAP_S`] : au plus 3 flashs par seconde.
const STEPS: [f32; 6] = [0.0, 0.9, 0.3, 1.0, 1.0, 1.0];
const STEP_S: f32 = 0.1;
/// Écart minimal entre deux allumages : un peu plus d'un tiers de seconde, soit au plus 2,9 flashs par seconde.
pub const MIN_IGNITE_GAP_S: f32 = 0.34;
/// Durée d'une variation complète de la lumière d'une lettre : jamais plus brusque que `MIN_FADE` du limiteur.
pub const LETTER_FADE_S: f32 = 0.1;
/// Bascules par seconde du grésillement (26 dans Sillage) : trois au plus dans les 0,9 s qu'il dure, une lettre à la fois.
const SIZZLE_RATE: f64 = 3.0;
/// Durée de l'allumage d'une lettre.
pub const IGNITE_SECONDS: f32 = STEPS.len() as f32 * STEP_S;

/// Durée d'une chute, avant la reconstruction (`FALL_SECONDS`).
pub const FALL_SECONDS: f32 = 1.6;
/// Retard de chute entre deux lettres voisines.
pub const FALL_STAGGER: f32 = 0.07;
/// Pesanteur de la chute des lettres, m/s² (le `38` de Sillage).
const FALL_G: f32 = 38.0;

/// Niveau de lumière 0 → 1 d'une lettre `age` secondes après l'ordre d'allumage (négatif : pas encore).
///
/// `calm` (réglage d'accessibilité) remplace le bégaiement par une montée linéaire : pas de variation brusque.
#[must_use]
pub fn ignite_level(age: f32, calm: bool) -> f32 {
    if age <= 0.0 {
        return 0.0;
    }
    if age >= IGNITE_SECONDS {
        return 1.0;
    }
    if calm {
        return age / IGNITE_SECONDS;
    }
    STEPS[((age / STEP_S) as usize).min(STEPS.len() - 1)]
}

/// Bosse de lumière qui court sur les lettres : `phase` 0 → 1 est la phrase, la bosse la parcourt en débordant un peu.
#[must_use]
pub fn wave_gain(index: usize, count: usize, phase: f32) -> f32 {
    if count == 0 {
        return 0.0;
    }
    let front = phase * (count as f32 + 3.0) - 1.5;
    let d = index as f32 - front;
    (-(d * d) / 3.2).exp()
}

/// Lettre qui grésille : toutes les ~9 s, une lettre tirée au hasard clignote ~0,9 s. Renvoie un facteur 0,12 → 1.
///
/// Une seule lettre à la fois, 0,9 s toutes les 9 s, trois bascules au plus (règle 7) ; le mode calme, les flashs coupés
/// et la respiration l'écartent.
#[must_use]
pub fn sizzle(time_s: f64, index: usize, count: usize, seed: f64) -> f32 {
    if count == 0 {
        return 1.0;
    }
    let slot = (time_s / 9.0).floor();
    if (hash(slot, seed) * count as f64).floor() as usize != index {
        return 1.0;
    }
    let local = time_s - slot * 9.0 - 1.0 - hash(slot, seed + 1.0) * 5.0;
    if !(0.0..=0.9).contains(&local) {
        return 1.0;
    }
    if hash((local * SIZZLE_RATE).floor(), seed + index as f64) > 0.45 {
        1.0
    } else {
        0.12
    }
}

/// Rapproche `value` de `target` sans que la lumière change plus vite que [`LETTER_FADE_S`] pour une course complète.
#[must_use]
pub fn slew(value: f32, target: f32, dt_s: f32) -> f32 {
    let step = dt_s / LETTER_FADE_S;
    value + (target - value).clamp(-step, step)
}

/// Une lettre qui tombe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fall {
    /// Distance descendue, m.
    pub drop: f32,
    /// Bascule, rad (le signe selon la parité de la lettre est appliqué par l'appelant).
    pub tilt: f32,
    /// Part de la lumière restante, 1 → 0.
    pub light: f32,
}

/// Chute d'une lettre `age` secondes après le début de sa chute : descente accélérée et bascule.
#[must_use]
pub fn fall_state(age: f32) -> Fall {
    let t = age.max(0.0);
    Fall {
        drop: 0.5 * FALL_G * t * t,
        tilt: (t * 1.6).min(1.2),
        light: (1.0 - t / 0.45).max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_allumage_begaie_puis_tient() {
        assert!(ignite_level(-1.0, false).abs() < f32::EPSILON);
        assert!(ignite_level(0.0, false).abs() < f32::EPSILON);
        assert!((ignite_level(0.12, false) - 0.9).abs() < f32::EPSILON);
        assert!((ignite_level(0.22, false) - 0.3).abs() < f32::EPSILON);
        assert!((ignite_level(IGNITE_SECONDS, false) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn le_mode_calme_monte_sans_a_coup() {
        let mut last = 0.0;
        for i in 1..100 {
            let v = ignite_level(IGNITE_SECONDS * i as f32 / 100.0, true);
            assert!(v >= last);
            last = v;
        }
    }

    #[test]
    fn la_vague_parcourt_la_phrase() {
        let peak = |phase: f32| {
            (0..10)
                .max_by(|&a, &b| wave_gain(a, 10, phase).total_cmp(&wave_gain(b, 10, phase)))
                .unwrap_or(0)
        };
        assert!(peak(0.0) < peak(0.5));
        assert!(peak(0.5) < peak(1.0));
        assert!(wave_gain(0, 0, 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn une_seule_lettre_gresille_a_la_fois() {
        let n = 8;
        for t in (0..400).map(|i| 5.0 + f64::from(i) * 0.05) {
            let dim = (0..n).filter(|&i| sizzle(t, i, n, 7.0) < 1.0).count();
            assert!(dim <= 1, "t={t}");
        }
        // Et ça arrive vraiment, de temps en temps.
        assert!((0..4000).any(|k| (0..n).any(|i| sizzle(f64::from(k) * 0.05, i, n, 7.0) < 1.0)));
    }

    #[test]
    fn le_gresillement_reste_sous_trois_flashs_par_seconde() {
        let n = 6;
        let dt = 1.0_f64 / 240.0;
        for index in 0..n {
            let mut transitions = Vec::new();
            let mut last = 1.0;
            for k in 0..(240 * 120) {
                let t = f64::from(k) * dt;
                let v = sizzle(t, index, n, 7.0);
                if (v - last).abs() > 0.1 {
                    transitions.push(t);
                }
                last = v;
            }
            // Fenêtre glissante d'une seconde : une bascule vers le bas puis le retour font un flash.
            for (i, &t0) in transitions.iter().enumerate() {
                let in_window = transitions[i..].iter().take_while(|&&t| t < t0 + 1.0).count();
                assert!(
                    in_window <= 2 * cf_show::limiter::MAX_FLASHES_PER_SECOND,
                    "lettre {index} à {t0}"
                );
            }
        }
    }

    #[test]
    fn la_lumiere_d_une_lettre_ne_change_pas_plus_vite_que_le_fondu_minimal() {
        let mut v = 0.0;
        let dt = 1.0 / 60.0;
        let mut prev = v;
        for _ in 0..30 {
            v = slew(v, 1.0, dt);
            assert!((v - prev) / dt <= 1.0 / cf_show::limiter::MIN_FADE + 1e-3);
            prev = v;
        }
        assert!((v - 1.0).abs() < f32::EPSILON);
        const { assert!(LETTER_FADE_S >= cf_show::limiter::MIN_FADE) };
    }

    #[test]
    fn la_chute_accelere_et_eteint() {
        let a = fall_state(0.2);
        let b = fall_state(0.4);
        assert!(b.drop > a.drop * 3.0);
        assert!(fall_state(0.5).light.abs() < f32::EPSILON);
        assert!(fall_state(-3.0).drop.abs() < f32::EPSILON);
        assert!(fall_state(9.0).tilt <= 1.2);
    }
}
