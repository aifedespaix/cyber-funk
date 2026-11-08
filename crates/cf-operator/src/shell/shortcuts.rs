//! Les raccourcis clavier propres à la fenêtre opérateur : `Ctrl+1/2/3` pour les onglets, `M` pour poser un repère
//! dans le journal. Tout le reste du clavier (GO, Noir…) appartient à `cf-control`.

use bevy_egui::egui::{self, Event, Key};

use super::Tab;

/// Touche du repère dans le journal.
pub(crate) const KEY_MARK: &str = "M";
/// Raccourci de l'onglet Live.
pub(crate) const KEY_TAB_1: &str = "Ctrl+1";
/// Raccourci de l'onglet DJs.
pub(crate) const KEY_TAB_2: &str = "Ctrl+2";
/// Raccourci de l'onglet Debug.
pub(crate) const KEY_TAB_3: &str = "Ctrl+3";

/// Tous les raccourcis de la fenêtre : (touche, effet).
pub(crate) const SHORTCUTS: &[(&str, &str)] = &[
    (KEY_TAB_1, "Onglet Live"),
    (KEY_TAB_2, "Onglet DJs"),
    (KEY_TAB_3, "Onglet Debug"),
    (
        KEY_MARK,
        "Marquer ce moment : repère « mark » dans le journal de session (sauf saisie de texte)",
    ),
];

/// Ce qu'une touche globale demande à la coque.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShellKey {
    /// Ouvrir cet onglet.
    Tab(Tab),
    /// Poser un repère dans le journal.
    Mark,
}

impl ShellKey {
    /// Le texte de la touche, tel que l'écrivent les infobulles.
    pub(crate) fn text(self) -> &'static str {
        match self {
            Self::Tab(Tab::Live) => KEY_TAB_1,
            Self::Tab(Tab::Djs) => KEY_TAB_2,
            Self::Tab(Tab::Debug) => KEY_TAB_3,
            Self::Mark => KEY_MARK,
        }
    }
}

/// L'effet d'une touche dans la table, pour une infobulle (`""` si la touche n'y est pas).
pub(crate) fn describe(key: &str) -> &'static str {
    SHORTCUTS.iter().find(|(k, _)| *k == key).map_or("", |(_, d)| d)
}

/// L'onglet demandé par `Ctrl+1/2/3` (Maj indifférente, jamais avec Alt : `AltGr` vaut `Ctrl+Alt` sous Windows).
pub(crate) fn tab_for_shortcut(key: Key, modifiers: egui::Modifiers) -> Option<Tab> {
    if !(modifiers.command || modifiers.ctrl) || modifiers.alt {
        return None;
    }
    match key {
        Key::Num1 => Some(Tab::Live),
        Key::Num2 => Some(Tab::Djs),
        Key::Num3 => Some(Tab::Debug),
        _ => None,
    }
}

/// Les touches globales pressées dans cette image, ajoutées à `out` (dans l'ordre des événements). Rien pendant une
/// saisie de texte.
pub(crate) fn poll(input: &egui::InputState, typing: bool, out: &mut Vec<ShellKey>) {
    if typing {
        return;
    }
    for event in &input.events {
        let Event::Key {
            key,
            physical_key,
            pressed: true,
            repeat: false,
            modifiers,
        } = event
        else {
            continue;
        };
        let tab =
            tab_for_shortcut(*key, *modifiers).or_else(|| physical_key.and_then(|k| tab_for_shortcut(k, *modifiers)));
        if let Some(tab) = tab {
            out.push(ShellKey::Tab(tab));
        } else if *key == Key::M && !(modifiers.ctrl || modifiers.command || modifiers.alt || modifiers.mac_cmd) {
            out.push(ShellKey::Mark);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl() -> egui::Modifiers {
        egui::Modifiers {
            ctrl: true,
            command: true,
            ..egui::Modifiers::NONE
        }
    }

    #[test]
    fn ctrl_1_2_3_choisissent_les_onglets() {
        assert_eq!(tab_for_shortcut(Key::Num1, ctrl()), Some(Tab::Live));
        assert_eq!(tab_for_shortcut(Key::Num2, ctrl()), Some(Tab::Djs));
        assert_eq!(tab_for_shortcut(Key::Num3, ctrl()), Some(Tab::Debug));
    }

    #[test]
    fn sans_ctrl_ou_avec_alt_rien_ne_se_passe() {
        assert_eq!(tab_for_shortcut(Key::Num1, egui::Modifiers::NONE), None);
        let altgr = egui::Modifiers { alt: true, ..ctrl() };
        assert_eq!(tab_for_shortcut(Key::Num1, altgr), None);
        assert_eq!(tab_for_shortcut(Key::Num4, ctrl()), None);
    }

    #[test]
    fn chaque_raccourci_a_une_description() {
        for key in [KEY_TAB_1, KEY_TAB_2, KEY_TAB_3, KEY_MARK] {
            assert!(!describe(key).is_empty(), "{key}");
        }
        assert_eq!(describe("Z"), "");
    }
}
