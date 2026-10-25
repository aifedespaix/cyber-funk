//! Le cycle de la pluie (`weather.ts` de Sillage) : des averses espacées, ni trop rares ni continues.
//!
//! Logique pure, sans Bevy : le temps est injecté (`dt_s`), le hasard vient d'un RNG à graine (Sillage tirait
//! `Math.random`, jamais rejouable). Même graine et mêmes entrées : mêmes averses.
//!
//! `rain` (0 → 1) est l'intensité de la pluie qui tombe, `wet` (0 → 1) l'état de la chaussée : elle reste luisante un
//! moment après la fin de l'averse. Le tonnerre est un fait ponctuel (`thunder`, vrai une seule image).

use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

/// Durées en secondes : l'averse dure 20 à 30 s, et revient 30 s à 1 min après la fin de la précédente.
pub const RAIN_MIN: f32 = 20.0;
/// Voir [`RAIN_MIN`].
pub const RAIN_MAX: f32 = 30.0;
/// Attente minimale entre deux averses (s).
pub const DRY_MIN: f32 = 30.0;
/// Attente maximale entre deux averses (s).
pub const DRY_MAX: f32 = 60.0;
/// Premier épisode : assez tôt pour qu'on le voie, pas dès la première seconde.
pub const FIRST_MIN: f32 = 18.0;
/// Voir [`FIRST_MIN`].
pub const FIRST_MAX: f32 = 35.0;
/// Le ciel se charge vite (s pour passer de 0 à 1)…
pub const RAIN_IN: f32 = 4.0;
/// … se vide un peu moins vite…
pub const RAIN_OUT: f32 = 5.0;
/// … et la route met plus longtemps à sécher qu'à mouiller.
pub const WET_IN: f32 = 7.0;
/// Voir [`WET_IN`].
pub const WET_OUT: f32 = 16.0;

/// Le plan d'averses d'un niveau du spectacle : `dry` attente entre deux averses, `rain` durée, en secondes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RainPlan {
    /// Attente entre deux averses (min, max), s. `[0, 0]` : jamais de pluie.
    pub dry: [f32; 2],
    /// Durée d'une averse (min, max), s.
    pub rain: [f32; 2],
    /// Vrai orage : le tonnerre revient tout au long de l'averse.
    pub storm: bool,
}

/// La pluie au fil des niveaux du spectacle (`SHOW_RAIN` de Sillage) : rare au début (jamais en pré-intro), plus
/// fréquente ensuite, avec de vrais orages (tonnerre répété) aux niveaux 4 et 5. Un niveau hors table vaut le plus haut.
#[must_use]
pub fn show_rain_plan(level: u8) -> RainPlan {
    let (dry, rain, storm) = match level {
        0 => ([0.0, 0.0], [0.0, 0.0], false),
        1 => ([240.0, 420.0], [15.0, 22.0], false),
        2 => ([150.0, 280.0], [18.0, 26.0], false),
        3 => ([90.0, 180.0], [20.0, 30.0], false),
        4 => ([50.0, 100.0], [25.0, 40.0], true),
        _ => ([35.0, 80.0], [28.0, 45.0], true),
    };
    RainPlan { dry, rain, storm }
}

/// Le cycle des averses.
#[derive(Debug, Clone)]
pub struct Weather {
    /// Intensité de la pluie qui tombe, 0 → 1.
    pub rain: f32,
    /// État de la chaussée, 0 (sèche) → 1 (mouillée).
    pub wet: f32,
    /// Vrai une seule image, quand le tonnerre doit gronder (une averse sur deux, en plein milieu).
    pub thunder: bool,
    enabled: bool,
    raining: bool,
    thundered: bool,
    storm_left_s: f32,
    left_s: f32,
    rng: Pcg32,
}

