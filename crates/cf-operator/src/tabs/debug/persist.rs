//! Ce que la fenêtre « Debug monde » retient : sections repliées et familles d'événements ouvertes.
//!
//! Le tout vit dans la structure typée [`DebugPanelPrefs`] (`AppSettings::debug_panel`). Les fichiers d'avant la
//! retenaient dans `collapsed_panels` sous la forme `debug-open:<famille>` : ces entrées restent comprises à la lecture
//! et passent dans la structure typée au premier geste (migration silencieuse). Les curseurs des façades `tuning:*`
//! restent dans `collapsed_panels`, où `cf-render` les lit et les écrit : on n'y touche pas.
//!
//! Rien de l'état des quartiers (gel, forçage, choix en cours) n'est retenu ici.

use cf_show::persist::{AppSettings, DebugPanelPrefs};

use super::Section;

/// Préfixe historique, dans `collapsed_panels`, des familles ouvertes.
const LEGACY_OPEN_PREFIX: &str = "debug-open:";

/// La famille est ouverte : retenue dans la structure typée, ou encore sous l'ancienne clé.
pub(crate) fn family_is_open(settings: &AppSettings, family: &str) -> bool {
    settings.debug_panel.open_families.contains(family)
        || settings
            .collapsed_panels
            .iter()
            .any(|entry| entry.strip_prefix(LEGACY_OPEN_PREFIX) == Some(family))
}

/// Retient l'ouverture d'une famille. Migre d'abord les anciennes clés : sinon une famille fermée ici serait
/// rouverte au lancement suivant par son ancienne entrée.
pub(crate) fn set_family_open(settings: &mut AppSettings, family: &str, open: bool) {
    migrate_legacy(settings);
    let families = &mut settings.debug_panel.open_families;
    if open {
        if !families.contains(family) {
            families.insert(family.to_owned());
        }
    } else {
        families.remove(family);
    }
}

/// La section est dépliée (vrai par défaut).
pub(crate) fn section_is_open(settings: &AppSettings, section: Section) -> bool {
    !settings.debug_panel.closed_sections.contains(section.key())
}

/// Retient le dépliage d'une section.
pub(crate) fn set_section_open(settings: &mut AppSettings, section: Section, open: bool) {
    let closed = &mut settings.debug_panel.closed_sections;
    if open {
        closed.remove(section.key());
    } else if !closed.contains(section.key()) {
        closed.insert(section.key().to_owned());
    }
}

/// Fait passer les anciennes entrées `debug-open:*` dans [`DebugPanelPrefs`] et les retire de `collapsed_panels`.
/// Les autres entrées (`tuning:*`…) restent en place. `true` si quelque chose a bougé.
pub(crate) fn migrate_legacy(settings: &mut AppSettings) -> bool {
    let AppSettings {
        collapsed_panels,
        debug_panel,
        ..
    } = settings;
    let DebugPanelPrefs { open_families, .. } = debug_panel;
    let before = collapsed_panels.len();
    collapsed_panels.retain(|entry| match entry.strip_prefix(LEGACY_OPEN_PREFIX) {
        Some(family) => {
            open_families.insert(family.to_owned());
            false
        }
        None => true,
    });
    collapsed_panels.len() != before
}
