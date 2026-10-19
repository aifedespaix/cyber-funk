//! Paramètres pilotables d'un look et macros (potards assignables).
//!
//! Un [`ParamPath`] est le chemin d'un champ numérique du fichier de look (`"gates.cruise"`, `"layers.foreground"`) :
//! le même texte dans le fichier et dans l'interface, mais une liste close côté code (enum), vérifiée dès la lecture.
//! Aucune chaîne libre n'est donc interprétée pendant le show.

use std::fmt;
use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

/// Nombre maximal de macros par look (les huit potards de la fenêtre opérateur).
pub const MAX_MACROS: usize = 8;
/// Nombre maximal de paramètres qu'une même macro pilote.
pub const MAX_DRIVES: usize = 8;
/// Longueur maximale du nom d'une macro, en caractères (il doit tenir sous un potard).
pub const MAX_MACRO_NAME: usize = 24;

/// Un paramètre numérique d'un look que l'on peut piloter (macro, interface, MIDI).
///
/// Les plages de [`ParamPath::range`] sont aussi celles que la validation impose aux valeurs écrites dans les looks :
/// une seule source de vérité.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "&'static str")]
pub enum ParamPath {
    /// `gates.flight_rate` : multiplicateur de la fréquence des envols (0 = jamais).
    FlightRate,
    /// `gates.stunt_gap_scale` : multiplicateur de l'attente entre deux cascades (< 1 : plus serré).
    StuntGapScale,
    /// `gates.event_rate` : multiplicateur de la fréquence des événements du trafic.
    EventRate,
    /// `gates.meteor_rate` : lancers d'astéroïdes par minute.
    MeteorRate,
    /// `gates.cruise` : multiplicateur d'allure.
    Cruise,
    /// `gates.ground_traffic` : densité du trafic au sol.
    GroundTraffic,
    /// `gates.fx.rgb_split` : dosage du décalage RVB.
    FxRgbSplit,
    /// `gates.fx.flash` : dosage des flashs (toujours plafonné ensuite par le limiteur photosensible).
    FxFlash,
    /// `gates.fx.city_pulse` : dosage de la pulsation de la ville.
    FxCityPulse,
    /// `layers.background` : opacité de la couche de fond (ciel, planètes).
    LayerBackground,
    /// `layers.world` : opacité de la couche du monde.
    LayerWorld,
    /// `layers.foreground` : opacité de l'avant-plan (logo du DJ…).
    LayerForeground,
    /// `layers.overlay` : opacité de la surcouche 2D (cartons-titres).
    LayerOverlay,
    /// `hue_shift_deg` : rotation de teinte de la palette, en degrés.
    HueShift,
}

impl ParamPath {
    /// Tous les paramètres, dans l'ordre du fichier de look.
    pub const ALL: [Self; 14] = [
        Self::FlightRate,
        Self::StuntGapScale,
        Self::EventRate,
        Self::MeteorRate,
        Self::Cruise,
        Self::GroundTraffic,
        Self::FxRgbSplit,
        Self::FxFlash,
        Self::FxCityPulse,
        Self::LayerBackground,
        Self::LayerWorld,
        Self::LayerForeground,
        Self::LayerOverlay,
        Self::HueShift,
    ];

    /// Le chemin tel qu'écrit dans les fichiers (chemin du champ dans le fichier de look).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FlightRate => "gates.flight_rate",
            Self::StuntGapScale => "gates.stunt_gap_scale",
            Self::EventRate => "gates.event_rate",
            Self::MeteorRate => "gates.meteor_rate",
            Self::Cruise => "gates.cruise",
            Self::GroundTraffic => "gates.ground_traffic",
            Self::FxRgbSplit => "gates.fx.rgb_split",
            Self::FxFlash => "gates.fx.flash",
            Self::FxCityPulse => "gates.fx.city_pulse",
            Self::LayerBackground => "layers.background",
            Self::LayerWorld => "layers.world",
            Self::LayerForeground => "layers.foreground",
            Self::LayerOverlay => "layers.overlay",
            Self::HueShift => "hue_shift_deg",
        }
    }

    /// Plage permise pour ce paramètre (bornes incluses), dans son unité.
    ///
    /// Les bornes hautes laissent de la marge au-dessus du niveau 5 de Sillage (`flightRate` 1,6, `meteorRate` 9,
    /// `cruise` 1,15) et couvrent les portes ouvertes (`meteorRate` 14).
    #[must_use]
    pub const fn range(self) -> RangeInclusive<f32> {
        match self {
            Self::FlightRate | Self::EventRate => 0.0..=4.0,
            Self::StuntGapScale => 0.1..=4.0,
            Self::MeteorRate => 0.0..=60.0,
            Self::Cruise => 0.5..=2.0,
            Self::GroundTraffic
            | Self::FxRgbSplit
            | Self::FxFlash
            | Self::FxCityPulse
            | Self::LayerBackground
            | Self::LayerWorld
            | Self::LayerForeground
            | Self::LayerOverlay => 0.0..=1.0,
            Self::HueShift => -180.0..=180.0,
        }
    }

    /// Retrouve un paramètre par son chemin ; `None` s'il n'existe pas.
    #[must_use]
    pub fn parse(path: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|param| param.as_str() == path)
    }
}

