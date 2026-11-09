//! Les petits écarts de la voiture : de temps en temps elle se laisse glisser sur le côté, enchaîne un zigzag calé sur la
//! musique, ou traverse la route sur un drop. Rien de cela n'existe dans Sillage ; c'est de la vie en plus par-dessus la
//! ligne de course (voir [`RaceLine`](super::race_line::RaceLine)).
//!
//! Trois gestes, tous rares et bornés pour rester crédibles (une vraie voiture ne change pas de file à chaque temps) :
//!
//! - **Embardée** : sur un kick, de temps en temps, un coup de volant d'quarante centimètres à un mètre vingt, puis retour au
//!   calme. Jamais deux en moins de [`NUDGE_COOLDOWN_S`] ; pas du tout quand le morceau est calme.
//! - **Zigzag** : sur la phrase de huit mesures, une fois de temps en temps et seulement quand l'énergie est haute, deux
//!   oscillations lentes (quatre temps chacune) avec une enveloppe qui monte et redescend.
//! - **Traversée** : un drop envoie la voiture de l'autre côté de la route (voir `RaceLine::mirror`).
//!
//! La sortie est un décalage en m, déjà doux (sinus, ressort du premier ordre) : le volant et l'inertie de la voiture le
//! lissent encore.

use std::f64::consts::{PI, TAU};

use crate::rng::hash;

/// Délai minimal entre deux embardées, en s.
pub const NUDGE_COOLDOWN_S: f64 = 3.0;
/// En dessous de cette énergie (0..1), pas d'embardée ni de zigzag : la voiture reste posée.
pub const MIN_ENERGY: f64 = 0.35;
/// Au-dessus de cette énergie, un zigzag devient possible.
const ZIGZAG_ENERGY: f64 = 0.55;
/// Période du zigzag, en temps : une fenêtre de huit mesures dont la première moitié est occupée.
const WINDOW_BEATS: f64 = 32.0;
/// Durée du zigzag dans sa fenêtre, en temps : deux oscillations de quatre temps.
const ZIGZAG_BEATS: f64 = 8.0;
/// Vitesse de rattrapage de l'embardée vers sa cible, 1/s.
const NUDGE_RATE: f64 = 2.5;
/// Constante de temps du retour au calme après une embardée, en s.
const NUDGE_RELAX_S: f64 = 1.0;
/// Décalage maximal, en m (somme des deux gestes).
pub const MAX_OFFSET: f64 = 2.2;

/// Les gestes en cours. État minuscule, aucune allocation.
#[derive(Debug, Clone)]
pub struct Sway {
    seed: f64,
    nudge: f64,
    nudge_target: f64,
    cooldown_s: f64,
    kicks: u32,
}

impl Sway {
    /// Un balancement de graine `seed` : le hasard des gestes, décidé une fois pour toutes pour une graine donnée.
    #[must_use]
    pub fn new(seed: f64) -> Self {
        Self {
            seed,
            nudge: 0.0,
            nudge_target: 0.0,
            cooldown_s: NUDGE_COOLDOWN_S,
            kicks: 0,
        }
    }

    /// Repart à plat (téléportation, changement de voiture).
    pub fn reset(&mut self) {
        self.nudge = 0.0;
        self.nudge_target = 0.0;
        self.cooldown_s = NUDGE_COOLDOWN_S;
    }

    /// Probabilité qu'un kick déclenche une embardée à cette énergie : nulle au calme, un kick sur cinq au sommet.
    fn nudge_chance(energy: f64) -> f64 {
        if energy < MIN_ENERGY { 0.0 } else { 0.06 + 0.14 * energy }
    }

    /// Probabilité qu'une fenêtre de huit mesures porte un zigzag : nulle sous `ZIGZAG_ENERGY`, 55 % au sommet.
    fn zigzag_chance(energy: f64) -> f64 {
        ((energy - ZIGZAG_ENERGY) / (1.0 - ZIGZAG_ENERGY)).clamp(0.0, 1.0) * 0.55
    }

    /// Le zigzag à ce temps musical : décalage en m, 0 hors fenêtre.
    fn zigzag(&self, beat: f64, energy: f64) -> f64 {
        let window = (beat / WINDOW_BEATS).floor();
        let within = beat - window * WINDOW_BEATS;
        if within >= ZIGZAG_BEATS || hash(window, self.seed + 17.0) >= Self::zigzag_chance(energy) {
            return 0.0;
        }
        let progress = within / ZIGZAG_BEATS;
        let envelope = (progress * PI).sin();
        let amplitude = 0.8 + 0.7 * energy;
        let side = if hash(window, self.seed + 29.0) < 0.5 {
            -1.0
        } else {
            1.0
        };
        side * amplitude * envelope * (progress * 2.0 * TAU).sin()
    }

