//! Les palettes de présélection des DJs : dix ambiances cyberpunk assorties, chacune fixe d'un coup les couleurs de la
//! voiture, du quartier et du néon du nom (le thème du DJ).
//!
//! Elles s'ajoutent aux six thèmes livrés de Sillage (qui restent inchangés, comparés à `scene.css`) sous la forme de
//! thèmes personnalisés à cinq couleurs : le format RON des shows n'a pas à changer.

use super::event::{DjSlot, Event};
use super::ids::HexColor;
use super::look::{CustomTheme, Theme};

/// Une palette de présélection : son identifiant, son nom pour l'opérateur et ses cinq couleurs
/// (encre, velours, cyan, braise, glace).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DjPalette {
    /// Identifiant stable.
    pub id: &'static str,
    /// Nom affiché.
    pub label: &'static str,
    /// Encre, velours, cyan, braise, glace, au format `#rrggbb`.
    pub colors: [&'static str; 5],
}

impl DjPalette {
    /// Le thème de cette palette. `None` seulement si une couleur du tableau n'était pas `#rrggbb` (un test l'interdit).
    #[must_use]
    pub fn theme(&self) -> Option<Theme> {
        let [ink, velvet, cyan, ember, ice] = self.colors.map(|c| HexColor::new(c).ok());
        Some(Theme::Custom(CustomTheme {
            ink: ink?,
            velvet: velvet?,
            cyan: cyan?,
            ember: ember?,
            ice: ice?,
        }))
    }
}

/// Les dix palettes, dans l'ordre d'attribution automatique.
pub const DJ_PALETTES: [DjPalette; 10] = [
    DjPalette {
        id: "neon-tokyo",
        label: "Néon Tokyo",
        colors: ["#05060f", "#1a1038", "#00f0ff", "#ff2a6d", "#e8f9ff"],
    },
    DjPalette {
        id: "soleil-acide",
        label: "Soleil acide",
        colors: ["#0a0604", "#2a0f2e", "#ffb300", "#ff3d7f", "#fff3d6"],
    },
    DjPalette {
        id: "vert-toxique",
        label: "Vert toxique",
        colors: ["#020a06", "#0b2a1a", "#3dff7a", "#ff4fd8", "#eafff0"],
    },
    DjPalette {
        id: "ultraviolet",
        label: "Ultraviolet",
        colors: ["#08030f", "#25104d", "#b46bff", "#00e0ff", "#f3e8ff"],
    },
    DjPalette {
        id: "magma",
        label: "Magma",
        colors: ["#0a0303", "#2b0a0a", "#ff6a2a", "#ffd23f", "#fff0e0"],
    },
    DjPalette {
        id: "glace-electrique",
        label: "Glace électrique",
        colors: ["#030812", "#0c2144", "#52c8ff", "#ff5cf0", "#eef9ff"],
    },
    DjPalette {
        id: "rose-bonbon",
        label: "Rose bonbon",
        colors: ["#0d0309", "#3a0f2c", "#ff4fa3", "#3dffd1", "#fff0f8"],
    },
    DjPalette {
        id: "or-cyber",
        label: "Or cyber",
        colors: ["#090703", "#2a200a", "#ffd24a", "#25d9ff", "#fffbe6"],
    },
    DjPalette {
        id: "corail-et-mer",
        label: "Corail et mer",
        colors: ["#03090b", "#0a2a33", "#2ff0c8", "#ff6b5c", "#effffb"],
    },
    DjPalette {
        id: "sang-et-chrome",
        label: "Sang et chrome",
        colors: ["#0b0305", "#2d0a14", "#ff3355", "#9ad7ff", "#f6f9ff"],
    },
];

/// Donne un thème aux DJs qui n'en ont pas. Deux DJs consécutifs ne reçoivent jamais la même palette, et un thème choisi
/// à la main n'est jamais modifié ni répété par son voisin automatique. Déterministe : même liste, même résultat.
pub fn assign_themes(slots: &mut [DjSlot]) {
    for i in 0..slots.len() {
        if slots[i].dj.theme.is_some() {
            continue;
        }
        let previous = i.checked_sub(1).and_then(|p| slots[p].dj.theme.clone());
        let next = slots.get(i + 1).and_then(|s| s.dj.theme.clone());
        let chosen = (0..DJ_PALETTES.len())
            .map(|k| DJ_PALETTES[(i + k) % DJ_PALETTES.len()])
            .filter_map(|p| p.theme())
            .find(|theme| previous.as_ref() != Some(theme) && next.as_ref() != Some(theme));
        slots[i].dj.theme = chosen;
    }
}

