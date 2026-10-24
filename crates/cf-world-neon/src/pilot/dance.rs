//! La chorégraphie (`dance.ts`) : la position latérale voulue pour un style de danse, calée sur le temps musical, et
//! l'horloge de danse qui reste dans le tempo même quand la détection hésite.

use std::f64::consts::TAU;

use crate::rng::hash;

fn frac(x: f64) -> f64 {
    x - x.floor()
}

/// Triangle de période 1, de -1 à 1.
fn triangle(u: f64) -> f64 {
    4.0 * (frac(u) - 0.5).abs() - 1.0
}

/// Dent de scie de période 1 : montée lente de -1 à 1, puis retour brutal.
fn saw(u: f64) -> f64 {
    frac(u) * 2.0 - 1.0
}

/// Voies d'une danse « voies » : une nouvelle toutes les quatre mesures, jamais deux fois la même d'affilée.
const LANES: [f64; 4] = [-0.85, -0.3, 0.3, 0.85];

fn lane(bar: i64) -> f64 {
    // `sin(n × 91,7 + 3,1) × 43758,5453` : le hasard maison de `dance.ts`, pas `random.ts`.
    let pick = |n: i64| -> usize {
        let x = (n as f64 * 91.7 + 3.1).sin() * 43_758.545_3;
        ((frac(x) * LANES.len() as f64).floor() as usize).min(LANES.len() - 1)
    };
    let mut index = pick(bar);
    if bar > 0 && index == pick(bar - 1) {
        index = (index + 1) % LANES.len();
    }
    LANES[index]
}

/// Les styles de danse de Sillage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DanceStyle {
    /// Dent de scie : dérive lente d'un bord à l'autre, retour brutal.
    Sawtooth,
    /// Grande courbe sur seize temps.
    #[default]
    Sweep,
    /// Zigzag en triangle.
    Zigzag,
    /// Slalom sinusoïdal sur quatre temps.
    Slalom,
    /// Saute d'un côté à l'autre à chaque temps.
    Hop,
    /// Deux sinusoïdes mêlées.
    Snake,
    /// Voies : une nouvelle toutes les quatre mesures.
    Lanes,
}

impl DanceStyle {
    /// Les danses qui changent de côté d'un coup demandent une direction plus vive.
    #[must_use]
    pub fn snappy(self) -> bool {
        matches!(self, Self::Sawtooth | Self::Hop | Self::Lanes | Self::Zigzag)
    }
}

/// Position latérale voulue, entre -1 (bord gauche) et 1 (bord droit), pour une danse donnée. `beat` est un compte de
/// temps continu : toutes les figures se calent dessus, donc sur le tempo de la musique.
#[must_use]
pub fn dance_position(style: DanceStyle, beat: f64) -> f64 {
    match style {
        DanceStyle::Sawtooth => 0.9 * saw(beat / 4.0),
        DanceStyle::Sweep => 0.92 * (beat / 16.0 * TAU).sin(),
        DanceStyle::Zigzag => 0.8 * triangle(beat / 2.0),
        DanceStyle::Slalom => 0.62 * (beat / 4.0 * TAU).sin(),
        DanceStyle::Hop => {
            if (beat.floor() as i64).rem_euclid(2) == 0 {
                -0.72
            } else {
                0.72
            }
        }
        DanceStyle::Snake => 0.7 * (beat / 8.0 * TAU).sin() + 0.28 * (beat / 3.0 * TAU).sin(),
        DanceStyle::Lanes => lane((beat / 4.0).floor() as i64),
    }
}

/// Étage voulu pour la mesure `bar`, selon le style de danse : de 1 à 6 (`danceFloor`).
#[must_use]
pub fn dance_floor(style: DanceStyle, bar: i64) -> i32 {
    let cap = crate::route::FLOOR_COUNT as f64;
    let clamp_floor = |f: f64| f.round().clamp(1.0, cap) as i32;
    let b = bar as f64;
    match style {
        DanceStyle::Sawtooth => clamp_floor(1.0 + bar.rem_euclid(5) as f64),
        DanceStyle::Sweep => clamp_floor(3.5 + 2.5 * (b / 8.0 * TAU).sin()),
        DanceStyle::Zigzag => clamp_floor(1.0 + (bar.rem_euclid(10) as f64 - 5.0).abs()),
        DanceStyle::Slalom => clamp_floor(2.5 + 1.5 * (b / 2.0 * std::f64::consts::PI).sin()),
        DanceStyle::Hop => {
            if bar.rem_euclid(2) == 0 {
                1
            } else {
                3
            }
        }
        DanceStyle::Snake => clamp_floor(3.5 + 2.0 * (b * 0.9).sin() + (b * 0.37).sin()),
        DanceStyle::Lanes => clamp_floor(1.0 + hash(b, 77.0) * cap),
    }
}

