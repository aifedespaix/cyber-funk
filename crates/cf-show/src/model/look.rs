//! Le look : ce qu'était un « acte » de Sillage (`engine/show/acts.ts`), mais en données éditables.
//!
//! Un look ne scénarise pas l'image : il règle les portes à l'intérieur desquelles le réalisateur improvise (plans,
//! cascades, événements, astéroïdes), plus les couches, la palette et les macros. Les nombres glissent d'un look à
//! l'autre pendant l'entrée d'un cue ; les listes et booléens basculent d'un coup (voir `GateBlender` de Sillage).

use serde::{Deserialize, Serialize};

use super::ids::{EventName, HexColor, LookId, ShotId, ShotTier, StuntKind, StuntTier, WorldId};
use super::params::{Macro, ParamPath};

/// Étage de vol le plus haut que Sillage connaisse (`maxFloor` 6 : tous les étages).
pub const MAX_FLOOR: u8 = 6;
/// Plus grande grille de coupe permise, en temps musicaux (Sillage monte à 16 ; 64 = quatre phrases).
pub const MAX_CUT_BEATS: u8 = 64;
/// Longueur maximale du nom affiché d'un look, en caractères.
pub const MAX_LOOK_NAME: usize = 60;

/// Un look, tel qu'écrit dans `assets/looks/<id>.ron` ou embarqué dans un événement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Look {
    /// Version du format ; vaut [`super::FORMAT_VERSION`] une fois le fichier chargé (les anciennes versions sont
    /// migrées à la lecture).
    pub version: u32,
    /// Identifiant stable, référencé par les cues. Pour un fichier de la bibliothèque, égal au nom du fichier.
    pub id: LookId,
    /// Nom affiché dans la fenêtre opérateur, 1 à [`MAX_LOOK_NAME`] caractères.
    pub name: String,
    /// Monde 3D où le look se joue (`"neon"`).
    pub world: WorldId,
    /// Opacité de chaque couche de rendu.
    pub layers: LayerMix,
    /// D'où vient la palette de couleurs.
    pub palette: PaletteRef,
    /// Rotation de teinte appliquée à la palette, en degrés, -180 à 180 (0 : palette telle quelle).
    pub hue_shift_deg: f32,
    /// Ce que le look autorise au réalisateur (champs de `ShowGates` de Sillage).
    pub gates: ShowGates,
    /// Potards assignables, 0 à [`super::params::MAX_MACROS`]. Absent du fichier : aucune macro.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub macros: Vec<Macro>,
}

impl Look {
    /// Valeur courante d'un paramètre pilotable.
    #[must_use]
    pub fn param(&self, param: ParamPath) -> f32 {
        match param {
            ParamPath::FlightRate => self.gates.flight_rate,
            ParamPath::StuntGapScale => self.gates.stunt_gap_scale,
            ParamPath::EventRate => self.gates.event_rate,
            ParamPath::MeteorRate => self.gates.meteor_rate,
            ParamPath::Cruise => self.gates.cruise,
            ParamPath::GroundTraffic => self.gates.ground_traffic,
            ParamPath::FxRgbSplit => self.gates.fx.rgb_split,
            ParamPath::FxFlash => self.gates.fx.flash,
            ParamPath::FxCityPulse => self.gates.fx.city_pulse,
            ParamPath::LayerBackground => self.layers.background,
            ParamPath::LayerWorld => self.layers.world,
            ParamPath::LayerForeground => self.layers.foreground,
            ParamPath::LayerOverlay => self.layers.overlay,
            ParamPath::HueShift => self.hue_shift_deg,
        }
    }

    /// Accès en écriture à un paramètre pilotable (macros, interface). Aucune borne n'est appliquée ici.
    pub fn param_mut(&mut self, param: ParamPath) -> &mut f32 {
        match param {
            ParamPath::FlightRate => &mut self.gates.flight_rate,
            ParamPath::StuntGapScale => &mut self.gates.stunt_gap_scale,
            ParamPath::EventRate => &mut self.gates.event_rate,
            ParamPath::MeteorRate => &mut self.gates.meteor_rate,
            ParamPath::Cruise => &mut self.gates.cruise,
            ParamPath::GroundTraffic => &mut self.gates.ground_traffic,
            ParamPath::FxRgbSplit => &mut self.gates.fx.rgb_split,
            ParamPath::FxFlash => &mut self.gates.fx.flash,
            ParamPath::FxCityPulse => &mut self.gates.fx.city_pulse,
            ParamPath::LayerBackground => &mut self.layers.background,
            ParamPath::LayerWorld => &mut self.layers.world,
            ParamPath::LayerForeground => &mut self.layers.foreground,
            ParamPath::LayerOverlay => &mut self.layers.overlay,
            ParamPath::HueShift => &mut self.hue_shift_deg,
        }
    }
}

