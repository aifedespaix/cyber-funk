//! Ordres ponctuels de l'opérateur, à consommation unique (`take` de Sillage, avec son `ORDER_TTL`).
//!
//! Un ordre (« plan suivant », « montre le DJ »…) n'est pas exécuté par le chef du spectacle : il est déposé ici et le
//! système concerné le prend (caméra, enseigne, cascades, trafic). S'il n'est pris par personne dans `ORDER_TTL`
//! secondes, il expire : un ordre donné pendant une transition ne ressort pas trente secondes plus tard.

use cf_control::TriggerId;

/// Un ordre reste disponible ce temps (s) si aucun système ne le prend (`ORDER_TTL` de Sillage).
pub const ORDER_TTL: f32 = 3.0;
/// Déclencheurs du monde en attente au plus. Au-delà, le plus ancien est oublié (aucune allocation).
pub const TRIGGER_QUEUE: usize = 4;

/// Les ordres à usage unique (`QueuedAction` de Sillage), chacun pris par un seul système.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Order {
    /// Coupe au plan suivant (caméra).
    SkipShot,
    /// Plan sur l'enseigne du DJ (caméra et enseigne).
    ShowDj,
    /// L'Ascension (cascades), si les portes la permettent.
    Ascension,
    /// Un événement du trafic au hasard parmi ceux que le look permet (trafic).
    RandomEvent,
}

impl Order {
    /// Tous les ordres.
    pub const ALL: [Self; 4] = [Self::SkipShot, Self::ShowDj, Self::Ascension, Self::RandomEvent];

    const fn index(self) -> usize {
        self as usize
    }
}

/// Les ordres en attente, lisibles par tous (`pending`, compteurs `seq` pour réagir sans consommer), consommés par un
/// seul système via `ShowDirector::take`. Taille fixe : aucune allocation.
#[derive(Debug, Clone, Default)]
pub struct Orders {
    /// Durée de vie restante (s) de chaque ordre ; ≤ 0 : aucun ordre en attente.
    left: [f32; 4],
    /// Nombre d'ordres donnés, par ordre (compteur qui ne fait que monter).
    seq: [u32; 4],
    /// Déclencheurs du monde en attente et leur durée de vie restante (s).
    triggers: [Option<(TriggerId, f32)>; TRIGGER_QUEUE],
    trigger_seq: u32,
}

impl Orders {
    /// Un ordre de ce type attend d'être pris.
    #[must_use]
    pub fn pending(&self, order: Order) -> bool {
        self.left[order.index()] > 0.0
    }

    /// Nombre d'ordres de ce type donnés depuis l'ouverture : de quoi réagir sans consommer (`djShowSeq` de Sillage).
    #[must_use]
    pub fn seq(&self, order: Order) -> u32 {
        self.seq[order.index()]
    }

    /// Le plus ancien déclencheur du monde en attente, sans le consommer.
    #[must_use]
    pub fn trigger_pending(&self) -> Option<&TriggerId> {
        self.oldest_trigger()
            .and_then(|i| self.triggers[i].as_ref())
            .map(|(id, _)| id)
    }

    /// Nombre de déclencheurs du monde donnés depuis l'ouverture.
    #[must_use]
    pub fn trigger_seq(&self) -> u32 {
        self.trigger_seq
    }

    pub(crate) fn give(&mut self, order: Order) {
        self.left[order.index()] = ORDER_TTL;
        self.seq[order.index()] = self.seq[order.index()].wrapping_add(1);
    }

    /// Vrai une seule fois après un ordre encore vivant, puis il retombe.
    pub(crate) fn take(&mut self, order: Order) -> bool {
        let live = self.pending(order);
        self.left[order.index()] = 0.0;
        live
    }

    /// Dépose un déclencheur du monde ; file pleine : remplace le plus ancien. Le cloner ne coûte qu'un compteur.
    pub(crate) fn give_trigger(&mut self, id: TriggerId) {
        let slot = self
            .triggers
            .iter()
            .position(Option::is_none)
            .or_else(|| self.oldest_trigger())
            .unwrap_or(0);
        self.triggers[slot] = Some((id, ORDER_TTL));
        self.trigger_seq = self.trigger_seq.wrapping_add(1);
    }

    /// Prend le plus ancien déclencheur du monde encore vivant.
    pub(crate) fn take_trigger(&mut self) -> Option<TriggerId> {
        let i = self.oldest_trigger()?;
        self.triggers[i].take().map(|(id, _)| id)
    }

    /// Fait vieillir les ordres de `dt` secondes ; les expirés disparaissent.
    pub(crate) fn tick(&mut self, dt: f32) {
        for left in &mut self.left {
            if *left > 0.0 {
                *left -= dt;
            }
        }
        for slot in &mut self.triggers {
            if let Some((_, left)) = slot {
                *left -= dt;
                if *left <= 0.0 {
                    *slot = None;
                }
            }
        }
    }

    /// Le déclencheur déposé le plus tôt (le moins de vie restante : tous partent de `ORDER_TTL`).
    fn oldest_trigger(&self) -> Option<usize> {
        self.triggers
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| slot.as_ref().map(|(_, left)| (i, *left)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_order_is_taken_once_and_counted() {
        let mut orders = Orders::default();
        assert!(!orders.take(Order::SkipShot));
        orders.give(Order::SkipShot);
        assert!(orders.pending(Order::SkipShot));
        assert_eq!(orders.seq(Order::SkipShot), 1);
        assert!(orders.take(Order::SkipShot));
        assert!(!orders.take(Order::SkipShot), "consommation unique");
        assert_eq!(orders.seq(Order::SkipShot), 1, "le compteur ne redescend pas");
        assert!(!orders.pending(Order::ShowDj), "les ordres sont indépendants");
    }

    #[test]
    fn an_order_nobody_takes_expires_after_ttl() {
        let mut orders = Orders::default();
        orders.give(Order::Ascension);
        orders.tick(ORDER_TTL - 0.1);
        assert!(orders.pending(Order::Ascension));
        orders.tick(0.2);
        assert!(!orders.take(Order::Ascension));
    }

    #[test]
    fn triggers_queue_oldest_first_and_overflow_forgets_the_oldest() {
        let mut orders = Orders::default();
        for (i, name) in ["a", "b", "c", "d", "e"].into_iter().enumerate() {
            orders.give_trigger(TriggerId::new(name));
            // Chaque dépôt vieillit un peu les précédents : l'ordre d'arrivée se lit dans la vie restante.
            orders.tick(0.01 * (i as f32 + 1.0));
        }
        assert_eq!(orders.trigger_seq(), 5);
        let taken: Vec<String> = std::iter::from_fn(|| orders.take_trigger())
            .map(|id| id.0.to_string())
            .collect();
        assert_eq!(taken, ["b", "c", "d", "e"], "« a » a été remplacé par « e »");
    }

    #[test]
    fn triggers_expire_too() {
        let mut orders = Orders::default();
        orders.give_trigger(TriggerId::new("storm"));
        assert_eq!(orders.trigger_pending().map(|id| &*id.0), Some("storm"));
        orders.tick(ORDER_TTL + 0.01);
        assert!(orders.take_trigger().is_none());
    }
}
