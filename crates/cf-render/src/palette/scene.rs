//! La palette de la scène (`ScenePalette` de `engine/palette.ts`) : onze rôles de couleur, les six thèmes livrés (lus
//! dans `themes.ron`) et le thème personnalisé (cinq couleurs, le reste en est dérivé : `customVariables` de
//! `settings.ts`).

use std::collections::BTreeMap;

use bevy::prelude::Resource;
use cf_show::model::{CustomTheme, PresetTheme, Theme};
use serde::Deserialize;

use super::color::{Hsl, LinearRgb, mix_srgb8, parse_hex};

/// Les thèmes livrés, en RON, compilés dans le binaire (rien à charger pendant un show).
const BUILTIN_THEMES: &str = include_str!("themes.ron");

/// Les six thèmes livrés, dans l'ordre de `PRESET_THEMES` de `settings.ts`.
pub const PRESETS: [PresetTheme; 6] = [
    PresetTheme::Velvet,
    PresetTheme::Synthwave,
    PresetTheme::Terminal,
    PresetTheme::Aurora,
    PresetTheme::Gold,
    PresetTheme::Frost,
];

/// Nom d'un thème livré dans `themes.ron` et dans `scene.css` (`data-scene-theme`).
#[must_use]
pub const fn preset_name(preset: PresetTheme) -> &'static str {
    match preset {
        PresetTheme::Velvet => "velvet",
        PresetTheme::Synthwave => "synthwave",
        PresetTheme::Terminal => "terminal",
        PresetTheme::Aurora => "aurora",
        PresetTheme::Gold => "gold",
        PresetTheme::Frost => "frost",
    }
}

/// Les rôles de couleur de la scène (`SCENE_COLOR_NAMES` de `palette.ts`, `--scene-*` de `scene.css`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneColor {
    /// Noir mat du fond.
    Ink,
    /// Violet nuit.
    Velvet,
    /// Le cyan du thème : ce qui se ramasse.
    Mint,
    /// Bleu clair secondaire.
    Azure,
    /// Violet éclairé : les lueurs sur le noir.
    Violet,
    /// Le corail du thème : ce qui se fuit.
    Rose,
    /// Ambre.
    Amber,
    /// Glace : les éclats les plus clairs.
    Ice,
    /// Gyrophares : le seul rouge franc de la scène.
    Alarm,
    /// Vert terminal des immeubles-vitrines.
    Lime,
    /// Jaune soleil (smiley, lave).
    Sun,
}

impl SceneColor {
    /// Tous les rôles, dans l'ordre de `SCENE_COLOR_NAMES`.
    pub const ALL: [Self; 11] = [
        Self::Ink,
        Self::Velvet,
        Self::Mint,
        Self::Azure,
        Self::Violet,
        Self::Rose,
        Self::Amber,
        Self::Ice,
        Self::Alarm,
        Self::Lime,
        Self::Sun,
    ];

    /// Nom du rôle, celui de `--scene-<nom>`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ink => "ink",
            Self::Velvet => "velvet",
            Self::Mint => "mint",
            Self::Azure => "azure",
            Self::Violet => "violet",
            Self::Rose => "rose",
            Self::Amber => "amber",
            Self::Ice => "ice",
            Self::Alarm => "alarm",
            Self::Lime => "lime",
            Self::Sun => "sun",
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// La palette à l'écran : une couleur par rôle, en lumière linéaire.
///
/// Écrite par le seul système du module (`CfSet::Present`) : le fondu de thème, la teinte du look et le thème du show
/// y sont déjà appliqués. Les autres modules la lisent (`Res<ScenePalette>`) et recalculent leurs couleurs dérivées à
/// chaque image où elle change (`Res::is_changed`) : il n'y a pas d'abonnements `derived`/`watchPalette` comme dans
/// Sillage.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct ScenePalette {
    colors: [LinearRgb; 11],
}

impl ScenePalette {
    /// La couleur d'un rôle.
    #[must_use]
    pub fn get(&self, color: SceneColor) -> LinearRgb {
        self.colors[color.index()]
    }

    /// Change la couleur d'un rôle.
    pub fn set(&mut self, color: SceneColor, value: LinearRgb) {
        self.colors[color.index()] = value;
    }

    /// Cycle lumineux qui répartit les teintes : cyan dominant, un peu de violet, de corail et de glace
    /// (`prismaticCycle` de `palette.ts`).
    #[must_use]
    pub fn prismatic_cycle(&self) -> [LinearRgb; 6] {
        [
            self.get(SceneColor::Mint),
            self.get(SceneColor::Azure),
            self.get(SceneColor::Violet),
            self.get(SceneColor::Mint),
            self.get(SceneColor::Rose),
            self.get(SceneColor::Ice),
        ]
    }

