//! Les écrans branchés, tels que l'interface les lit, et les ordres qu'elle peut donner à la fenêtre projecteur.
//!
//! Comme pour les quartiers (`district`), les types vivent ici et non dans le binaire qui les remplit et les lit : la
//! fenêtre opérateur (`cf-operator`) ne connaît pas le code du projecteur. Le binaire (`projector`) est le seul
//! écrivain de [`ScreenList`] et le seul lecteur de [`ScreenOrder`].

use bevy::prelude::*;
use bevy::window::Monitor;

/// Ordre de l'interface à la fenêtre projecteur. La fenêtre est déplacée, jamais recréée ; la fenêtre opérateur ne
/// bouge pas.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub enum ScreenOrder {
    /// Mettre le projecteur sur l'écran de ce nom (comparaison sans la casse, comme `--projector-monitor`).
    MoveProjector(String),
    /// Plein écran (`true`) ou fenêtre ordinaire.
    SetFullscreen(bool),
    /// Afficher le numéro de chaque écran pendant quelques secondes (pas encore disponible : il faudrait une fenêtre
    /// par écran).
    Identify,
}

/// Un écran, copie lisible pour l'interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenInfo {
    /// Nom donné par le système ; `None` : écran anonyme (choisissable seulement par son numéro, en ligne de commande).
    pub name: Option<String>,
    /// Largeur en pixels physiques.
    pub width: u32,
    /// Hauteur en pixels physiques.
    pub height: u32,
    /// Fréquence en millihertz, si le système la donne.
    pub refresh_mhz: Option<u32>,
    /// Écran principal du système (au plus un dans la liste).
    pub primary: bool,
    /// Écran qui porte le projecteur en ce moment (au plus un dans la liste).
    pub projector: bool,
}

/// Les écrans détectés et l'état réel du projecteur. Réécrite par le binaire seulement quand un écran apparaît,
/// disparaît, ou que le projecteur change d'écran ou de mode.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct ScreenList {
    /// Les écrans, dans l'ordre de l'énumération du système (celui des numéros de `--projector-monitor`).
    pub screens: Vec<ScreenInfo>,
    /// Le projecteur est-il en plein écran en ce moment ? `None` : mode non pilotable depuis l'interface (lancé avec
    /// `--windowed`, ou pas encore placé).
    pub fullscreen: Option<bool>,
}

/// Décrit les écrans (`monitors` : chaque écran et sa marque « principal ») ; `projector` : la place de l'écran qui
/// porte le projecteur dans `monitors`.
///
/// Au plus un écran est marqué principal : le premier marqué. Un système qui en déclarerait deux (cela arrive pendant
/// un branchement) ne doit pas afficher deux « principal » à l'opérateur.
#[must_use]
pub fn describe_screens(monitors: &[(&Monitor, bool)], projector: Option<usize>) -> Vec<ScreenInfo> {
    let first_primary = monitors.iter().position(|(_, primary)| *primary);
    monitors
        .iter()
        .enumerate()
        .map(|(slot, (monitor, _))| ScreenInfo {
            name: monitor.name.clone(),
            width: monitor.physical_width,
            height: monitor.physical_height,
            refresh_mhz: monitor.refresh_rate_millihertz,
            primary: first_primary == Some(slot),
            projector: projector == Some(slot),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(name: Option<&str>, width: u32, height: u32) -> Monitor {
        Monitor {
            name: name.map(str::to_owned),
            physical_height: height,
            physical_width: width,
            physical_position: IVec2::ZERO,
            refresh_rate_millihertz: Some(60_000),
            scale_factor: 1.0,
            video_modes: Vec::new(),
        }
    }

    #[test]
    fn describe_screens_marks_one_primary() {
        let a = monitor(Some("DP-1"), 2560, 1440);
        let b = monitor(Some("HDMI-1"), 1920, 1080);
        let c = monitor(None, 1280, 720);
        // Deux écrans se disent principaux : un seul l'est pour l'opérateur, le premier.
        let screens = describe_screens(&[(&a, false), (&b, true), (&c, true)], Some(2));
        assert_eq!(screens.iter().filter(|s| s.primary).count(), 1);
        assert!(screens[1].primary);
        assert_eq!(screens.iter().filter(|s| s.projector).count(), 1);
        assert!(screens[2].projector);
        assert_eq!(
            screens[0],
            ScreenInfo {
                name: Some("DP-1".into()),
                width: 2560,
                height: 1440,
                refresh_mhz: Some(60_000),
                primary: false,
                projector: false,
            }
        );
    }

    #[test]
    fn describe_screens_without_primary_marks_none() {
        let a = monitor(Some("DP-1"), 1920, 1080);
        let screens = describe_screens(&[(&a, false)], None);
        assert!(!screens[0].primary && !screens[0].projector);
        assert_eq!(describe_screens(&[], None), Vec::<ScreenInfo>::new());
    }
}
