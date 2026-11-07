//! Mode manuel du monde : l'opérateur reprend la main sur les événements spontanés.

use bevy::prelude::*;

/// Automatisme du monde. En mode manuel, les événements spontanés (enseignes, immeubles, voitures, quartiers) ne se
/// déclenchent plus d'eux-mêmes ; seuls ceux demandés par l'opérateur ou par le spectacle passent.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldAutomation {
    /// Le monde n'initie plus d'événement de lui-même.
    pub manual: bool,
}

impl WorldAutomation {
    /// Le monde a le droit de déclencher un événement spontané.
    #[must_use]
    pub const fn allows_spontaneous(self) -> bool {
        !self.manual
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_automation_allows_spontaneous() {
        assert!(WorldAutomation::default().allows_spontaneous());
        assert!(!WorldAutomation { manual: true }.allows_spontaneous());
    }
}
