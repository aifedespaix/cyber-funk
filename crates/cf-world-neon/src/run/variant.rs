//! Les variantes d'une passation de DJ : de quoi qu'elle ne soit jamais deux fois la même.
//!
//! Chaque passation tire (avec un hasard déterministe, graine = numéro de la passation et rang du DJ visé) : le côté où
//! l'ancienne voiture se range, la vitesse à laquelle elle roule seule, d'où et quand arrive la suivante, combien de temps
//! elles roulent côte à côte, leurs variations de vitesse « organiques » et le cadrage de la caméra. Le style de
//! cadrage ne se répète jamais deux fois de suite. Pur : aucune dépendance à Bevy.

use cf_render::{FRAMING_STYLES, Framing};

use crate::rng::hash;

/// Une oscillation de vitesse : la voiture roule un peu plus ou un peu moins vite que sa vitesse de base, doucement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wobble {
    /// Amplitude, en part de la vitesse de base (0,05 : ± 5 %).
    pub amp: f32,
    /// Période (s).
    pub period_s: f32,
    /// Phase de départ (rad).
    pub phase_rad: f32,
}

impl Wobble {
    /// L'écart relatif de vitesse à `t_s` (autour de 0).
    #[must_use]
    pub fn at(&self, t_s: f32) -> f32 {
        self.amp * (std::f32::consts::TAU * t_s / self.period_s + self.phase_rad).sin()
    }
}

/// Les instants clés d'une passation (s après son début).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Marks {
    /// Fin du freinage de l'ancienne voiture jusqu'à sa vitesse de croisière seule.
    pub slowed: f32,
    /// Le quartier est entièrement sous terre : la route est nue.
    pub folded: f32,
    /// La caméra se place devant la voiture seule.
    pub camera: f32,
    /// La nouvelle voiture commence à se synchroniser, loin derrière.
    pub arrival: f32,
    /// Elle est à hauteur de l'ancienne : elles roulent côte à côte.
    pub abreast: f32,
    /// L'ancienne commence à se désynchroniser.
    pub fading: f32,
    /// L'ancienne a disparu.
    pub gone: f32,
    /// La voiture réelle est rendue au pilote.
    pub release: f32,
    /// Début de la présentation du nouveau DJ (le portique).
    pub gantry: f32,
    /// Fin de la passation : le quartier est en place.
    pub end: f32,
}

/// Durée du freinage jusqu'à la vitesse seule (s).
const SLOW_S: f32 = 6.0;
/// Le quartier est replié à cet instant (s).
pub const FOLD_END_S: f32 = 9.5;
/// Instant où la caméra passe devant (s).
const CAMERA_S: f32 = 5.0;
/// Durée du rattrapage de la nouvelle voiture (s).
pub const CATCH_S: f32 = 9.0;
/// Durée de la désynchronisation de l'ancienne voiture (s).
pub const FADE_S: f32 = 4.0;
/// Délai entre la fin de la désynchronisation et le portique (s).
const BEFORE_GANTRY_S: f32 = 2.5;
/// Du début du portique à la fin du quartier (s) : comme à l'intro, 5 s de portique puis 12 s de quartier.
pub const PRESENT_S: f32 = 17.0;
/// La voiture réelle est rendue au pilote cette durée avant le portique (s) : le temps de reprendre sa vitesse de
/// croisière avant de passer dessous.
const RELEASE_LEAD_S: f32 = 2.0;

/// Une passation tirée.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandoverVariant {
    /// Numéro de la passation depuis le lancement.
    pub number: u32,
    /// Côté où l'ancienne se range : +1 à droite, −1 à gauche.
    pub side: f32,
    /// Vitesse de l'ancienne voiture une fois seule (m/s).
    pub lone_speed_mps: f32,
    /// Distance derrière l'ancienne d'où la nouvelle arrive (m).
    pub gap_m: f32,
    /// Instant où la nouvelle commence à se former (s).
    pub arrival_s: f32,
    /// Durée du côte à côte (s).
    pub together_s: f32,
    /// Variation de vitesse de l'ancienne.
    pub old_wobble: Wobble,
    /// Variation de vitesse de la nouvelle.
    pub new_wobble: Wobble,
    /// Écart latéral final de l'ancienne par rapport au centre de la route (m).
    pub lane_offset_m: f32,
    /// Cadrage de la caméra.
    pub framing: Framing,
}

