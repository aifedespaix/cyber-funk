//! L'ordre « lance un astre » d'un plan (`launchMeteor` de Sillage) vers le monde.

use super::CinemaDirector;

impl CinemaDirector {
    /// Un plan vient de demander un astre au monde (`meteor-ride` sans astre en vol) : vrai une seule fois par demande.
    /// Le système Bevy en fait un message [`crate::world::LaunchMeteor`].
    pub fn take_meteor_launch(&mut self) -> bool {
        std::mem::take(&mut self.meteor_launches) > 0
    }
}