impl Event {
    /// Donne un thème aux DJs sans thème (voir [`assign_themes`]).
    pub fn assign_dj_themes(&mut self) {
        assign_themes(&mut self.slots);
    }
}

#[cfg(test)]
#[allow(clippy::panic, clippy::indexing_slicing)] // Tests.
mod tests {
    use super::*;
    use crate::model::PresetTheme;

    fn luminance(hex: &str) -> f32 {
        let channel = |i: usize| {
            let v = f32::from(u8::from_str_radix(&hex[1 + 2 * i..3 + 2 * i], 16).unwrap_or(0)) / 255.0;
            if v <= 0.039_28 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(0) + 0.7152 * channel(1) + 0.0722 * channel(2)
    }

    fn contrast(a: &str, b: &str) -> f32 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    #[test]
    fn every_palette_is_valid_and_distinct() {
        let mut themes = Vec::new();
        for p in DJ_PALETTES {
            let theme = p.theme();
            assert!(theme.is_some(), "{}", p.id);
            themes.push(theme);
        }
        for (i, a) in themes.iter().enumerate() {
            assert!(themes[i + 1..].iter().all(|b| a != b), "palettes en double : {i}");
        }
    }

    #[test]
    fn every_palette_keeps_the_neon_readable_on_the_ink() {
        for p in DJ_PALETTES {
            let [ink, _, cyan, ember, ice] = p.colors;
            for neon in [cyan, ember, ice] {
                assert!(contrast(ink, neon) >= 4.5, "{} : {ink} / {neon}", p.id);
            }
        }
    }

    fn slot(theme: Option<Theme>) -> DjSlot {
        // Un créneau réel du show de démonstration : seul le thème nous intéresse.
        let event = crate::load::parse_event(include_str!("../../../../assets/shows/demo.ron"), "demo");
        let mut slot = match event {
            Ok(e) => e.slots[0].clone(),
            Err(e) => panic!("show de démonstration : {e}"),
        };
        slot.dj.theme = theme;
        slot
    }

    #[test]
    fn consecutive_automatic_palettes_never_repeat() {
        for n in 1..=12 {
            let mut slots: Vec<DjSlot> = (0..n).map(|_| slot(None)).collect();
            assign_themes(&mut slots);
            assert!(slots.iter().all(|s| s.dj.theme.is_some()));
            for pair in slots.windows(2) {
                assert_ne!(pair[0].dj.theme, pair[1].dj.theme);
            }
        }
    }

    #[test]
    fn a_manual_theme_is_never_overwritten_nor_repeated_by_its_automatic_neighbours() {
        let manual = Some(Theme::Preset(PresetTheme::Gold));
        let mut slots = vec![slot(None), slot(manual.clone()), slot(None)];
        assign_themes(&mut slots);
        assert_eq!(slots[1].dj.theme, manual);
        assert_ne!(slots[0].dj.theme, manual);
        assert_ne!(slots[2].dj.theme, manual);
        let same = DJ_PALETTES[0].theme();
        let mut slots = vec![slot(same.clone()), slot(None)];
        assign_themes(&mut slots);
        assert_ne!(
            slots[1].dj.theme, same,
            "jamais la même palette que le voisin choisi à la main"
        );
    }

    #[test]
    fn two_manual_identical_themes_are_left_alone() {
        let same = Some(Theme::Preset(PresetTheme::Gold));
        let mut slots = vec![slot(same.clone()), slot(same.clone())];
        assign_themes(&mut slots);
        assert_eq!(
            (slots[0].dj.theme.clone(), slots[1].dj.theme.clone()),
            (same.clone(), same)
        );
    }

    #[test]
    fn the_assignment_is_deterministic() {
        let build = || {
            let mut slots: Vec<DjSlot> = (0..7).map(|_| slot(None)).collect();
            assign_themes(&mut slots);
            slots.into_iter().map(|s| s.dj.theme).collect::<Vec<_>>()
        };
        assert_eq!(build(), build());
    }
}