    /// Avance d'une image et rend le décalage latéral voulu, en m, entre `-MAX_OFFSET` et `MAX_OFFSET`.
    ///
    /// - `beat` : temps musical continu (horloge de danse) ;
    /// - `kick` : un kick vient d'avoir lieu (une image) ;
    /// - `energy` : 0 (calme) → 1 (sommet) ;
    /// - `calm` : mode respiration ou entrée non fiable, tout reste posé.
    pub fn update(&mut self, dt: f64, beat: f64, kick: bool, energy: f64, calm: bool) -> f64 {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.25) } else { 0.0 };
        let energy = if energy.is_finite() {
            energy.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let beat = if beat.is_finite() { beat } else { 0.0 };
        self.cooldown_s = (self.cooldown_s - dt).max(0.0);

        if kick {
            self.kicks = self.kicks.wrapping_add(1);
            let roll = hash(f64::from(self.kicks), self.seed + 3.0);
            if !calm && self.cooldown_s <= 0.0 && roll < Self::nudge_chance(energy) {
                let side = if hash(f64::from(self.kicks), self.seed + 5.0) < 0.5 {
                    -1.0
                } else {
                    1.0
                };
                self.nudge_target = side * (0.4 + 0.8 * energy);
                self.cooldown_s = NUDGE_COOLDOWN_S;
            }
        }
        // La cible retombe toute seule, la voiture la rattrape en douceur : un coup de volant, puis on se redresse.
        self.nudge_target *= (-dt / NUDGE_RELAX_S).exp();
        self.nudge += (self.nudge_target - self.nudge) * (1.0 - (-dt * NUDGE_RATE).exp());

        let zigzag = if calm { 0.0 } else { self.zigzag(beat, energy) };
        (self.nudge + zigzag).clamp(-MAX_OFFSET, MAX_OFFSET)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rejoue `seconds` s à 120 bpm avec un kick par temps ; rend (décalage mini, maxi, vitesse latérale max, embardées).
    fn run(sway: &mut Sway, energy: f64, seconds: f64) -> (f64, f64, f64, usize) {
        let (mut lo, mut hi, mut fastest, mut nudges) = (f64::MAX, f64::MIN, 0.0_f64, 0);
        let (mut previous, mut beat) = (0.0, 0.0);
        let mut last_target = 0.0_f64;
        for frame in 0..(seconds * 60.0) as usize {
            beat += 2.0 / 60.0;
            let kick = frame % 30 == 0;
            let x = sway.update(1.0 / 60.0, beat, kick, energy, false);
            if sway.nudge_target.abs() > last_target.abs() + 0.3 {
                nudges += 1;
            }
            last_target = sway.nudge_target;
            lo = lo.min(x);
            hi = hi.max(x);
            fastest = fastest.max(((x - previous) * 60.0).abs());
            previous = x;
        }
        (lo, hi, fastest, nudges)
    }

    #[test]
    fn rien_ne_bouge_au_calme() {
        let (lo, hi, _, nudges) = run(&mut Sway::new(4.0), 0.2, 300.0);
        assert!(lo == 0.0 && hi == 0.0 && nudges == 0, "{lo} {hi} {nudges}");
    }

    #[test]
    fn les_ecarts_restent_bornes_et_lents() {
        for seed in [1.0, 7.0, 42.0] {
            let (lo, hi, fastest, _) = run(&mut Sway::new(seed), 1.0, 600.0);
            assert!(lo >= -MAX_OFFSET && hi <= MAX_OFFSET, "{lo} {hi}");
            assert!(fastest < 8.0, "vitesse latérale {fastest} m/s");
            assert!(hi > 0.5 || lo < -0.5, "au sommet, la voiture s'écarte");
        }
    }

    #[test]
    fn les_embardees_sont_espacees() {
        // Dix minutes au sommet, un kick par demi-seconde : au plus une par délai minimal, et bien moins en pratique.
        let (_, _, _, nudges) = run(&mut Sway::new(11.0), 1.0, 600.0);
        assert!(nudges <= (600.0 / NUDGE_COOLDOWN_S) as usize, "{nudges}");
        assert!(nudges < 120, "trop fréquentes : {nudges}");
    }

    #[test]
    fn le_mode_calme_pose_tout() {
        let mut sway = Sway::new(2.0);
        let mut beat = 0.0;
        for frame in 0..6000 {
            beat += 2.0 / 60.0;
            let x = sway.update(1.0 / 60.0, beat, frame % 30 == 0, 1.0, true);
            assert!(x.abs() < 1e-9);
        }
    }

    #[test]
    fn entrees_non_finies_tolerees() {
        let mut sway = Sway::new(1.0);
        let x = sway.update(f64::NAN, f64::NAN, true, f64::NAN, false);
        assert!(x.is_finite());
    }

    #[test]
    fn le_zigzag_a_lieu_et_s_eteint() {
        // Cherche une fenêtre portant un zigzag à l'énergie max : il oscille des deux côtés, puis la fenêtre se vide.
        let sway = Sway::new(5.0);
        let window = (0..40).find(|w| sway.zigzag(f64::from(*w) * WINDOW_BEATS + 2.0, 1.0).abs() > 0.1);
        let window = f64::from(window.unwrap_or(0));
        let samples: Vec<f64> = (0..80)
            .map(|i| sway.zigzag(window * WINDOW_BEATS + f64::from(i) * 0.1, 1.0))
            .collect();
        assert!(samples.iter().any(|v| *v > 0.3) && samples.iter().any(|v| *v < -0.3));
        assert!(sway.zigzag(window * WINDOW_BEATS + ZIGZAG_BEATS + 1.0, 1.0).abs() < 1e-12);
    }
}
