//! L'ordre de l'image, commun à tous les plugins de cyber-funk.
//!
//! Sillage appelait ses ~40 systèmes à la main, dans un ordre figé (`sillage-scene.ts`). Ici l'ordre est porté par
//! cinq ensembles chaînés ; à l'intérieur d'un ensemble, le scheduler de Bevy parallélise librement.

use bevy::prelude::*;

/// Les étapes de l'image, dans l'ordre où elles s'exécutent pendant `Update`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CfSet {
    /// Entrées : `Action` (clavier, MIDI, OSC), `AudioFeatures` du thread d'analyse.
    Input,
    /// Chef du spectacle : publie le `ShowState` de l'image.
    Show,
    /// Le monde : voiture, trafic, cascades, événements, streaming des tronçons.
    Simulate,
    /// Le réalisateur : pose de la caméra.
    Direct,
    /// Présentation : palettes, effets, couches, interface opérateur.
    Present,
}

impl CfSet {
    /// Tous les ensembles, dans l'ordre d'exécution.
    pub const ORDER: [Self; 5] = [Self::Input, Self::Show, Self::Simulate, Self::Direct, Self::Present];
}

/// Déclare l'ordre de l'image. À ajouter une fois, avant les autres plugins de cyber-funk.
#[derive(Debug, Default)]
pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                CfSet::Input,
                CfSet::Show,
                CfSet::Simulate,
                CfSet::Direct,
                CfSet::Present,
            )
                .chain(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Trace(Vec<CfSet>);

    fn mark(set: CfSet) -> impl FnMut(ResMut<Trace>) {
        move |mut trace: ResMut<Trace>| trace.0.push(set)
    }

    #[test]
    fn les_ensembles_s_executent_dans_l_ordre() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CorePlugin)).init_resource::<Trace>();
        // Ajoutés à l'envers : seul l'ordre des ensembles peut les remettre dans le bon sens.
        for set in CfSet::ORDER.into_iter().rev() {
            app.add_systems(Update, mark(set).in_set(set));
        }
        app.update();
        assert_eq!(app.world().resource::<Trace>().0, CfSet::ORDER);
    }
}
