//! Les ordres de l'opérateur au spectacle : une file remplie par l'interface, vidée une fois par image.
//!
//! Aucun ordre ne contourne le limiteur photosensible : ce sont des `Action` que lit le chef du spectacle, et la
//! luminance finale passe par le limiteur de `cf-show` quoi qu'il arrive (règle 7).

use bevy::prelude::*;
use cf_control::{Action, DjIndex, TimedAction};
use cf_render::{ControlAction, ControlBridge, ControlsStatus, submit_action};

/// Les ordres de l'image, remplis par l'interface, envoyés par [`flush_orders`]. Le tampon est réutilisé.
#[derive(Resource, Debug, Default)]
pub(crate) struct LiveOrders(pub Vec<Action>);

impl LiveOrders {
    /// Passe au DJ de ce rang (quel que soit celui qui est en scène) : arme le créneau, puis demande le suivant, qui
    /// y mène par la passation prévue.
    pub(crate) fn go_to_dj(&mut self, slot: usize) {
        let Ok(index) = u16::try_from(slot) else {
            return;
        };
        self.0.push(Action::ArmDj(DjIndex(index)));
        self.0.push(Action::NextDj);
    }
}

/// Envoie les ordres de l'image au spectacle, par le chemin des contrôles quand il existe (le routage du tap tempo et
/// la « dernière action » en dépendent), sinon directement.
pub(crate) fn flush_orders(
    mut orders: ResMut<LiveOrders>,
    bridge: Option<ResMut<ControlBridge>>,
    status: Option<ResMut<ControlsStatus>>,
    mut out: MessageWriter<ControlAction>,
    time: Res<Time<Real>>,
) {
    if orders.0.is_empty() {
        return;
    }
    if let (Some(mut bridge), Some(mut status)) = (bridge, status) {
        for action in orders.0.drain(..) {
            submit_action(&mut bridge, &mut status, &mut out, action);
        }
    } else {
        let time_s = time.elapsed_secs_f64();
        for action in orders.0.drain(..) {
            out.write(ControlAction(TimedAction { time_s, action }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aller_a_un_dj_arme_son_creneau_puis_demande_le_suivant() {
        let mut orders = LiveOrders::default();
        orders.go_to_dj(2);
        assert_eq!(orders.0, [Action::ArmDj(DjIndex(2)), Action::NextDj]);
    }

    #[test]
    fn un_rang_hors_limites_ne_fait_rien() {
        let mut orders = LiveOrders::default();
        orders.go_to_dj(usize::MAX);
        assert_eq!(orders.0, []);
    }
}
