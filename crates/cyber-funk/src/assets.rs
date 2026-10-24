//! Où trouver `assets/` : indépendamment du dossier de travail (J8-07).
//!
//! Le binaire installé (cargo-packager) se lance depuis n'importe où (Finder, raccourci, service) : un chemin relatif
//! au dossier de travail ne retrouverait ni la voiture, ni les looks, ni les musiques. On cherche donc d'abord à côté
//! de l'exécutable. `--assets-dir` (et `--music-dir`) restent prioritaires : ils valent tels quels.

use std::path::{Path, PathBuf};

/// Nom du dossier des ressources.
const ASSETS: &str = "assets";

/// Dossier des ressources quand `--assets-dir` n'est pas donné.
///
/// Ordre : à côté de l'exécutable, `Contents/Resources` d'un bundle macOS, le dépôt (builds de développement
/// seulement), puis `assets/` relatif au dossier de travail (comportement historique, dernier recours).
#[must_use]
pub fn default_assets_dir() -> PathBuf {
    let exe = std::env::current_exe().ok();
    // Le chemin du dépôt est figé à la compilation : il ne doit jamais servir à un binaire livré.
    let dev_root = cfg!(debug_assertions).then(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"));
    locate(exe.as_deref(), dev_root.as_deref())
}

/// Dossier des musiques de démo quand `--music-dir` n'est pas donné.
#[must_use]
pub fn default_music_dir() -> PathBuf {
    default_assets_dir().join("music")
}

/// Premier candidat qui existe ; sinon `assets` relatif au dossier de travail.
fn locate(exe: Option<&Path>, dev_root: Option<&Path>) -> PathBuf {
    let mut candidates = Vec::new();
    if let Some(dir) = exe.and_then(Path::parent) {
        candidates.push(dir.join(ASSETS));
        // `Foo.app/Contents/MacOS/cyber-funk` → `Foo.app/Contents/Resources/assets`.
        candidates.push(dir.join("../Resources").join(ASSETS));
    }
    candidates.extend(dev_root.map(Path::to_path_buf));
    candidates
        .into_iter()
        .find(|path| path.is_dir())
        .unwrap_or_else(|| PathBuf::from(ASSETS))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn l_exe_trouve_ses_assets_a_cote_de_lui() {
        let root = tempfile::tempdir().expect("dossier temporaire");
        fs::create_dir_all(root.path().join("bin/assets")).expect("assets");
        let exe = root.path().join("bin/cyber-funk");
        assert_eq!(locate(Some(&exe), None), root.path().join("bin/assets"));
    }

    #[test]
    fn un_bundle_macos_cherche_dans_resources() {
        let root = tempfile::tempdir().expect("dossier temporaire");
        fs::create_dir_all(root.path().join("Contents/MacOS")).expect("macos");
        fs::create_dir_all(root.path().join("Contents/Resources/assets")).expect("resources");
        let exe = root.path().join("Contents/MacOS/cyber-funk");
        let found = locate(Some(&exe), None);
        assert!(found.is_dir());
        assert!(found.ends_with("Resources/assets"), "{}", found.display());
    }

    #[test]
    fn le_depot_sert_en_developpement_et_le_dossier_de_travail_en_dernier_recours() {
        let root = tempfile::tempdir().expect("dossier temporaire");
        let repo = root.path().join("repo-assets");
        fs::create_dir_all(&repo).expect("dépôt");
        let exe = root.path().join("target/debug/cyber-funk");
        assert_eq!(locate(Some(&exe), Some(&repo)), repo);
        assert_eq!(locate(Some(&exe), None), PathBuf::from("assets"));
    }

    #[test]
    fn les_assets_se_trouvent_depuis_un_autre_dossier_de_travail() {
        // Le dossier de travail du test est celui du crate (pas la racine du dépôt) : le défaut ne doit pas en dépendre.
        let found = default_assets_dir();
        assert!(found.join("shows/demo.ron").is_file(), "{}", found.display());
        assert!(default_music_dir().is_dir());
    }
}