/// Ce que le look autorise : mêmes champs que `ShowGates` de Sillage (`engine/show/types.ts`), en `snake_case`.
///
/// Les listes sont des listes blanches : vide = rien de permis dans cette famille.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "mêmes portes indépendantes que `ShowGates` de Sillage"
)]
pub struct ShowGates {
    /// Étage de vol le plus haut permis, 0 (jamais de vol) à [`MAX_FLOOR`] (tous).
    pub max_floor: u8,
    /// Multiplicateur de la fréquence des envols décidés par le pilote, 0 (jamais) à 4.
    pub flight_rate: f32,
    /// Les envols n'ont lieu que sur un drop.
    pub flight_on_drop_only: bool,
    /// Figures en vol permises (tonneaux, saltos).
    pub tricks: bool,
    /// Rangs de cascades permis.
    pub stunt_tiers: Vec<StuntTier>,
    /// Liste blanche des cascades (l'Ascension a sa propre porte : `ascension`).
    pub stunt_kinds: Vec<StuntKind>,
    /// Multiplicateur de l'attente entre deux cascades, 0,1 à 4 : < 1 en enchaîne plus vite.
    pub stunt_gap_scale: f32,
    /// Rangs de plans caméra permis.
    pub shot_tiers: Vec<ShotTier>,
    /// Plans interdits (ids du catalogue caméra).
    pub shot_block: Vec<ShotId>,
    /// Temps musicaux entre deux coupes de caméra, selon la section du morceau.
    pub cut_grid: CutGrid,
    /// Salves de coupes rapides sur les drops.
    pub salvo: bool,
    /// Liste blanche des événements du trafic.
    pub events: Vec<EventName>,
    /// Multiplicateur de la fréquence des événements, 0 à 4.
    pub event_rate: f32,
    /// Lancers d'astéroïdes par minute, 0 à 60 (0 aux niveaux 0 à 2 de Sillage, ≈ 14 portes ouvertes).
    pub meteor_rate: f32,
    /// Pluies d'astéroïdes permises.
    pub meteor_shower: bool,
    /// Multiplicateur d'allure, 0,5 à 2 (Sillage : 1 → 1,15).
    pub cruise: f32,
    /// Densité du trafic au sol, 0 à 1.
    pub ground_traffic: f32,
    /// Effets dosés par le look.
    pub fx: FxGates,
    /// L'Ascension (cascade du niveau 5 de Sillage) est permise.
    pub ascension: bool,
}

/// Combien de temps musicaux entre deux coupes de caméra, selon la section du morceau (`CutGrid` de Sillage).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CutGrid {
    /// Section calme, en temps : 1 à [`MAX_CUT_BEATS`].
    pub calm: u8,
    /// Section groove, en temps : 1 à [`MAX_CUT_BEATS`].
    pub groove: u8,
    /// Section pic (après un drop), en temps : 1 à [`MAX_CUT_BEATS`].
    pub peak: u8,
}

/// Effets dosés par le look, de 0 (absent) à 1 (plein) : `ShowFxGates` de Sillage.
///
/// Le plafond photosensible (WCAG 2.3.1) ne dépend pas de ces valeurs : le limiteur s'applique en dernier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FxGates {
    /// Décalage RVB, 0 à 1.
    pub rgb_split: f32,
    /// Flashs, 0 à 1 (demande, plafonnée ensuite par le limiteur).
    pub flash: f32,
    /// Pulsation de la ville, 0 à 1.
    pub city_pulse: f32,
}

/// Opacité de chaque couche de rendu (fond, monde, avant-plan, surcouche 2D), de 0 (éteinte) à 1 (pleine).
///
/// Une couche à 0 ne coûte rien au GPU : sa caméra est désactivée (J4-01).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerMix {
    /// Fond (ciel, planètes, soleil), 0 à 1.
    pub background: f32,
    /// Monde (ville, voiture, trafic), 0 à 1.
    pub world: f32,
    /// Avant-plan 3D qui ne passe jamais derrière le monde (logo du DJ), 0 à 1.
    pub foreground: f32,
    /// Surcouche 2D (cartons-titres), 0 à 1.
    pub overlay: f32,
}

/// D'où un look tire sa palette de couleurs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaletteRef {
    /// Garder la palette déjà à l'écran (aucun fondu de palette à l'entrée du cue).
    Inherit,
    /// Le thème du DJ du créneau ; s'il n'en a pas, celui de l'événement.
    Dj,
    /// Un des six thèmes livrés (ceux de Sillage).
    Preset(PresetTheme),
    /// Cinq couleurs choisies, le reste de la palette en est dérivé.
    Custom(CustomTheme),
}

/// Un thème complet, pour un DJ ou pour l'événement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    /// Un des six thèmes livrés.
    Preset(PresetTheme),
    /// Cinq couleurs choisies.
    Custom(CustomTheme),
}

/// Les six thèmes livrés, ceux de Sillage (`PRESET_THEMES` de `settings.ts`, couleurs dans `docs/sillage/scene.css`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PresetTheme {
    /// Velours violet (thème par défaut de Sillage).
    Velvet,
    /// Synthwave rose et cyan.
    Synthwave,
    /// Terminal vert.
    Terminal,
    /// Aurore boréale.
    Aurora,
    /// Or.
    Gold,
    /// Givre.
    Frost,
}

/// Thème personnalisé : les cinq couleurs que l'on règle soi-même (`CustomTheme` de Sillage), au format `#rrggbb`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomTheme {
    /// Encre : le fond le plus sombre.
    pub ink: HexColor,
    /// Couleur principale (velours).
    pub velvet: HexColor,
    /// Cyan : néons froids.
    pub cyan: HexColor,
    /// Braise : néons chauds.
    pub ember: HexColor,
    /// Glace : les éclats les plus clairs.
    pub ice: HexColor,
}
