//! La ligne de course du pilote en spectacle (`race-line.ts`), quand il n'y a rien à ramasser : une trajectoire vivante
//! calée sur le temps musical.

use crate::rng::hash;

/// Positions de file en part de l'amplitude (−1 = bord gauche de la ligne, 1 = bord droit).
const LANES: [f64; 5] = [-1.0, -0.55, 0.0, 0.55, 1.0];
/// Rang de la file centrale, point de départ.
const CENTER: usize = 2;
/// Plus loin que cela, la ligne sortirait de la route accessible (±4,6 m).
const MAX_X: f64 = 3.8;
const BEATS_PER_BAR: f64 = 4.0;
/// Période de la grande courbe : une phrase de huit mesures.
const PHRASE_BEATS: f64 = 32.0;

/// Ligne de course : elle change de file sur le premier temps d'une mesure (rarement au calme, plus souvent quand
/// l'énergie monte), dessine une grande courbe sur la phrase, et son amplitude suit l'énergie : posée près du centre
/// quand c'est calme, large au sommet. Un ressort amorti la lisse : pas de zigzag, jamais plus vif que le volant.
#[derive(Debug, Clone)]
pub struct RaceLine {
    /// Abscisse visée (m).
    pub x: f64,
    speed: f64,
    index: Option<i64>,
    lane: usize,
    previous: usize,
    seed: f64,
}

impl RaceLine {
    /// Une ligne de graine `seed` (le hasard des files).
    #[must_use]
    pub fn new(seed: f64) -> Self {
        Self {
            x: 0.0,
            speed: 0.0,
            index: None,
            lane: CENTER,
            previous: CENTER,
            seed,
        }
    }

    /// Repart de `x` (changement de mode, téléportation) sans à-coup.
    pub fn reset(&mut self, x: f64) {
        self.x = if x.is_finite() { x } else { 0.0 };
        self.speed = 0.0;
        self.index = None;
    }

    /// - `beat` : temps musical continu (horloge de danse) ;
    /// - `energy` : 0 (calme) → 1 (sommet) ;
    /// - `rate` : taux de rattrapage réel du volant (1/s) : le ressort n'est jamais plus vif que lui.
    pub fn update(&mut self, dt: f64, beat: f64, energy: f64, rate: f64) -> f64 {
        let e = if energy.is_finite() { energy } else { 0.3 }.clamp(0.0, 1.0);
        let time = if beat.is_finite() { beat } else { 0.0 };
        let bar = (time / BEATS_PER_BAR).floor() as i64;
        // Au calme on reste longtemps dans une file ; au sommet on en change toutes les deux mesures.
        let every = if e < 0.25 {
            8
        } else if e < 0.55 {
            4
        } else {
            2
        };
        let index = bar.div_euclid(every);
        if self.index != Some(index) {
            self.index = Some(index);
            self.previous = self.lane;
            self.lane = self.pick_lane(index);
        }
        let amplitude = 0.7 + 3.0 * e * e * (3.0 - 2.0 * e) * 0.9;
        let calm = if e < 0.25 { 0.35 + e } else { 1.0 };
        let sweep = (time / PHRASE_BEATS * std::f64::consts::TAU).sin() * amplitude * 0.3 * e;
        let want = (LANES[self.lane] * amplitude * 0.8 * calm + sweep).clamp(-MAX_X, MAX_X);
        let rate = if rate.is_finite() { rate } else { 5.0 };
        self.follow(want, dt, (rate * 0.5).clamp(1.6, 2.6));
        self.x
    }

    /// File suivante : jamais la même, et de l'autre côté une fois sur deux pour que le changement se voie.
    fn pick_lane(&self, index: i64) -> usize {
        let i = index as f64;
        let mut lane = ((hash(i, self.seed) * LANES.len() as f64).floor() as usize).min(LANES.len() - 1);
        if lane == self.previous {
            lane = (lane + 2) % LANES.len();
        }
        let sign = |l: usize| LANES[l].partial_cmp(&0.0).map_or(0, |o| o as i8);
        if hash(i, self.seed + 9.0) < 0.5 && sign(lane) == sign(self.previous) && lane != CENTER {
            lane = LANES.len() - 1 - lane;
        }
        lane
    }

    /// Ressort critique par petits pas : stable même à basse fréquence d'images.
    fn follow(&mut self, want: f64, dt: f64, omega: f64) {
        let mut left = dt.clamp(0.0, 0.25);
        while left > 0.0 {
            let h = left.min(0.02);
            self.speed += (omega * omega * (want - self.x) - 2.0 * omega * self.speed) * h;
            self.x += self.speed * h;
            left -= h;
        }
        if !self.x.is_finite() || !self.speed.is_finite() {
            self.reset(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(line: &mut RaceLine, energy: f64, seconds: f64) -> (f64, f64) {
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        let mut beat = 0.0;
        for _ in 0..(seconds * 60.0) as usize {
            beat += 2.0 / 60.0;
            let x = line.update(1.0 / 60.0, beat, energy, 5.0);
            lo = lo.min(x);
            hi = hi.max(x);
        }
        (lo, hi)
    }

    #[test]
    fn l_amplitude_suit_l_energie_et_reste_sur_la_route() {
        let (lo_calm, hi_calm) = run(&mut RaceLine::new(3.0), 0.1, 120.0);
        let (lo_peak, hi_peak) = run(&mut RaceLine::new(3.0), 1.0, 120.0);
        assert!(hi_calm - lo_calm < hi_peak - lo_peak, "plus large au sommet");
        assert!(lo_peak >= -MAX_X - 0.6 && hi_peak <= MAX_X + 0.6, "{lo_peak} {hi_peak}");
    }

    #[test]
    fn jamais_plus_vive_que_le_volant() {
        let mut line = RaceLine::new(9.0);
        let mut previous = line.x;
        let mut beat = 0.0;
        for _ in 0..7200 {
            beat += 2.0 / 60.0;
            let x = line.update(1.0 / 60.0, beat, 1.0, 5.0);
            assert!(
                ((x - previous) * 60.0).abs() < 12.0,
                "vitesse latérale {}",
                (x - previous) * 60.0
            );
            previous = x;
        }
    }

    #[test]
    fn entrees_non_finies_tolerees() {
        let mut line = RaceLine::new(1.0);
        let x = line.update(0.016, f64::NAN, f64::NAN, f64::NAN);
        assert!(x.is_finite());
    }
}