    /// La même palette avec toutes les teintes tournées de `degrees` (la rotation de teinte du look,
    /// `ShowState::hue_shift_deg`). Rotation nulle : la palette est rendue telle quelle, à l'octet près.
    #[must_use]
    pub fn hue_rotated(&self, degrees: f32) -> Self {
        if !degrees.is_finite() || degrees.abs() < 1e-3 {
            return *self;
        }
        let turn = degrees / 360.0;
        let mut out = *self;
        for color in &mut out.colors {
            let Hsl { h, s, l } = color.to_hsl();
            *color = LinearRgb::from_hsl(Hsl { h: h + turn, s, l });
        }
        out
    }
}

/// Un thème illisible.
#[derive(Debug, thiserror::Error)]
pub enum PaletteError {
    /// Le RON des thèmes livrés ne se lit pas.
    #[error("themes.ron illisible : {0}")]
    Ron(#[from] ron::error::SpannedError),
    /// Une couleur n'est pas au format `#rrggbb`.
    #[error("thème « {theme} », rôle « {role} » : couleur invalide « {value} »")]
    BadColor {
        /// Thème fautif.
        theme: String,
        /// Rôle fautif.
        role: &'static str,
        /// Texte lu.
        value: String,
    },
    /// Un thème livré manque dans le fichier.
    #[error("thème livré « {0} » absent de themes.ron")]
    MissingTheme(&'static str),
}

/// Les onze couleurs d'un thème, telles qu'écrites dans `themes.ron`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeColors {
    ink: String,
    velvet: String,
    mint: String,
    azure: String,
    violet: String,
    rose: String,
    amber: String,
    ice: String,
    alarm: String,
    lime: String,
    sun: String,
}

impl ThemeColors {
    fn into_palette(self, theme: &str) -> Result<ScenePalette, PaletteError> {
        let mut palette = ScenePalette::default();
        let entries = [
            (SceneColor::Ink, &self.ink),
            (SceneColor::Velvet, &self.velvet),
            (SceneColor::Mint, &self.mint),
            (SceneColor::Azure, &self.azure),
            (SceneColor::Violet, &self.violet),
            (SceneColor::Rose, &self.rose),
            (SceneColor::Amber, &self.amber),
            (SceneColor::Ice, &self.ice),
            (SceneColor::Alarm, &self.alarm),
            (SceneColor::Lime, &self.lime),
            (SceneColor::Sun, &self.sun),
        ];
        for (role, value) in entries {
            let color = LinearRgb::from_hex(value).ok_or_else(|| PaletteError::BadColor {
                theme: theme.to_owned(),
                role: role.name(),
                value: value.clone(),
            })?;
            palette.set(role, color);
        }
        Ok(palette)
    }
}

/// Les six thèmes livrés, lus une fois à l'ouverture.
#[derive(Resource, Debug, Clone, PartialEq, Default)]
pub struct ThemeLibrary {
    presets: [ScenePalette; 6],
}

impl ThemeLibrary {
    /// Les thèmes compilés dans le binaire (`themes.ron`).
    ///
    /// # Errors
    ///
    /// Le fichier est illisible, une couleur est invalide ou un des six thèmes manque.
    pub fn builtin() -> Result<Self, PaletteError> {
        Self::from_ron(BUILTIN_THEMES)
    }

    /// Les thèmes d'un texte RON (même format que `themes.ron`).
    ///
    /// # Errors
    ///
    /// Voir [`ThemeLibrary::builtin`].
    pub fn from_ron(text: &str) -> Result<Self, PaletteError> {
        let mut table: BTreeMap<String, ThemeColors> = ron::from_str(text)?;
        let mut presets = [ScenePalette::default(); 6];
        for (slot, preset) in presets.iter_mut().zip(PRESETS) {
            let name = preset_name(preset);
            let colors = table.remove(name).ok_or(PaletteError::MissingTheme(name))?;
            *slot = colors.into_palette(name)?;
        }
        Ok(Self { presets })
    }

    /// La palette d'un thème livré.
    #[must_use]
    pub fn preset(&self, preset: PresetTheme) -> &ScenePalette {
        let index = PRESETS.iter().position(|p| *p == preset).unwrap_or(0);
        &self.presets[index]
    }

    /// La palette d'un thème de show (livré ou personnalisé).
    #[must_use]
    pub fn resolve(&self, theme: &Theme) -> ScenePalette {
        match theme {
            Theme::Preset(preset) => *self.preset(*preset),
            Theme::Custom(custom) => self.custom(custom),
        }
    }

