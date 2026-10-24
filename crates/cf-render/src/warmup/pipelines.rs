//! Le guetteur de pipelines : il lit le `PipelineCache` du monde de rendu et en publie le compte vers le monde
//! principal, pour deux usages.
//!
//! 1. **Attendre la compilation** : tant qu'un pipeline est en attente, la préchauffe n'est pas finie.
//! 2. **Détecteur de régression** : une fois l'application `Ready`, tout pipeline qui apparaît dans le cache est un
//!    pipeline compilé pendant le show (donc un à-coup visible). Il est journalisé en avertissement avec son libellé,
//!    pour trouver quel matériau ou quelle variante la préchauffe n'a pas couverte.
//!
//! Le monde de rendu est un `SubApp` à part : on passe par des compteurs atomiques partagés (aucun verrou).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use bevy::material::descriptor::PipelineDescriptor;
use bevy::prelude::*;
use bevy::render::render_resource::PipelineCache;
use bevy::render::{Render, RenderApp, RenderSystems};

#[derive(Debug, Default)]
struct WatchInner {
    /// Le monde de rendu a publié au moins une fois (faux sans GPU : tests, serveur sans écran).
    attached: AtomicBool,
    /// Nombre de pipelines connus du cache (en attente, en cours ou prêts).
    total: AtomicUsize,
    /// Nombre de pipelines pas encore prêts.
    waiting: AtomicUsize,
    /// Le détecteur est armé : l'application est `Ready`.
    armed: AtomicBool,
    /// Nombre de pipelines au moment du passage à `Ready`.
    baseline: AtomicUsize,
}

/// Les compteurs du `PipelineCache`, partagés entre le monde de rendu (qui écrit) et le monde principal (qui lit).
#[derive(Resource, Debug, Clone, Default)]
pub struct PipelineWatch(Arc<WatchInner>);

impl PipelineWatch {
    /// Vrai si un monde de rendu publie réellement ses compteurs.
    #[must_use]
    pub fn attached(&self) -> bool {
        self.0.attached.load(Ordering::Relaxed)
    }

    /// Nombre de pipelines connus.
    #[must_use]
    pub fn total(&self) -> usize {
        self.0.total.load(Ordering::Relaxed)
    }

    /// Nombre de pipelines en attente de compilation.
    #[must_use]
    pub fn waiting(&self) -> usize {
        self.0.waiting.load(Ordering::Relaxed)
    }

    /// Nombre de pipelines apparus depuis `Ready` (0 tant que le détecteur n'est pas armé).
    #[must_use]
    pub fn late(&self) -> usize {
        if self.0.armed.load(Ordering::Relaxed) {
            self.total().saturating_sub(self.0.baseline.load(Ordering::Relaxed))
        } else {
            0
        }
    }

    /// Écrit les compteurs (côté monde de rendu, ou tests).
    pub fn publish(&self, total: usize, waiting: usize) {
        self.0.total.store(total, Ordering::Relaxed);
        self.0.waiting.store(waiting, Ordering::Relaxed);
        self.0.attached.store(true, Ordering::Relaxed);
    }

    /// Arme le détecteur : tout pipeline au-delà du compte actuel sera une compilation tardive.
    pub fn arm(&self) {
        self.0.baseline.store(self.total(), Ordering::Relaxed);
        self.0.armed.store(true, Ordering::Relaxed);
    }

    fn is_armed(&self) -> bool {
        self.0.armed.load(Ordering::Relaxed)
    }

    fn baseline(&self) -> usize {
        self.0.baseline.load(Ordering::Relaxed)
    }
}

/// Libellé lisible d'un pipeline du cache, pour le journal.
fn label_of(descriptor: &PipelineDescriptor) -> String {
    let label = match descriptor {
        PipelineDescriptor::RenderPipelineDescriptor(d) => d.label.as_deref(),
        PipelineDescriptor::ComputePipelineDescriptor(d) => d.label.as_deref(),
        PipelineDescriptor::MeshPipelineDescriptor(d) => d.label.as_deref(),
    };
    label.unwrap_or("(sans libellé)").to_owned()
}

/// Monde de rendu, après la file d'attente des pipelines : publie les compteurs et, armé, journalise les nouveaux.
fn watch_pipelines(cache: Res<PipelineCache>, watch: Res<PipelineWatch>, mut reported: Local<usize>) {
    let total = cache.pipelines().count();
    let waiting = cache.waiting_pipelines().count();
    watch.publish(total, waiting);
    if !watch.is_armed() {
        *reported = total;
        return;
    }
    let from = (*reported).max(watch.baseline());
    for cached in cache.pipelines().skip(from) {
        // Un avertissement par pipeline : chacun est un à-coup devant le public et un trou dans la préchauffe.
        tracing::warn!(
            pipeline = %label_of(&cached.descriptor),
            "pipeline compilé après Ready : la préchauffe ne couvre pas cette variante"
        );
    }
    *reported = total.max(*reported);
}

/// Branche le guetteur sur le monde de rendu. Sans monde de rendu (tests), la ressource reste « non attachée ».
pub(super) fn install(app: &mut App) {
    let watch = PipelineWatch::default();
    app.insert_resource(watch.clone());
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app
            .insert_resource(watch)
            .add_systems(Render, watch_pipelines.in_set(RenderSystems::Cleanup));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_detecteur_ne_compte_que_les_pipelines_apres_ready() {
        let watch = PipelineWatch::default();
        assert!(!watch.attached());
        watch.publish(40, 3);
        assert!(watch.attached());
        assert_eq!(watch.late(), 0, "désarmé : rien n'est tardif");
        watch.arm();
        assert_eq!(watch.late(), 0);
        watch.publish(40, 0);
        assert_eq!(watch.late(), 0, "journal vide tant que rien ne se compile");
        watch.publish(42, 2);
        assert_eq!(watch.late(), 2);
    }
}
