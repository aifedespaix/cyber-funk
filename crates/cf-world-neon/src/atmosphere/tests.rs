//! Tests de l'atmosphère qui croisent le shader et son miroir Rust, et le plugin sans rendu.

use bevy::prelude::*;

use super::light::ATMOSPHERE_WESL;
use super::{AtmospherePlugin, FogReach, SceneLightUniform, formula};

/// Lit `const NAME: f32 = valeur;` dans le texte WESL.
fn wesl_const(name: &str) -> f32 {
    let key = format!("const {name}: f32 = ");
    let at = ATMOSPHERE_WESL.find(&key).map_or(0, |i| i + key.len());
    let rest = &ATMOSPHERE_WESL[at..];
    let end = rest.find(';').unwrap_or(0);
    rest[..end].trim().parse().unwrap_or(f32::NAN)
}

#[test]
fn les_constantes_du_shader_sont_celles_du_miroir_rust() {
    for (name, rust) in [
        ("ATMO_U_SERIES", formula::U_SERIES),
        ("ATMO_U_MAX", formula::U_MAX),
        ("ATMO_DEPTH_BELOW_MAX", formula::DEPTH_BELOW_MAX),
        ("ATMO_TUBE_RADIUS_M", formula::TUBE_RADIUS_M),
        ("ATMO_SHELL_FLOOR", formula::SHELL_FLOOR),
    ] {
        assert!((wesl_const(name) - rust).abs() < 1e-9, "{name} : shader {}, Rust {rust}", wesl_const(name));
    }
}

#[test]
fn le_bloc_du_shader_suit_l_ordre_des_uniformes() {
    let fields = [
        "dir_key", "c_key", "c_fill", "dir_rim", "c_rim", "fog", "c_fog", "rayleigh", "kick_age", "shell", "source",
        "tube_pos", "tube_col",
    ];
    let block_end = ATMOSPHERE_WESL.rfind("@binding(9)").unwrap_or(0);
    let block = &ATMOSPHERE_WESL[..block_end];
    let mut last = 0;
    for field in fields {
        let at = block.find(&format!("    {field}: ")).unwrap_or(0);
        assert!(at > last, "champ `{field}` absent ou hors d'ordre dans `SceneLight`");
        last = at;
    }
}

#[test]
fn le_shader_n_importe_rien_et_ne_fait_aucune_passe() {
    // Ajouté au bout d'autres shaders : un `import` ici serait après du code, ce que WESL refuse.
    assert!(!ATMOSPHERE_WESL.lines().any(|l| l.trim_start().starts_with("import ")));
    // Brouillard par fragment : ni texture ni échantillonneur (aucune cible lue, donc aucune passe).
    assert!(!ATMOSPHERE_WESL.contains("textureSample") && !ATMOSPHERE_WESL.contains("@fragment"));
}

#[test]
fn tous_les_shaders_du_monde_appellent_l_atmosphere() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/shaders/cf-world-neon/");
    for (file, call) in [
        ("facade.wesl", "atmo_fog("),
        ("road.wesl", "atmo_fog("),
        ("traffic_vehicle.wesl", "atmo_fog("),
        ("landmark_common.wesl", "atmo_fog("),
        ("sign.wesl", "atmo_fade("),
    ] {
        let text = std::fs::read_to_string(format!("{dir}{file}")).unwrap_or_default();
        assert!(text.contains(call), "{file} n'appelle pas `{call}`");
    }
}

#[test]
fn sans_rendu_le_plugin_publie_un_bloc_neutre_et_la_portee_de_la_brume() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin::default())
        .init_asset::<crate::facade::FacadeMaterial>()
        .add_plugins(AtmospherePlugin);
    app.update();
    // Sans palette ni origine flottante, rien n'est composé : le bloc reste à zéro (brouillard d'avant).
    assert!(app.world().resource::<SceneLightUniform>().0.fog.w.abs() < f32::EPSILON);
    assert!((app.world().resource::<FogReach>().0 - 1057.3).abs() < 2.0);
}
