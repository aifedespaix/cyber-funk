//! Plan imposé par l'opérateur (onglet Debug) : le réalisateur y coupe, puis le garde jusqu'à « Auto ».
//!
//! Ce n'est pas un ordre du chef du spectacle (`Order` ne porte pas de charge utile) : le message `ShotOrder` arrive
//! directement ici. Le mode manuel du monde n'est jamais touché : choisir un plan ne change aucun mode.

use super::{CinemaDirector, Decision, PickMode, TransitionKind};

/// Ce que l'opérateur voit d'un plan du catalogue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotInfo {
    /// Identifiant stable (`"chase"`, `"orbit"`…).
    pub id: &'static str,
    /// Durée normale minimale (s).
    pub min_s: f32,
    /// Durée normale maximale (s).
    pub max_s: f32,
}

/// Ce que le plan imposé décide pour cette image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HoldCall {
    /// Aucun plan imposé : la routine ordinaire décide.
    Off,
    /// Couper (ou revenir) sur le plan imposé.
    Cut(Decision),
    /// Le plan imposé reste : la routine ordinaire se tait.
    Keep,
}

/// L'état du plan imposé.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Hold {
    /// Rang du plan gardé dans le catalogue.
    pub spec: Option<usize>,
    /// La coupe vers ce plan reste à faire (une transition peut être en cours).
    pub cut_pending: bool,
    /// Transition d'entrée sur le plan imposé ; `None` : coupe franche.
    pub kind: Option<TransitionKind>,
}

impl CinemaDirector {
    /// Les identifiants du catalogue, dans l'ordre de Sillage.
    pub fn shot_ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.catalog.iter().map(|spec| spec.id())
    }

    /// Les plans du catalogue avec leurs durées, dans l'ordre de Sillage.
    pub fn shot_infos(&self) -> impl Iterator<Item = ShotInfo> + '_ {
        self.catalog.iter().map(|spec| ShotInfo {
            id: spec.id(),
            min_s: spec.min_seconds(),
            max_s: spec.max_seconds(),
        })
    }

    /// Le plan imposé par l'opérateur, s'il y en a un.
    #[must_use]
    pub fn held_shot(&self) -> Option<&'static str> {
        self.hold.spec.and_then(|i| self.catalog.get(i)).map(|s| s.id())
    }

    /// Impose ce plan : coupe dessus à la prochaine image libre et le garde. `false` si l'identifiant est inconnu.
    pub fn hold_shot(&mut self, id: &str) -> bool {
        self.hold_shot_with(id, TransitionKind::Cut)
    }

    /// Comme [`Self::hold_shot`], avec la transition d'entrée voulue (le retour à la scène d'attente est un fondu).
    pub fn hold_shot_with(&mut self, id: &str, kind: TransitionKind) -> bool {
        let Some(index) = self.editor.find(id) else {
            return false;
        };
        self.hold = Hold {
            spec: Some(index),
            cut_pending: self.current.spec != Some(index) || self.transition.active,
            kind: Some(kind),
        };
        true
    }

    /// Rend la main au montage automatique.
    pub fn release_shot(&mut self) {
        self.hold = Hold::default();
    }

    /// La décision du plan imposé pour cette image : couper dessus, ou le relancer s'il vient de se terminer.
    pub(super) fn hold_decision(&mut self, alive: bool) -> HoldCall {
        let Some(index) = self.hold.spec else {
            return HoldCall::Off;
        };
        let cut = std::mem::take(&mut self.hold.cut_pending);
        if cut || !alive {
            return HoldCall::Cut(Decision {
                mode: PickMode::Normal,
                spec: Some(index),
                kind: Some(self.hold.kind.unwrap_or(TransitionKind::Cut)),
                counted: false,
            });
        }
        // Gardé : la routine de coupe se tait.
        HoldCall::Keep
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shots::{Chase, Orbit};
    use crate::testkit::{BoxCity, Sim};
    use crate::world::CinemaFrame;
    use bevy::math::DVec3;
    use cf_render::CameraPose;

    #[test]
    fn un_plan_impose_reste_a_l_antenne_jusqu_a_auto() {
        let mut sim = Sim::new(true);
        let mut director = CinemaDirector::new(vec![Box::new(Chase), Box::new(Orbit)], false, 7);
        let city = BoxCity;
        let start = CameraPose {
            position: sim.world.car + DVec3::new(0.0, 3.0, 8.0),
            ..CameraPose::default()
        };
        director.start(&start, &CinemaFrame::new(&sim.world, &city, &[]));
        assert!(!director.hold_shot("inconnu"));
        assert!(director.hold_shot("orbit"));
        let mut run = |director: &mut CinemaDirector, seconds: f64| {
            let until = sim.world.time + seconds;
            while sim.world.time < until {
                sim.step();
                director.update(&CinemaFrame::new(&sim.world, &city, &[]));
            }
        };
        run(&mut director, 120.0);
        assert_eq!(
            director.shot_id(),
            Some("orbit"),
            "gardé bien au-delà de sa durée normale"
        );
        assert_eq!(director.held_shot(), Some("orbit"));
        director.release_shot();
        assert_eq!(director.held_shot(), None);
        run(&mut director, 120.0);
        assert!(director.stats().shots > 3, "le montage automatique a repris");
    }
}
