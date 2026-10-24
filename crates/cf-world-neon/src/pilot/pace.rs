//! L'allure de la voiture (`pace.ts`) : la vitesse que la musique et les portes veulent, avant l'inertie.

use super::input::{BeatInput, ShowInput};

/// Vitesse de croisière de base, en m/s : le « plancher » de la course, même quand la musique est calme.
pub const CRUISE: f64 = 40.0;

/// Un kick qui revient après ce silence de grosse caisse est un « drop ».
const DROP_AFTER: f64 = 1.3;
const BOOST_SECONDS: f64 = 3.6;
const BOOST_COOLDOWN: f64 = 4.5;
/// Le volume récent doit être sous cette part de sa moyenne lente pour qu'un retour du kick soit un drop.
const DROP_DIP: f64 = 0.85;
/// Énergie sous laquelle on considère que la musique s'est arrêtée.
const HUSH_LEVEL: f64 = 0.07;
/// Allure au calme (≈ 60–90 km/h affichés) et au sommet, en m/s.
const CALM_SPEED: f64 = 23.0;
const PEAK_SPEED: f64 = 60.0;
/// Spectacle : poussée d'un dépassement (m/s) et sa durée.
const OVERTAKE_PUSH: f64 = 7.0;
const OVERTAKE_SECONDS: f64 = 1.6;
/// Un dépassement attend un kick pour se traduire en accélération ; passé ce délai (s), il est oublié.
const OVERTAKE_WAIT: f64 = 5.0;

/// Spectacle : force des boosts de drop aux niveaux 4 et 5 (1 ailleurs).
fn boost_gain(level: u8) -> f64 {
    match level {
        4 => 1.12,
        5 => 1.25,
        _ => 1.0,
    }
}

/// Vitesse affichée (km/h) pour une vitesse réelle en m/s. Seul l'affichage change, jamais la voiture : l'allure ordinaire
/// (45 à 70 m/s) se lit 130 à 160, et les plus gros boosts (~100 m/s) montent à 240. Courbe en trois tronçons continus.
#[must_use]
pub fn to_kmh(speed: f64) -> f64 {
    if speed <= 45.0 {
        speed * (130.0 / 45.0)
    } else if speed <= 70.0 {
        130.0 + (speed - 45.0) * (30.0 / 25.0)
    } else {
        (160.0 + (speed - 70.0) * (80.0 / 30.0)).min(250.0)
    }
}

/// Ce que l'allure décide à cette image (`PaceState`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PaceState {
    /// Vitesse voulue, m/s.
    pub speed: f64,
    /// Vrai pendant une seule image, au moment d'un drop qui déclenche un boost.
    pub boosted: bool,
    /// 0–1 : force du boost en cours, pour que la voiture reprenne vite de la vitesse.
    pub surge: f64,
}

/// Pilote la vitesse de la voiture avec le son :
/// - l'ambiance du morceau règle l'allure : ~60–90 km/h quand c'est doux, jusqu'au plein régime au sommet ;
/// - un crescendo (énergie qui monte plus vite que sa moyenne) accélère en continu, un decrescendo relâche ;
/// - le tempo ajuste un peu l'ensemble, chaque kick donne un léger coup d'accélérateur ;
/// - un drop (retour du kick après un break, ou montée brutale) lance un gros boost qui retombe ;
/// - un vrai blanc (presque plus de son) la fait ralentir franchement.
#[derive(Debug, Clone)]
pub struct Pace {
    state: PaceState,
    since_kick: f64,
    fast: f64,
    slow: f64,
    boost: f64,
    cooldown: f64,
    break_amount: f64,
    hush: f64,
    silent_for: f64,
    tempo: f64,
    mood: f64,
    trend: f64,
    /// Allure du spectacle (`gates.cruise`), lissée.
    cruise: f64,
    reward: u8,
    reward_age: f64,
    push: f64,
}

impl Default for Pace {
    fn default() -> Self {
        Self {
            state: PaceState::default(),
            since_kick: 0.0,
            fast: 0.0,
            slow: 0.0,
            boost: 0.0,
            cooldown: 0.0,
            break_amount: 0.0,
            hush: 0.0,
            silent_for: 0.0,
            tempo: 1.0,
            mood: 0.4,
            trend: 0.0,
            cruise: 1.0,
            reward: 0,
            reward_age: 0.0,
            push: 0.0,
        }
    }
}

impl Pace {
    /// Un dépassement réussi (trafic au sol) : une petite accélération au prochain kick.
    pub fn overtook(&mut self) {
        self.reward = (self.reward + 1).min(2);
        self.reward_age = 0.0;
    }

