//! Les GLB des motos (avec leur pilote figé) et du conducteur de la voiture se lisent avec le lecteur du projet
//! (`car::Gltf` : meshopt, WebP) et portent le matériau `body_paint` avec son masque de peinture.
//!
//! Ils sont produits par `tools/char/build-riders.sh`. Sans écran : seul le contenu du fichier est vérifié.

#![allow(clippy::expect_used, clippy::panic)]

use std::path::Path;

use cf_world_neon::car::Gltf;

fn read(relative: &str) -> Gltf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/models")
        .join(relative);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("lecture de {}: {e}", path.display()));
    Gltf::from_glb(&bytes).unwrap_or_else(|e| panic!("{relative} illisible : {e}"))
}

/// Le pilote est la seule partie qui porte `body_paint` ; son masque est la texture d'occlusion.
fn assert_rider_paint(gltf: &Gltf, name: &str) {
    let paint = gltf
        .materials()
        .iter()
        .find(|m| m.name == "body_paint")
        .unwrap_or_else(|| panic!("{name} : pas de matériau body_paint (pilote absent ?)"));
    assert!(
        paint.occlusion_texture.is_some(),
        "{name} : le masque de peinture manque"
    );
}

#[test]
fn les_motos_portent_leur_pilote() {
    for moto in 1..=3 {
        for quality in ["high", "low"] {
            let name = format!("motos/moto-{moto}-{quality}.glb");
            let gltf = read(&name);
            assert!(!gltf.meshes().is_empty(), "{name} : aucun maillage");
            assert_rider_paint(&gltf, &name);
            // Au moins une carrosserie texturée en plus du pilote.
            assert!(gltf.materials().len() >= 3, "{name} : trop peu de matériaux");
        }
    }
}

#[test]
fn le_conducteur_de_la_voiture_ne_porte_que_le_pilote() {
    let gltf = read("characters/driver-car.glb");
    assert_eq!(gltf.materials().len(), 1, "un seul matériau, celui du pilote");
    assert_rider_paint(&gltf, "driver-car.glb");
}