impl Weather {
    /// Une météo neuve dont les tirages partent de `seed`. Le premier épisode vient entre [`FIRST_MIN`] et [`FIRST_MAX`].
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut rng = Pcg32::seed_from_u64(seed);
        let left_s = span(&mut rng, FIRST_MIN, FIRST_MAX);
        Self {
            rain: 0.0,
            wet: 0.0,
            thunder: false,
            enabled: true,
            raining: false,
            thundered: false,
            storm_left_s: 8.0,
            left_s,
            rng,
        }
    }

    /// Une averse est-elle décidée (le ciel se charge ou se vide d'après elle) ?
    #[must_use]
    pub fn raining(&self) -> bool {
        self.raining
    }

    /// Coupe la météo : l'averse en cours s'achève, aucune autre ne vient. Rallumée, la première revient assez vite.
    pub fn set_enabled(&mut self, on: bool) {
        if on == self.enabled {
            return;
        }
        self.enabled = on;
        self.raining = false;
        self.left_s = span(&mut self.rng, FIRST_MIN, FIRST_MAX);
    }

    /// Avance de `dt_s` secondes. `level` : niveau du spectacle (`None` sans spectacle : la cadence libre) ; pas de pluie
    /// en pré-intro (niveau 0), l'attente entre deux averses raccourcit avec le niveau.
    pub fn update(&mut self, dt_s: f32, level: Option<u8>) {
        let dt_s = if dt_s.is_finite() { dt_s.max(0.0) } else { 0.0 };
        self.thunder = false;
        let plan = level.map(show_rain_plan);
        let allowed = self.enabled && plan.is_none_or(|p| p.dry[0] > 0.0);
        if let Some(p) = plan {
            if allowed && !self.raining && self.left_s > p.dry[1] {
                self.left_s = p.dry[1];
            }
        }
        // Sillage mettait `left = 0` ici, mais le décompte est gelé quand la pluie est interdite : l'averse ne s'arrêtait
        // jamais. On la coupe franchement (voir « Écarts » de la PR J5-04).
        if self.raining && !allowed {
            self.raining = false;
        }
        if allowed {
            self.left_s -= dt_s;
        }
        if allowed && self.left_s <= 0.0 {
            self.raining = !self.raining;
            self.left_s = match (plan, self.raining) {
                (Some(p), true) => span(&mut self.rng, p.rain[0], p.rain[1]),
                (Some(p), false) => span(&mut self.rng, p.dry[0], p.dry[1]),
                (None, true) => span(&mut self.rng, RAIN_MIN, RAIN_MAX),
                (None, false) => span(&mut self.rng, DRY_MIN, DRY_MAX),
            };
            let storm_chance = if plan.is_some_and(|p| p.storm) { 1.0 } else { 0.5 };
            self.thundered = !self.raining || unit(&mut self.rng) < storm_chance;
            self.storm_left_s = span(&mut self.rng, 5.0, 10.0);
        }
        if !self.enabled {
            self.raining = false;
        }
        if self.raining && !self.thundered && self.rain > 0.9 {
            self.thundered = true;
            self.thunder = true;
        }
        // Aux niveaux d'orage, le tonnerre revient tout au long de l'averse.
        if self.raining && plan.is_some_and(|p| p.storm) && self.thundered && self.rain > 0.9 {
            self.storm_left_s -= dt_s;
            if self.storm_left_s <= 0.0 {
                self.thunder = true;
                self.storm_left_s = span(&mut self.rng, 7.0, 15.0);
            }
        }
        let rain_target = if self.raining { 1.0 } else { 0.0 };
        self.rain = approach(
            self.rain,
            rain_target,
            dt_s / if self.raining { RAIN_IN } else { RAIN_OUT },
        );
        // La route est mouillée tant qu'il pleut, puis sèche lentement.
        self.wet = approach(
            self.wet,
            rain_target,
            dt_s / if self.raining { WET_IN } else { WET_OUT },
        );
    }
}