    /// La palette d'un thème personnalisé : les cinq couleurs choisies, le reste dérivé (`customVariables`).
    ///
    /// Dans Sillage, le thème « custom » n'a pas de bloc CSS : ce que `customVariables` ne pose pas (`alarm`, `lime`,
    /// `sun`) vient de `:root`, c'est-à-dire de Void Velvet ; `mint` et `rose` sont les alias de `cyan` et `ember`.
    #[must_use]
    pub fn custom(&self, custom: &CustomTheme) -> ScenePalette {
        let velvet_theme = self.preset(PresetTheme::Velvet);
        let mut palette = *velvet_theme;
        let bytes = |hex: &str| parse_hex(hex).unwrap_or([0, 0, 0]);
        let (ink, velvet, cyan, ember, ice) = (
            bytes(custom.ink.as_str()),
            bytes(custom.velvet.as_str()),
            bytes(custom.cyan.as_str()),
            bytes(custom.ember.as_str()),
            bytes(custom.ice.as_str()),
        );
        let set = |palette: &mut ScenePalette, role, value| palette.set(role, LinearRgb::from_srgb8(value));
        set(&mut palette, SceneColor::Ink, ink);
        set(&mut palette, SceneColor::Velvet, velvet);
        set(&mut palette, SceneColor::Mint, cyan);
        set(&mut palette, SceneColor::Rose, ember);
        set(&mut palette, SceneColor::Ice, ice);
        set(&mut palette, SceneColor::Azure, mix_srgb8(cyan, ink, 0.3));
        set(&mut palette, SceneColor::Violet, mix_srgb8(velvet, ice, 0.35));
        set(&mut palette, SceneColor::Amber, mix_srgb8(ember, ice, 0.4));
        palette
    }
}

#[cfg(test)]
mod tests {
    use cf_show::model::HexColor;

    use super::*;

    fn hex(text: &str) -> HexColor {
        HexColor::new(text).unwrap()
    }

    #[test]
    fn les_six_themes_livres_se_lisent() {
        let library = ThemeLibrary::builtin().unwrap();
        let velvet = library.preset(PresetTheme::Velvet);
        assert_eq!(velvet.get(SceneColor::Mint).to_srgb8(), [0x00, 0xe5, 0xff]);
        assert_ne!(library.preset(PresetTheme::Gold), velvet);
    }

    #[test]
    fn un_theme_manquant_est_une_erreur() {
        assert!(matches!(
            ThemeLibrary::from_ron("{}"),
            Err(PaletteError::MissingTheme("velvet"))
        ));
    }

    #[test]
    fn une_couleur_invalide_est_une_erreur() {
        let text = BUILTIN_THEMES.replace("#050508", "#05050");
        assert!(matches!(
            ThemeLibrary::from_ron(&text),
            Err(PaletteError::BadColor { role: "ink", .. })
        ));
    }

    #[test]
    fn le_theme_perso_derive_comme_settings_ts() {
        let library = ThemeLibrary::builtin().unwrap();
        let custom = CustomTheme {
            ink: hex("#000000"),
            velvet: hex("#200040"),
            cyan: hex("#00ff00"),
            ember: hex("#ff0000"),
            ice: hex("#ffffff"),
        };
        let palette = library.custom(&custom);
        let srgb = |role| palette.get(role).to_srgb8();
        assert_eq!(srgb(SceneColor::Mint), [0, 255, 0]);
        assert_eq!(srgb(SceneColor::Rose), [255, 0, 0]);
        // mixHex(cyan, ink, 0.3) : 255 * 0,7 = 178,5 -> 179 (Math.round).
        assert_eq!(srgb(SceneColor::Azure), [0, 179, 0]);
        // mixHex(velvet, ice, 0.35) : 32 + 223 * 0,35 = 110,05 -> 110, 0 + 255 * 0,35 = 89,25 -> 89, 64 + 191 * 0,35.
        assert_eq!(srgb(SceneColor::Violet), [110, 89, 131]);
        // mixHex(ember, ice, 0.4) : 255, 102, 102.
        assert_eq!(srgb(SceneColor::Amber), [255, 102, 102]);
        // Non dérivés : Void Velvet.
        assert_eq!(srgb(SceneColor::Alarm), [0xff, 0x2e, 0x1f]);
        assert_eq!(srgb(SceneColor::Sun), [0xff, 0xd2, 0x3f]);
    }

    #[test]
    fn rotation_nulle_rend_la_palette_telle_quelle() {
        let library = ThemeLibrary::builtin().unwrap();
        let palette = *library.preset(PresetTheme::Aurora);
        assert_eq!(palette.hue_rotated(0.0), palette);
        assert_ne!(palette.hue_rotated(90.0), palette);
        // Un tour complet ramène aux mêmes octets.
        let full = palette.hue_rotated(360.0 - 1e-2);
        for role in SceneColor::ALL {
            let (a, b) = (full.get(role).to_srgb8(), palette.get(role).to_srgb8());
            assert!(a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 1), "{role:?}");
        }
    }
}
