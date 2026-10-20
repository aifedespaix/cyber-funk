//! Pool d'entités pour ce qui va et vient hors des tronçons (véhicules, astres, enseignes d'événement).
//!
//! Tout est créé au démarrage ; en régime établi on emprunte et on rend, jamais de `spawn`/`despawn`. Pool épuisé :
//! l'appelant renonce (un véhicule de moins), il ne crée rien. Le pool ne touche pas aux composants : à l'emprunteur
//! d'afficher l'entité, au rendeur de la cacher (`Visibility::Hidden`).

use bevy::prelude::*;

/// Marqueur des entités qui appartiennent à un pool (diagnostic : compter les entités préallouées).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Pooled;

/// Une entité empruntée : à rendre telle quelle à son pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PoolHandle {
    slot: u32,
    entity: Entity,
}

impl PoolHandle {
    /// L'entité empruntée.
    #[must_use]
    pub fn entity(self) -> Entity {
        self.entity
    }
}

/// Un pool d'entités de capacité fixe. Emprunter et rendre se font en temps constant, sans allocation.
#[derive(Debug, Clone, Default)]
pub struct EntityPool {
    entities: Vec<Entity>,
    busy: Vec<bool>,
    /// Pile des emplacements libres (le dernier rendu ressort le premier).
    free: Vec<u32>,
}

impl EntityPool {
    /// Pool sur des entités déjà créées (et marquées [`Pooled`] par l'appelant).
    #[must_use]
    pub fn new(entities: Vec<Entity>) -> Self {
        let len = entities.len();
        // L'ordre inverse fait sortir la première entité en premier : plus lisible au débogage.
        let free = (0..len).rev().map(|i| u32::try_from(i).unwrap_or(u32::MAX)).collect();
        Self {
            entities,
            busy: vec![false; len],
            free,
        }
    }

    /// Crée `capacity` entités cachées, chacune avec une copie de `bundle`, et en fait un pool. Au démarrage seulement.
    pub fn spawn<B: Bundle + Clone>(commands: &mut Commands, capacity: usize, bundle: B) -> Self {
        let entities = (0..capacity)
            .map(|_| commands.spawn((bundle.clone(), Pooled, Visibility::Hidden)).id())
            .collect();
        Self::new(entities)
    }

    /// Emprunte une entité, ou `None` si le pool est épuisé.
    pub fn acquire(&mut self) -> Option<PoolHandle> {
        let slot = self.free.pop()?;
        let index = slot as usize;
        let (Some(busy), Some(&entity)) = (self.busy.get_mut(index), self.entities.get(index)) else {
            return None;
        };
        *busy = true;
        Some(PoolHandle { slot, entity })
    }

    /// Rend une entité. Renvoie `false` si la poignée ne vient pas de ce pool ou a déjà été rendue (rien ne change).
    pub fn release(&mut self, handle: PoolHandle) -> bool {
        let index = handle.slot as usize;
        match (self.busy.get_mut(index), self.entities.get(index)) {
            (Some(busy), Some(&entity)) if *busy && entity == handle.entity => {
                *busy = false;
                self.free.push(handle.slot);
                true
            }
            _ => false,
        }
    }

    /// Nombre total d'entités du pool.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.entities.len()
    }

    /// Entités empruntées en ce moment.
    #[must_use]
    pub fn in_use(&self) -> usize {
        self.entities.len() - self.free.len()
    }

    /// Toutes les entités du pool, libres ou non.
    #[must_use]
    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(n: u32) -> EntityPool {
        EntityPool::new(
            (0..n)
                .map(|i| Entity::from_raw_u32(i + 1).expect("index valide"))
                .collect(),
        )
    }

    #[test]
    fn emprunter_rendre_epuiser() {
        let mut pool = pool(2);
        let a = pool.acquire().expect("libre");
        let b = pool.acquire().expect("libre");
        assert_ne!(a.entity(), b.entity());
        assert!(pool.acquire().is_none());
        assert_eq!(pool.in_use(), 2);
        assert!(pool.release(a));
        assert!(!pool.release(a), "double rendu refusé");
        assert_eq!(pool.acquire().map(PoolHandle::entity), Some(a.entity()));
        assert_eq!(pool.capacity(), 2);
    }

    #[test]
    fn poignee_etrangere_refusee() {
        let mut p1 = pool(1);
        let mut p2 = EntityPool::new(vec![Entity::from_raw_u32(99).expect("index valide")]);
        let h = p2.acquire().expect("libre");
        assert!(!p1.release(h));
        assert!(p2.release(h));
    }
}
