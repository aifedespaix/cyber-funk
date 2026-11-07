//! Les raccourcis globaux de la coque : une table unique, [`SHORTCUTS`], lue par l'aide `?` et par les infobulles.
//!
//! Choix des touches, vérifiés contre le clavier par défaut (`cf_control::ControlProfile::sillage_keyboard`) que le
//! pont des contrôles lit **dans les deux fenêtres** :
//! - les chiffres seuls sont liés à `GotoCue(n)` : les onglets passent donc par `Ctrl+1/2/3`. Le pont exige que Ctrl
//!   corresponde exactement (`key_matches` de `cf-control`), Ctrl+chiffre ne saute donc jamais de cue ;
//! - `Échap` n'est lié à rien (Sillage s'en servait pour sortir, cyber-funk jamais) : c'est le Noir ;
//! - `?` n'est lié à rien ; sur AZERTY c'est Maj+`,`, et la vue Live n'écoute `,` que sans Maj ;
//! - `M` seul n'est lié à rien (la vue Live ne prend que `Ctrl+M`, le changement de mode) : c'est le repère du journal.
//!   Lu sur la touche **logique** (la lettre M, où qu'elle soit sur le clavier), jamais avec Ctrl, Alt ou Cmd.
//!
//! Les chiffres sont reconnus par leur caractère **ou** leur position : sur AZERTY la rangée du haut donne « & é " »
//! sans Maj, seule la touche physique dit « 1 2 3 ».

use bevy_egui::egui::{self, Event, Key};

use super::Tab;
use crate::OperatorMode;

/// Texte des raccourcis, partagé par la table, les infobulles et l'aide.
pub(crate) const KEY_TAB_1: &str = "Ctrl+1";
/// Voir [`KEY_TAB_1`].
pub(crate) const KEY_TAB_2: &str = "Ctrl+2";
/// Voir [`KEY_TAB_1`].
pub(crate) const KEY_TAB_3: &str = "Ctrl+3";
/// Voir [`KEY_TAB_1`].
pub(crate) const KEY_HELP: &str = "?";
/// Voir [`KEY_TAB_1`].
pub(crate) const KEY_BLACKOUT: &str = "Échap";
/// Voir [`KEY_TAB_1`].
pub(crate) const KEY_MARK: &str = "M";

/// Tous les raccourcis de la coque : (touche, effet). L'aide les liste dans cet ordre.
pub(crate) const SHORTCUTS: &[(&str, &str)] = &[
    (KEY_TAB_1, "Onglet Réglages (Live en mode Live)"),
    (KEY_TAB_2, "Onglet Statistiques"),
    (KEY_TAB_3, "Onglet Debug (Préparation seulement)"),
    (KEY_HELP, "Afficher ou masquer cette aide"),
    (
        KEY_BLACKOUT,
        "Noir : bascule le blackout (sauf dialogue, menu ouvert ou saisie de texte)",
    ),
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
    /// Afficher ou masquer l'aide.
    Help,
    /// Basculer le Noir.
    Blackout,
    /// Poser un repère dans le journal.
    Mark,
}

impl ShellKey {
    /// Toutes les commandes de la coque (l'aide doit les lister toutes).
    #[cfg(test)]
    pub(crate) const ALL: [ShellKey; 6] = [
        Self::Tab(Tab::Settings),
        Self::Tab(Tab::Stats),
        Self::Tab(Tab::Debug),
        Self::Help,
        Self::Blackout,
        Self::Mark,
    ];

    /// Le texte de la touche, tel que [`SHORTCUTS`] l'affiche.
    pub(crate) fn text(self) -> &'static str {
        match self {
            Self::Tab(Tab::Settings) => KEY_TAB_1,
            Self::Tab(Tab::Stats) => KEY_TAB_2,
            Self::Tab(Tab::Debug) => KEY_TAB_3,
            Self::Help => KEY_HELP,
            Self::Blackout => KEY_BLACKOUT,
            Self::Mark => KEY_MARK,
        }
    }
}

/// L'effet d'une touche dans la table, pour une infobulle (`""` si la touche n'y est pas).
pub(crate) fn describe(key: &str) -> &'static str {
    SHORTCUTS.iter().find(|(k, _)| *k == key).map_or("", |(_, what)| what)
}

/// Ce qui empêche les touches globales de cette image.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct KeyGuard {
    /// Un champ texte d'egui a le clavier : aucune touche globale.
    pub typing: bool,
    /// Un dialogue (sortie, changement de mode) est ouvert : `Échap` lui appartient (il annule).
    pub dialog_open: bool,
    /// Un menu ou une liste déroulante d'egui est ouvert : `Échap` le ferme. Une liste ouverte ne prend pas le
    /// clavier (`typing` reste faux), il faut donc ce garde-fou pour que fermer un menu n'éteigne pas la salle.
    pub popup_open: bool,
}

impl KeyGuard {
    /// `Échap` vaut-il le Noir ? Jamais pendant une saisie, un dialogue ou un menu ouvert : il y veut dire « fermer ».
    pub(crate) fn escape_is_blackout(self) -> bool {
        !(self.typing || self.dialog_open || self.popup_open)
    }
}

/// L'onglet demandé par `Ctrl+1/2/3` (Maj indifférente, jamais avec Alt : `AltGr` vaut `Ctrl+Alt` sous Windows). L'onglet
/// Debug n'est pas accessible en Live.
pub(crate) fn tab_for_shortcut(key: Key, modifiers: egui::Modifiers, mode: OperatorMode) -> Option<Tab> {
    if !(modifiers.command || modifiers.ctrl) || modifiers.alt {
        return None;
    }
    let tab = match key {
        Key::Num1 => Tab::Settings,
        Key::Num2 => Tab::Stats,
        Key::Num3 => Tab::Debug,
        _ => return None,
    };
    tab.visible_in(mode).then_some(tab)
}

/// Les touches globales pressées dans cette image, ajoutées à `out` (dans l'ordre des événements).
pub(crate) fn poll(input: &egui::InputState, mode: OperatorMode, guard: KeyGuard, out: &mut Vec<ShellKey>) {
    if guard.typing {
        return;
    }
    for event in &input.events {
        match event {
            Event::Key {
                key,
                physical_key,
                pressed: true,
                repeat: false,
                modifiers,
            } => {
                let tab = tab_for_shortcut(*key, *modifiers, mode)
                    .or_else(|| physical_key.and_then(|k| tab_for_shortcut(k, *modifiers, mode)));
                if let Some(tab) = tab {
                    out.push(ShellKey::Tab(tab));
                } else if *key == Key::Escape && guard.escape_is_blackout() {
                    out.push(ShellKey::Blackout);
                } else if *key == Key::M && !(modifiers.ctrl || modifiers.command || modifiers.alt || modifiers.mac_cmd)
                {
                    out.push(ShellKey::Mark);
                }
            }
            Event::Text(text) if text == KEY_HELP => out.push(ShellKey::Help),
            _ => {}
        }
    }
}