fn between(seed: f64, salt: f64, lo: f32, hi: f32) -> f32 {
    lo + (hi - lo) * hash(seed, salt) as f32
}

impl HandoverVariant {
    /// Tire la passation `number` vers le DJ de rang `slot`. `previous_style` : le style de la passation d'avant, qui ne
    /// sera pas repris.
    #[must_use]
    pub fn new(number: u32, slot: usize, previous_style: Option<u8>) -> Self {
        let seed = f64::from(number) * 31.0 + slot as f64 * 7.0 + 3.0;
        let side = if hash(seed, 1.0) < 0.5 { -1.0 } else { 1.0 };
        let wobble = |salt: f64| Wobble {
            amp: between(seed, salt, 0.02, 0.07),
            period_s: between(seed, salt + 1.0, 4.5, 8.5),
            phase_rad: between(seed, salt + 2.0, 0.0, std::f32::consts::TAU),
        };
        // Le style : jamais celui d'avant.
        let raw = (hash(seed, 40.0) * f64::from(FRAMING_STYLES)) as u8 % FRAMING_STYLES;
        let style = match previous_style {
            Some(p) if p == raw => (raw + 1) % FRAMING_STYLES,
            _ => raw,
        };
        Self {
            number,
            side,
            lone_speed_mps: between(seed, 2.0, 8.0, 11.5),
            gap_m: between(seed, 3.0, 110.0, 170.0),
            arrival_s: between(seed, 4.0, 8.5, 10.5),
            together_s: between(seed, 5.0, 3.0, 5.0),
            old_wobble: wobble(10.0),
            new_wobble: wobble(20.0),
            lane_offset_m: between(seed, 6.0, 3.2, 4.2),
            framing: framing(style, seed),
        }
    }

    /// Les instants clés de cette passation.
    #[must_use]
    pub fn marks(&self) -> Marks {
        let abreast = self.arrival_s + CATCH_S;
        let fading = abreast + self.together_s;
        let gone = fading + FADE_S;
        let gantry = gone + BEFORE_GANTRY_S;
        Marks {
            slowed: SLOW_S,
            folded: FOLD_END_S,
            camera: CAMERA_S,
            arrival: self.arrival_s,
            abreast,
            fading,
            gone,
            release: gantry - RELEASE_LEAD_S,
            gantry,
            end: gantry + PRESENT_S,
        }
    }

    /// Durée totale (s).
    #[must_use]
    pub fn duration_s(&self) -> f32 {
        self.marks().end
    }
}

