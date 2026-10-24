//! Les raccords qui ne vivent dans aucun plugin (J8-07) : ce que seul le binaire connaît, parce qu'il est le seul à voir
//! à la fois le chargement (`WarmupPlugin`) et la qualité adaptative (`QualityPlugin`).
//!
//! Les autres raccords sont dans les plugins concernés : `CurrentShowFx` dans `ShowPlugin`, `QualityChanged` vers
//! `NeonWorld` dans `NeonWorldPlugin`, le chef du spectacle et `drop_planned` du côté du réalisateur.

use bevy::prelude::*;
use cf_render::{AppState, CfSet, QualityIdle};

/// Installe les raccords du binaire.
#[derive(Debug, Default, Clone, Copy)]
pub struct WiringPlugin;

impl Plugin for WiringPlugin {
    fn build(&self, app: &mut App) {
        // `Input` précède `Present`, où `update_quality` lit `QualityIdle`.
        app.add_systems(Update, idle_while_loading.in_set(CfSet::Input));
    }
}

/// Pendant `Loading`, l'image est fixe (rideau noir) et les images sont longues (compilation des pipelines) : la
/// qualité adaptative ne doit pas y voir une machine trop lente et descendre d'un cran avant le premier plan. `Idle`
/// ramène aussitôt la netteté pleine (`settle` de Sillage) ; la mesure reprend la main à `Ready`.
fn idle_while_loading(state: Option<Res<State<AppState>>>, mut idle: ResMut<QualityIdle>) {
    let loading = state.is_some_and(|state| *state.get() == AppState::Loading);
    // Écriture conditionnelle : pas de détection de changement déclenchée à vide.
    if idle.0 != loading {
        idle.0 = loading;
    }
}

#[cfg(test)]
mod tests {
    use bevy::state::app::StatesPlugin;

    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, cf_render::CorePlugin))
            .init_state::<AppState>()
            .init_resource::<QualityIdle>()
            .add_plugins(WiringPlugin);
        app
    }

    #[test]
    fn la_qualite_est_au_repos_pendant_le_chargement_seulement() {
        let mut app = app();
        app.update();
        assert!(app.world().resource::<QualityIdle>().0, "Loading : au repos");
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Ready);
        app.update();
        app.update();
        assert!(!app.world().resource::<QualityIdle>().0, "Ready : la mesure décide");
    }
}