/// Horloge de danse : avance au tempo détecté et se recale sur chaque kick, de sorte que la voiture reste dans le temps
/// même quand la détection hésite.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DanceClock {
    /// Temps musical continu.
    pub beat: f64,
}

impl DanceClock {
    /// Avance d'une image ; renvoie le temps musical.
    pub fn update(&mut self, dt: f64, bpm: Option<f64>, kick: bool, playing: bool) -> f64 {
        if playing {
            self.beat += dt * bpm.unwrap_or(110.0) / 60.0;
        }
        if kick {
            self.beat += (self.beat.round() - self.beat) * 0.35;
        }
        self.beat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [DanceStyle; 7] = [
        DanceStyle::Sawtooth,
        DanceStyle::Sweep,
        DanceStyle::Zigzag,
        DanceStyle::Slalom,
        DanceStyle::Hop,
        DanceStyle::Snake,
        DanceStyle::Lanes,
    ];

    #[test]
    fn positions_dans_la_route() {
        for style in ALL {
            for i in 0..4000 {
                let p = dance_position(style, f64::from(i) * 0.05);
                assert!((-1.0..=1.0).contains(&p), "{style:?} {p}");
            }
        }
    }

    #[test]
    fn valeurs_de_reference_de_sillage() {
        // `dancePosition("sweep", 4)` = 0,92 × sin(π/2) ; `"hop"` alterne ; `"sawtooth"` à mi-course vaut 0.
        assert!((dance_position(DanceStyle::Sweep, 4.0) - 0.92).abs() < 1e-12);
        assert!((dance_position(DanceStyle::Hop, 0.5) + 0.72).abs() < 1e-12);
        assert!((dance_position(DanceStyle::Hop, 1.5) - 0.72).abs() < 1e-12);
        assert!(dance_position(DanceStyle::Sawtooth, 2.0).abs() < 1e-12);
        assert!((dance_position(DanceStyle::Zigzag, 0.0) - 0.8).abs() < 1e-12);
        assert!((dance_position(DanceStyle::Zigzag, 1.0) + 0.8).abs() < 1e-12);
    }

    #[test]
    fn les_voies_changent_presque_toujours() {
        // Comme dans Sillage, on ne compare qu'au tirage brut de la mesure précédente : une répétition reste possible
        // quand ce tirage a lui-même été décalé, mais elle est rare.
        let same = (1..500).filter(|&bar| (lane(bar) - lane(bar - 1)).abs() < 1e-9).count();
        assert!(same < 100, "{same} répétitions sur 499");
        assert!((1..500).all(|bar| LANES.contains(&lane(bar))));
    }

    #[test]
    fn etages_de_danse_bornes() {
        for style in ALL {
            for bar in 0..200 {
                assert!((1..=6).contains(&dance_floor(style, bar)), "{style:?} {bar}");
            }
        }
    }

    #[test]
    fn l_horloge_suit_le_tempo_et_se_cale_sur_les_kicks() {
        let mut clock = DanceClock::default();
        for _ in 0..60 {
            clock.update(1.0 / 60.0, Some(120.0), false, true);
        }
        assert!((clock.beat - 2.0).abs() < 1e-9, "120 bpm = 2 temps par seconde");
        clock.beat = 3.8;
        let beat = clock.update(0.0, Some(120.0), true, true);
        assert!(beat > 3.8 && beat < 4.0, "{beat}");
        let stopped = clock.update(1.0, Some(120.0), false, false);
        assert!((stopped - beat).abs() < 1e-12, "à l'arrêt elle ne bouge pas");
    }
}
