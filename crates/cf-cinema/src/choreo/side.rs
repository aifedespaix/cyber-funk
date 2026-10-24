//! Ce que la mise en scène et l'éditeur demandent au chef du spectacle : les portes du look, le drop préparé, les
//! ordres de l'opérateur et la fraîcheur commune.
//!
//! Sillage passait tout cela dans `world.show` (un objet vivant : `take`, `freshness`, `drop`). Ici `CinemaWorld` reste
//! `Copy` ; ce qui est écrit (ordres consommés, fraîcheur marquée) ou de taille variable (listes de plans permis) passe
//! par ce trait, que le chef du spectacle implémente. Les tests posent un faux spectacle sans lancer `ShowDirector`.

use cf_show::director::{Order, ShowDirector};
use cf_show::drop::DropPlan;
use cf_show::freshness::Freshness;
use cf_show::model::ShowGates;
use rand_core::Rng;

/// Le chef du spectacle vu par le réalisateur.
pub trait ShowSide {
    /// Les portes du look à l'écran. `None` : pas de spectacle, tout est permis.
    fn gates(&self) -> Option<&ShowGates>;
    /// Escalade lissée, 0 → 1 : sert à doser les salves.
    fn heat(&self) -> f32;
    /// Le drop préparé pendant la montée, lisible jusqu'à l'image où il frappe.
    fn drop_plan(&self) -> Option<&DropPlan>;
    /// Monte à chaque changement de créneau DJ : l'enseigne du DJ doit paraître à l'écran.
    fn slot_seq(&self) -> u32;
    /// Prend un ordre de l'opérateur encore vivant : vrai une seule fois.
    fn take(&mut self, order: Order) -> bool;
    /// Fraîcheur commune (`shot:`, `stunt:`, `event:`, `landmark:`), en lecture.
    fn freshness(&self) -> &Freshness;
    /// Demande un ralenti du monde `scale` (0.1 → 1) tenu `seconds` (s) : le bullet-time. Sans effet par défaut (pas de
    /// spectacle, faux spectacle des tests).
    fn request_time_scale(&mut self, _scale: f32, _seconds: f32) {}
    /// Fraîcheur commune, en écriture : `mark` quand un plan est joué.
    fn freshness_mut(&mut self) -> &mut Freshness;
}

/// Pas de spectacle (démos, tests, mode historique) : aucune porte, aucun ordre, une fraîcheur vide.
#[derive(Debug, Default)]
pub struct NoShow {
    freshness: Freshness,
}

impl ShowSide for NoShow {
    fn gates(&self) -> Option<&ShowGates> {
        None
    }

    fn heat(&self) -> f32 {
        0.0
    }

    fn drop_plan(&self) -> Option<&DropPlan> {
        None
    }

    fn slot_seq(&self) -> u32 {
        0
    }

    fn take(&mut self, _order: Order) -> bool {
        false
    }

    fn freshness(&self) -> &Freshness {
        &self.freshness
    }

    fn freshness_mut(&mut self) -> &mut Freshness {
        &mut self.freshness
    }
}

impl<R: Rng> ShowSide for ShowDirector<R> {
    fn gates(&self) -> Option<&ShowGates> {
        Some(self.state().gates())
    }

    fn heat(&self) -> f32 {
        self.state().heat
    }

    fn drop_plan(&self) -> Option<&DropPlan> {
        ShowDirector::drop_plan(self)
    }

    fn slot_seq(&self) -> u32 {
        self.state().slot_seq
    }

    fn take(&mut self, order: Order) -> bool {
        ShowDirector::take(self, order)
    }

    fn freshness(&self) -> &Freshness {
        ShowDirector::freshness(self)
    }

    fn freshness_mut(&mut self) -> &mut Freshness {
        ShowDirector::freshness_mut(self)
    }

    fn request_time_scale(&mut self, scale: f32, seconds: f32) {
        ShowDirector::request_time_scale(self, scale, seconds);
    }
}
