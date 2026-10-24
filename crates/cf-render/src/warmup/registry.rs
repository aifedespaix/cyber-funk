//! Le registre de préchauffe : ce que chaque monde ou plugin déclare avoir besoin de voir compilé avant le show.
//!
//! Deux façons de s'inscrire, depuis le `build` d'un plugin :
//!
//! - [`WarmupAppExt::warm_material`] : « préchauffe tout ce que mes entités de ce matériau savent faire ». À
//!   l'ouverture, une copie minuscule de chaque combinaison (maillage, mode alpha) réellement présente dans le monde
//!   est posée devant la caméra qui la voit. Les entités cachées d'un pool (fumée, traînée, étoiles filantes…) sont
//!   couvertes : c'est leur première apparition en plein show qui compilerait sinon.
//! - [`WarmupAppExt::add_warmup`] : « je fais naître moi-même ce qu'il faut voir » (un matériau sans entité au repos,
//!   une variante par `shader_defs`). La fonction reçoit le `World` et rend les entités créées ; elles sont placées
//!   devant la caméra de leur `RenderLayers` et détruites au passage à `Ready`.
//!
//! Les entités de préchauffe ne portent jamais de logique : ce sont des copies de rendu, retirées avant le show.

use bevy::pbr::Material;
use bevy::prelude::*;

use super::mirror::mirror_material;

/// Fonction de préchauffe : fait naître des entités visibles et les rend.
pub type WarmupFn = Box<dyn Fn(&mut World) -> Vec<Entity> + Send + Sync>;

/// Les préchauffes déclarées, exécutées une fois quand les assets sont chargés.
#[derive(Resource, Default)]
pub struct WarmupRegistry {
    entries: Vec<(String, WarmupFn)>,
}

impl std::fmt::Debug for WarmupRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.entries.iter().map(|(label, _)| label)).finish()
    }
}

impl WarmupRegistry {
    /// Déclare une préchauffe sous un nom lisible (journal et écran de progression).
    pub fn add(&mut self, label: impl Into<String>, warmup: WarmupFn) {
        self.entries.push((label.into(), warmup));
    }

    /// Les noms des préchauffes déclarées.
    pub fn labels(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(label, _)| label.as_str())
    }

    /// Nombre de préchauffes déclarées.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Vrai si rien n'est déclaré.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Exécute toutes les préchauffes ; rend leurs entités et, par entrée, le nombre qu'elle en a créé.
    pub(super) fn run_all(&self, world: &mut World) -> (Vec<Entity>, Vec<(String, usize)>) {
        let mut entities = Vec::new();
        let mut counts = Vec::new();
        for (label, warmup) in &self.entries {
            let made = warmup(world);
            counts.push((label.clone(), made.len()));
            entities.extend(made);
        }
        (entities, counts)
    }
}

/// Déclarations de préchauffe sur l'`App`, pour les plugins.
pub trait WarmupAppExt {
    /// Déclare une préchauffe explicite (voir la doc du module).
    fn add_warmup(
        &mut self,
        label: &str,
        warmup: impl Fn(&mut World) -> Vec<Entity> + Send + Sync + 'static,
    ) -> &mut Self;

    /// Préchauffe toutes les combinaisons (maillage, mode alpha) des entités qui portent un `MeshMaterial3d<M>`.
    ///
    /// Limite : deux matériaux de même type qui ne diffèrent que par leur `bind_group_data` (clé de spécialisation
    /// portée par le matériau) ne sont pas distingués ; un tel matériau déclare une préchauffe explicite.
    fn warm_material<M: Material>(&mut self, label: &str) -> &mut Self;
}

impl WarmupAppExt for App {
    fn add_warmup(
        &mut self,
        label: &str,
        warmup: impl Fn(&mut World) -> Vec<Entity> + Send + Sync + 'static,
    ) -> &mut Self {
        self.init_resource::<WarmupRegistry>();
        self.world_mut()
            .resource_mut::<WarmupRegistry>()
            .add(label, Box::new(warmup));
        self
    }

    fn warm_material<M: Material>(&mut self, label: &str) -> &mut Self {
        self.add_warmup(label, mirror_material::<M>)
    }
}