    /// Avance d'une image. `show` : le spectacle de l'image, absent hors spectacle.
    pub fn update(&mut self, b: &BeatInput, dt: f64, show: Option<&ShowInput>) -> PaceState {
        let loud = b.loud;
        self.state.boosted = false;
        self.cooldown = (self.cooldown - dt).max(0.0);

        self.fast += (loud - self.fast) * (1.0 - (-dt / 1.2).exp());
        self.slow += (loud - self.slow) * (1.0 - (-dt / 9.0).exp());
        let rise = self.fast - self.slow;

        self.since_kick = if b.kick { 0.0 } else { self.since_kick + dt };
        // Un vrai drop suit un vrai creux : le volume récent est retombé sous sa moyenne. Un kick manqué par le détecteur
        // (micro, basses voilées) laisse le volume intact et ne doit pas déclencher de boost.
        let drop_back = b.kick && self.break_amount > 0.6 && self.fast < self.slow * DROP_DIP;
        let surge = rise > 0.2 && self.fast > 0.3;
        if (drop_back || surge) && self.cooldown <= 0.0 {
            self.boost = if drop_back { 1.0 } else { 0.7 };
            self.cooldown = BOOST_COOLDOWN;
            self.state.boosted = true;
        }
        self.boost = (self.boost - dt / BOOST_SECONDS).max(0.0);

        self.cruise = approach(self.cruise, show.map_or(1.0, |s| s.gates.cruise), dt, 2.5);
        let gain = show.map_or(1.0, |s| boost_gain(s.level));
        self.reward_age += dt;
        if self.reward > 0 && self.reward_age > OVERTAKE_WAIT {
            self.reward = 0;
        }
        if self.reward > 0 && b.kick && show.is_some() {
            self.reward -= 1;
            self.push = 1.0;
        }
        self.push = (self.push - dt / OVERTAKE_SECONDS).max(0.0);

        self.silent_for = if loud < HUSH_LEVEL { self.silent_for + dt } else { 0.0 };
        self.break_amount = approach(
            self.break_amount,
            if self.since_kick > DROP_AFTER { 1.0 } else { 0.0 },
            dt,
            0.5,
        );
        self.hush = approach(self.hush, if self.silent_for > 0.4 { 1.0 } else { 0.0 }, dt, 0.6);
        let bpm = b.bpm.unwrap_or(120.0);
        self.tempo = approach(self.tempo, (1.0 + (bpm - 120.0) / 400.0).clamp(0.93, 1.1), dt, 1.5);

        // Ambiance : moitié forme du morceau (sections), moitié niveau sonore du moment.
        let level = ((self.fast - 0.06) / 0.34).clamp(0.0, 1.0);
        self.mood = approach(
            self.mood,
            (b.intensity * 0.55 + level * 0.45).clamp(0.0, 1.0),
            dt,
            1.2,
        );
        self.trend = approach(self.trend, (rise / 0.12).clamp(-1.0, 1.0), dt, 1.5);

        let base = CALM_SPEED + (PEAK_SPEED - CALM_SPEED) * self.mood.powf(1.15);
        let crescendo = if self.trend > 0.0 { self.trend * 9.0 } else { self.trend * 5.0 };
        let drive = (base + crescendo) * self.tempo
            + b.bass * 5.0 * (0.4 + self.mood)
            + self.boost * self.boost * 46.0 * gain;
        self.state.speed = (drive * (1.0 - self.break_amount * 0.12) + self.push * self.push * OVERTAKE_PUSH)
            * self.cruise
            * (1.0 - self.hush * 0.55);
        self.state.surge = self.boost;
        self.state
    }

    /// L'état de la dernière image.
    #[must_use]
    pub fn state(&self) -> PaceState {
        self.state
    }

    /// Après un changement de titre : on oublie les états du morceau précédent.
    pub fn reset(&mut self) {
        self.since_kick = 0.0;
        self.boost = 0.0;
        self.break_amount = 0.0;
        self.silent_for = 0.0;
        self.fast = 0.0;
        self.slow = 0.0;
        self.trend = 0.0;
        self.mood = 0.4;
        self.reward = 0;
        self.push = 0.0;
    }
}

/// Rapproche `current` de `target` avec une constante de temps de `seconds`.
pub(super) fn approach(current: f64, target: f64, dt: f64, seconds: f64) -> f64 {
    current + (target - current) * (1.0 - (-dt / seconds).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loud(level: f64) -> BeatInput {
        BeatInput {
            live: true,
            loud: level,
            intensity: level,
            ..BeatInput::default()
        }
    }

    fn run(pace: &mut Pace, b: &BeatInput, seconds: f64, show: Option<&ShowInput>) -> PaceState {
        let mut out = PaceState::default();
        for _ in 0..(seconds * 60.0) as usize {
            out = pace.update(b, 1.0 / 60.0, show);
        }
        out
    }

    #[test]
    fn l_allure_monte_avec_l_ambiance() {
        let mut calm = Pace::default();
        let mut hot = Pace::default();
        let slow = run(&mut calm, &loud(0.1), 20.0, None).speed;
        let fast = run(&mut hot, &loud(0.9), 20.0, None).speed;
        assert!(slow < fast, "{slow} < {fast}");
        assert!((15.0..40.0).contains(&slow), "{slow}");
        assert!(fast > 45.0, "{fast}");
    }

    #[test]
    fn le_silence_freine_franchement() {
        let mut pace = Pace::default();
        let moving = run(&mut pace, &loud(0.5), 10.0, None).speed;
        let silent = run(&mut pace, &loud(0.0), 10.0, None).speed;
        assert!(silent < moving * 0.6, "{silent} vs {moving}");
    }

    #[test]
    fn la_porte_cruise_regle_l_allure() {
        let gates = |cruise| ShowInput {
            level: 3,
            heat: 0.5,
            breathing: false,
            drop_now: false,
            time_scale: 1.0,
            gates: super::super::input::PilotGates {
                cruise,
                flight_rate: 1.0,
                max_floor: 3,
                flight_on_drop_only: false,
                tricks: true,
            },
        };
        let (mut a, mut b) = (Pace::default(), Pace::default());
        let slow = run(&mut a, &loud(0.5), 15.0, Some(&gates(0.6))).speed;
        let fast = run(&mut b, &loud(0.5), 15.0, Some(&gates(1.15))).speed;
        assert!((fast / slow - 1.15 / 0.6).abs() < 0.05, "{slow} {fast}");
    }

    #[test]
    fn affichage_en_kmh_continu() {
        assert!((to_kmh(45.0) - 130.0).abs() < 1e-9);
        assert!((to_kmh(70.0) - 160.0).abs() < 1e-9);
        assert!((to_kmh(500.0) - 250.0).abs() < 1e-9);
    }
}
