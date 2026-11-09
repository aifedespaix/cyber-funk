//! La météo à la main (onglet Debug) : forcer ou couper la pluie, la brume, le brouillard bas et les éclairs.
//!
//! L'interface écrit [`WeatherControl`] ; la météo du monde (`cf-world-neon::weather`) le lit à chaque image. Par défaut
//! tout est en automatique, comme avant : la pluie suit le niveau du spectacle et rien n'est coupé. Rien n'est
//! sauvegardé, chaque session repart en automatique.

use bevy::prelude::*;

/// Ce que fait la pluie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RainMode {
    /// Les averses suivent le niveau du spectacle.
    #[default]
    Auto,
    /// Il pleut, quel que soit le niveau (même sans spectacle ni musique).
    Forced,
    /// Pas une goutte : l'averse en cours s'achève et aucune ne vient.
    Off,
}

impl RainMode {
    /// Les trois modes, dans l'ordre des boutons.
    pub const ALL: [Self; 3] = [Self::Auto, Self::Forced, Self::Off];

    /// Libellé du bouton.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Forced => "Pluie",
            Self::Off => "Sec",
        }
    }
}

/// Les réglages de météo de l'opérateur.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "des bascules indépendantes, une par élément de l'ambiance"
)]
pub struct WeatherControl {
    /// La pluie.
    pub rain: RainMode,
    /// La nappe de brume sous la route.
    pub mist: bool,
    /// Le brouillard bas, les voiles qui rampent à ras de la chaussée.
    pub haze: bool,
    /// Les éclairs d'orage (toujours derrière le limiteur photosensible : activer ne contourne rien).
    pub lightning: bool,
}

impl Default for WeatherControl {
    fn default() -> Self {
        Self {
            rain: RainMode::Auto,
            mist: true,
            haze: true,
            lightning: true,
        }
    }
}

impl WeatherControl {
    /// Tout est comme au démarrage.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn par_defaut_rien_n_est_force_ni_coupe() {
        let control = WeatherControl::default();
        assert!(control.is_default());
        assert_eq!(control.rain, RainMode::Auto);
        assert!(control.mist && control.haze && control.lightning);
    }

    #[test]
    fn chaque_mode_de_pluie_a_son_bouton() {
        let labels: Vec<_> = RainMode::ALL.iter().map(|m| m.label()).collect();
        assert_eq!(labels, ["Auto", "Pluie", "Sec"]);
    }
}
