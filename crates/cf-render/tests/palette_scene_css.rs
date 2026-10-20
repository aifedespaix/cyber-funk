//! Les six thèmes livrés sont identiques à `docs/sillage/scene.css`, octet par octet (critère de J4-08).
//!
//! Le test relit le CSS lui-même et rejoue la cascade de Sillage : `:root` (Void Velvet) donne toutes les valeurs, le
//! bloc `[data-scene-theme="x"]` les remplace, et `mint` / `rose` sont les alias de `cyan` / `ember` du thème (la règle
//! `[data-scene-theme]` les redéclare pour que l'alias se résolve dans chaque thème).

#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;

use cf_render::palette::{PRESETS, SceneColor, ThemeLibrary, preset_name};
use cf_show::model::{CustomTheme, HexColor, PresetTheme, Theme};

const SCENE_CSS: &str = include_str!("../../../docs/sillage/scene.css");

type Blocks = BTreeMap<String, BTreeMap<String, String>>;

/// Propriétés `--scene-*` de chaque bloc : le sélecteur (`root`, `any`, ou le nom du thème) → nom → valeur brute.
fn parse_blocks(css: &str) -> Blocks {
    // Les commentaires peuvent contenir des accolades ou des `;` : on les retire d'abord.
    let mut clean = String::new();
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        clean.push_str(&rest[..start]);
        let end = rest[start..].find("*/").map_or(rest.len(), |e| start + e + 2);
        rest = &rest[end..];
    }
    clean.push_str(rest);

    let mut blocks = Blocks::new();
    for chunk in clean.split('}') {
        let Some((selectors, body)) = chunk.split_once('{') else {
            continue;
        };
        let props: Vec<(String, String)> = body
            .split(';')
            .filter_map(|decl| decl.split_once(':'))
            .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
            .filter(|(k, _)| k.starts_with("--scene-"))
            .collect();
        for selector in selectors.split(',').map(str::trim) {
            let key = match selector
                .strip_prefix("[data-scene-theme=\"")
                .and_then(|s| s.strip_suffix("\"]"))
            {
                Some(theme) => theme.to_owned(),
                None if selector == ":root" => "root".to_owned(),
                None if selector == "[data-scene-theme]" => "any".to_owned(),
                None => continue,
            };
            blocks.entry(key).or_default().extend(props.iter().cloned());
        }
    }
    blocks
}

/// Valeur résolue d'un rôle pour un thème, comme la lit `readScenePalette`.
fn resolve(blocks: &Blocks, theme: &str, role: &str) -> String {
    match role {
        // Alias redéclarés par `[data-scene-theme]` : ils pointent vers le cyan / la braise du thème.
        "mint" => resolve(blocks, theme, "cyan"),
        "rose" => resolve(blocks, theme, "ember"),
        _ => {
            let name = format!("--scene-{role}");
            blocks
                .get(theme)
                .and_then(|b| b.get(&name))
                .or_else(|| blocks.get("root").and_then(|b| b.get(&name)))
                .cloned()
                .unwrap_or_else(|| panic!("{name} introuvable pour {theme}"))
        }
    }
}

fn hex_bytes(hex: &str) -> [u8; 3] {
    assert!(hex.starts_with('#') && hex.len() == 7, "couleur inattendue : {hex}");
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
    [byte(1), byte(3), byte(5)]
}

#[test]
fn les_six_themes_sont_identiques_a_scene_css() {
    let blocks = parse_blocks(SCENE_CSS);
    let library = ThemeLibrary::builtin().unwrap();
    assert_eq!(PRESETS.len(), 6);
    let mut compared = 0;
    for preset in PRESETS {
        let name = preset_name(preset);
        assert!(blocks.contains_key(name), "bloc CSS absent : {name}");
        for role in SceneColor::ALL {
            let css = resolve(&blocks, name, role.name());
            let actual = library.preset(preset).get(role).to_srgb8();
            assert_eq!(
                actual,
                hex_bytes(&css),
                "thème {name}, rôle {} : palette {actual:?} contre CSS {css}",
                role.name()
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 66, "6 thèmes x 11 rôles");
}

#[test]
fn le_css_ne_declare_pas_d_autre_theme() {
    let blocks = parse_blocks(SCENE_CSS);
    let mut themes: Vec<&str> = blocks
        .keys()
        .map(String::as_str)
        .filter(|k| *k != "root" && *k != "any")
        .collect();
    themes.sort_unstable();
    let mut ours: Vec<&str> = PRESETS.iter().map(|p| preset_name(*p)).collect();
    ours.sort_unstable();
    assert_eq!(themes, ours);
}

#[test]
fn le_theme_perso_a_les_couleurs_choisies_et_le_reste_de_velvet() {
    let library = ThemeLibrary::builtin().unwrap();
    let hex = |t: &str| HexColor::new(t).unwrap();
    let custom = CustomTheme {
        ink: hex("#101010"),
        velvet: hex("#301050"),
        cyan: hex("#22ddaa"),
        ember: hex("#ee3366"),
        ice: hex("#f0f0ff"),
    };
    let palette = library.resolve(&Theme::Custom(custom));
    assert_eq!(palette.get(SceneColor::Mint).to_srgb8(), [0x22, 0xdd, 0xaa]);
    assert_eq!(palette.get(SceneColor::Rose).to_srgb8(), [0xee, 0x33, 0x66]);
    let velvet = library.resolve(&Theme::Preset(PresetTheme::Velvet));
    for role in [SceneColor::Alarm, SceneColor::Lime, SceneColor::Sun] {
        assert_eq!(palette.get(role), velvet.get(role), "{role:?}");
    }
}