/// Le cadrage du style `style`, avec un peu de hasard sur les valeurs.
fn framing(style: u8, seed: f64) -> Framing {
    let j = |salt: f64, lo: f32, hi: f32| between(seed, 50.0 + salt, lo, hi);
    let flip = if hash(seed, 60.0) < 0.5 { -1.0 } else { 1.0 };
    match style {
        // Bas et proche : la voiture file sur la caméra, la suivante pointe au loin.
        0 => Framing {
            style,
            ahead_m: j(0.0, 16.0, 22.0),
            height_m: j(1.0, 0.8, 1.3),
            lateral_m: flip * j(2.0, 1.0, 2.2),
            sweep_m: 0.0,
            look_back_m: j(3.0, 50.0, 80.0),
            fov_deg: j(4.0, 44.0, 50.0),
        },
        // Longue focale : la distance s'écrase, la voiture qui arrive grossit lentement.
        1 => Framing {
            style,
            ahead_m: j(0.0, 38.0, 52.0),
            height_m: j(1.0, 1.8, 2.8),
            lateral_m: flip * j(2.0, 2.0, 3.5),
            sweep_m: 0.0,
            look_back_m: j(3.0, 60.0, 90.0),
            fov_deg: j(4.0, 26.0, 32.0),
        },
        // Rasant, au bord de la route.
        2 => Framing {
            style,
            ahead_m: j(0.0, 11.0, 16.0),
            height_m: j(1.0, 0.5, 0.9),
            lateral_m: flip * j(2.0, 5.0, 6.5),
            sweep_m: 0.0,
            look_back_m: j(3.0, 40.0, 60.0),
            fov_deg: j(4.0, 52.0, 60.0),
        },
        // Grue : haut, on voit la route nue et les deux voitures se rejoindre.
        3 => Framing {
            style,
            ahead_m: j(0.0, 24.0, 32.0),
            height_m: j(1.0, 6.0, 9.0),
            lateral_m: flip * j(2.0, 0.0, 1.5),
            sweep_m: 0.0,
            look_back_m: j(3.0, 50.0, 80.0),
            fov_deg: j(4.0, 48.0, 56.0),
        },
        // Orbite lente autour de la voiture seule.
        _ => Framing {
            style,
            ahead_m: j(0.0, 18.0, 26.0),
            height_m: j(1.0, 1.4, 2.2),
            lateral_m: -flip * j(2.0, 3.0, 4.0),
            sweep_m: flip * j(5.0, 6.5, 8.5),
            look_back_m: j(3.0, 40.0, 70.0),
            fov_deg: j(4.0, 42.0, 50.0),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_variant() {
        assert_eq!(HandoverVariant::new(3, 1, None), HandoverVariant::new(3, 1, None));
    }

    #[test]
    fn values_stay_in_their_documented_ranges_for_many_handovers() {
        for number in 0..300 {
            let v = HandoverVariant::new(number, (number % 5) as usize, None);
            assert!((8.0..=11.5).contains(&v.lone_speed_mps), "{v:?}");
            assert!((110.0..=170.0).contains(&v.gap_m) && (3.0..=5.0).contains(&v.together_s));
            assert!(v.side.abs() == 1.0 && v.framing.style < FRAMING_STYLES);
            let m = v.marks();
            assert!(m.camera < m.folded && m.slowed < m.folded);
            assert!(m.arrival < m.abreast && m.abreast < m.fading && m.fading < m.gone);
            assert!(m.gone <= m.release && m.release < m.gantry && m.gantry < m.end);
            assert!((40.0..=52.0).contains(&v.duration_s()), "{}", v.duration_s());
        }
    }

    #[test]
    fn the_camera_style_never_repeats_twice_in_a_row() {
        let mut previous = None;
        for number in 0..500 {
            let v = HandoverVariant::new(number, 1, previous);
            assert_ne!(Some(v.framing.style), previous, "passation {number}");
            previous = Some(v.framing.style);
        }
    }

    #[test]
    fn handovers_are_not_all_the_same() {
        let mut sides = std::collections::BTreeSet::new();
        let mut styles = std::collections::BTreeSet::new();
        let mut gaps = std::collections::BTreeSet::new();
        for number in 0..40 {
            let v = HandoverVariant::new(number, 1, None);
            sides.insert(v.side as i32);
            styles.insert(v.framing.style);
            gaps.insert(v.gap_m as i32 / 10);
        }
        assert_eq!(sides.len(), 2, "les deux côtés");
        assert!(styles.len() >= 4, "{styles:?}");
        assert!(gaps.len() >= 4, "{gaps:?}");
    }

    #[test]
    fn wobbles_are_gentle() {
        for number in 0..100 {
            let v = HandoverVariant::new(number, 2, None);
            for k in 0..200 {
                let t = k as f32 * 0.2;
                assert!(v.old_wobble.at(t).abs() <= 0.07 + 1e-6 && v.new_wobble.at(t).abs() <= 0.07 + 1e-6);
            }
        }
    }
}