/// Réel uniforme dans `[0, 1)`.
fn unit(rng: &mut Pcg32) -> f32 {
    (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32
}

fn span(rng: &mut Pcg32, min: f32, max: f32) -> f32 {
    min + unit(rng) * (max - min)
}

fn approach(value: f32, target: f32, step: f32) -> f32 {
    if value < target {
        target.min(value + step)
    } else {
        target.max(value - step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 30.0;

    fn run(weather: &mut Weather, seconds: f32, level: Option<u8>) -> (f32, usize) {
        let (mut max_rain, mut thunders) = (0.0_f32, 0);
        for _ in 0..(seconds / DT) as usize {
            weather.update(DT, level);
            max_rain = max_rain.max(weather.rain);
            thunders += usize::from(weather.thunder);
        }
        (max_rain, thunders)
    }

    #[test]
    fn la_premiere_averse_vient_entre_18_et_35_secondes_plus_la_montee() {
        for seed in 0..20 {
            let mut w = Weather::new(seed);
            let mut first = None;
            for i in 0..(80.0 / DT) as usize {
                w.update(DT, None);
                if first.is_none() && w.rain > 0.0 {
                    first = Some(i as f32 * DT);
                }
            }
            let t = first.expect("une averse en 80 s");
            assert!((FIRST_MIN - 1.0..=FIRST_MAX + 1.0).contains(&t), "seed {seed} : {t}");
        }
    }

    #[test]
    fn meme_graine_memes_averses() {
        let (mut a, mut b) = (Weather::new(5), Weather::new(5));
        for _ in 0..(300.0 / DT) as usize {
            a.update(DT, None);
            b.update(DT, None);
            assert!((a.rain - b.rain).abs() < f32::EPSILON && a.thunder == b.thunder);
        }
    }

    #[test]
    fn pas_de_pluie_en_pre_intro() {
        let mut w = Weather::new(1);
        let (max, _) = run(&mut w, 600.0, Some(0));
        assert!(max == 0.0, "niveau 0 : jamais de pluie");
    }

    #[test]
    fn une_averse_en_cours_s_arrete_quand_le_niveau_retombe_a_zero() {
        let mut w = Weather::new(2);
        run(&mut w, 40.0, None);
        assert!(w.raining(), "il pleut après 40 s libres");
        run(&mut w, 30.0, Some(0));
        assert!(!w.raining() && w.rain < 0.01, "la pluie s'est vidée");
    }

    #[test]
    fn l_attente_raccourcit_avec_le_niveau_et_les_orages_grondent_en_boucle() {
        let (mut calm, mut storm) = (Weather::new(3), Weather::new(3));
        let rains = |w: &mut Weather, level: u8| {
            let mut count = 0;
            let mut was = false;
            for _ in 0..(1800.0 / DT) as usize {
                w.update(DT, Some(level));
                if w.raining() && !was {
                    count += 1;
                }
                was = w.raining();
            }
            count
        };
        let (c, s) = (rains(&mut calm, 1), rains(&mut storm, 5));
        assert!(s > c, "plus d'averses au niveau 5 ({s}) qu'au niveau 1 ({c})");
        let mut w = Weather::new(4);
        let (_, thunders) = run(&mut w, 1800.0, Some(5));
        assert!(thunders >= 5, "orage : tonnerre répété, {thunders}");
    }

    #[test]
    fn couper_la_meteo_acheve_l_averse_et_rien_ne_revient() {
        let mut w = Weather::new(6);
        run(&mut w, 40.0, None);
        w.set_enabled(false);
        let (_, _) = run(&mut w, 20.0, None);
        assert!(w.rain < 0.01);
        let (max, _) = run(&mut w, 600.0, None);
        assert!(max < 0.01, "plus aucune averse");
    }

    #[test]
    fn la_route_seche_moins_vite_que_le_ciel_se_vide() {
        let mut w = Weather::new(7);
        run(&mut w, 40.0, None);
        w.set_enabled(false);
        run(&mut w, 6.0, None);
        assert!(
            w.rain < 0.05 && w.wet > 0.4,
            "ciel vide ({}), route encore mouillée ({})",
            w.rain,
            w.wet
        );
    }

    #[test]
    fn les_valeurs_aberrantes_ne_cassent_rien() {
        let mut w = Weather::new(8);
        for dt in [f32::NAN, -1.0, f32::INFINITY, 0.0] {
            w.update(dt, Some(200));
            assert!(w.rain.is_finite() && w.wet.is_finite());
        }
    }
}
