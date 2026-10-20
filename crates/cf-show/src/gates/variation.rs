//! `Variation` de Sillage (`engine/show/gates.ts`) : les variations de l'après-set, généralisées à deux looks.

/// Probabilité de prendre le look du bas (le niveau 4 de Sillage) plutôt que celui du haut (le niveau 5).
const LOWER_CHANCE: f64 = 0.4;
/// Part minimale et étendue du mélange tiré, de 0,2 à 0,9 du chemin vers le look du haut.
const MIX_MIN: f64 = 0.2;
const MIX_SPREAD: f64 = 0.7;
/// Durée minimale et étendue (s) d'une variation : de 45 à 90 s.
const HOLD_MIN_S: f64 = 45.0;
const HOLD_SPREAD_S: f64 = 45.0;

/// Variations de l'après-set (le niveau 5 au-delà de 100 % dans Sillage) : de temps en temps un tirage mêle les portes
/// d'un look du bas à celles d'un look du haut, pour que le show ne boucle jamais à l'identique.
///
/// Le tirage se lit avec [`Variation::uses_lower`] (le look de départ du fondu : le bas ou le haut) et
/// [`Variation::mix`] (avancement vers le haut) ; à passer à [`super::GateBlender::update`]. Le hasard est injecté
/// (`random` renvoie un nombre de [0, 1[) : une reprise avec la même graine rejoue pareil.
pub struct Variation<R> {
    lower: bool,
    mix: f32,
    left_s: f64,
    random: R,
}

impl<R> std::fmt::Debug for Variation<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Variation")
            .field("lower", &self.lower)
            .field("mix", &self.mix)
            .field("left_s", &self.left_s)
            .finish_non_exhaustive()
    }
}

impl<R: FnMut() -> f64> Variation<R> {
    /// Une variation au repos, qui tire avec `random`.
    pub fn new(random: R) -> Self {
        Self {
            lower: false,
            mix: 0.0,
            left_s: 0.0,
            random,
        }
    }

    /// Retour au repos : look du haut, mélange nul.
    pub fn reset(&mut self) {
        self.lower = false;
        self.mix = 0.0;
        self.left_s = 0.0;
    }

    /// Vrai si le fondu part du look du bas (sinon du look du haut, sans effet sur les nombres).
    #[must_use]
    pub fn uses_lower(&self) -> bool {
        self.lower
    }

    /// Avancement du look de départ vers celui du haut, 0,2 à 0,9.
    #[must_use]
    pub fn mix(&self) -> f32 {
        self.mix
    }

    /// Avance d'une image de `dt_s` secondes. `on` : les variations s'appliquent (sinon retour au repos). Renvoie vrai
    /// quand elles s'appliquent (`uses_lower` et `mix` sont alors à lire).
    pub fn update(&mut self, dt_s: f64, on: bool) -> bool {
        if !on {
            self.reset();
            return false;
        }
        self.left_s -= dt_s;
        if self.left_s <= 0.0 {
            self.lower = (self.random)() < LOWER_CHANCE;
            self.mix = (MIX_MIN + MIX_SPREAD * (self.random)()) as f32;
            self.left_s = HOLD_MIN_S + HOLD_SPREAD_S * (self.random)();
        }
        true
    }
}