impl fmt::Display for ParamPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Chemin de paramètre inconnu : le message liste les chemins valides.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("paramètre pilotable inconnu « {0} » ; chemins valides : {list}", list = valid_paths())]
pub struct UnknownParam(pub String);

fn valid_paths() -> String {
    ParamPath::ALL.map(ParamPath::as_str).join(", ")
}

impl TryFrom<String> for ParamPath {
    type Error = UnknownParam;

    fn try_from(path: String) -> Result<Self, Self::Error> {
        Self::parse(&path).ok_or(UnknownParam(path))
    }
}

impl From<ParamPath> for &'static str {
    fn from(param: ParamPath) -> Self {
        param.as_str()
    }
}

/// Un potard assignable : un nom et les paramètres qu'il fait varier ensemble.
///
/// Fichier : `Macro(name: "Vitesse", drives: [("gates.cruise", 0.9, 1.2)])`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Macro {
    /// Nom affiché sous le potard, 1 à [`MAX_MACRO_NAME`] caractères.
    pub name: String,
    /// Paramètres pilotés, 1 à [`MAX_DRIVES`], chacun au plus une fois.
    pub drives: Vec<MacroDrive>,
}

/// Ce qu'une macro fait d'un paramètre : sa valeur potard à 0 et potard à 1, interpolée linéairement entre les deux.
///
/// Fichier : un triplet `("gates.event_rate", 0.5, 2.0)`. `at_zero` peut dépasser `at_one` (potard inversé).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(from = "(ParamPath, f32, f32)", into = "(ParamPath, f32, f32)")]
pub struct MacroDrive {
    /// Paramètre piloté.
    pub param: ParamPath,
    /// Valeur absolue du paramètre potard à 0, dans la plage de [`ParamPath::range`].
    pub at_zero: f32,
    /// Valeur absolue du paramètre potard à 1, dans la plage de [`ParamPath::range`].
    pub at_one: f32,
}

impl MacroDrive {
    /// Valeur du paramètre pour une position de potard `knob` (0..1, bornée).
    #[must_use]
    pub fn value_at(&self, knob: f32) -> f32 {
        let t = knob.clamp(0.0, 1.0);
        self.at_zero + (self.at_one - self.at_zero) * t
    }
}

impl From<(ParamPath, f32, f32)> for MacroDrive {
    fn from((param, at_zero, at_one): (ParamPath, f32, f32)) -> Self {
        Self { param, at_zero, at_one }
    }
}

impl From<MacroDrive> for (ParamPath, f32, f32) {
    fn from(drive: MacroDrive) -> Self {
        (drive.param, drive.at_zero, drive.at_one)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_path_parses_back() {
        for param in ParamPath::ALL {
            assert_eq!(ParamPath::parse(param.as_str()), Some(param));
            assert!(param.range().start() < param.range().end());
        }
        assert_eq!(ParamPath::parse("gates.nope"), None);
    }

    #[test]
    fn unknown_path_lists_valid_ones() {
        let message = UnknownParam("gates.nope".to_owned()).to_string();
        assert!(message.contains("gates.nope"));
        assert!(message.contains("gates.cruise"));
    }

    #[test]
    fn knob_interpolates() {
        let drive = MacroDrive {
            param: ParamPath::Cruise,
            at_zero: 0.9,
            at_one: 1.2,
        };
        assert!((drive.value_at(0.5) - 1.05).abs() < 1e-6);
        assert!((drive.value_at(2.0) - 1.2).abs() < 1e-6);
    }
}
