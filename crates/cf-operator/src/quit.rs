//! Garde-fou de sortie : on ne quitte jamais par accident pendant un set.
//!
//! Fermer une fenêtre (croix, Alt+F4, gestionnaire de fenêtres) ne la détruit pas : Bevy émet seulement un
//! `WindowCloseRequested`, que ce module transforme en demande de confirmation. `Échap` n'est lu nulle part ici et
//! `DefaultPlugins` n'installe aucun raccourci de sortie : Échap ne quitte donc jamais seul.

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowCloseRequested};
use cf_render::CfSet;

/// Fenêtre de laquelle vient la demande de fermeture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitSource {
    /// Fenêtre opérateur.
    Operator,
    /// Fenêtre projecteur (la fenêtre principale).
    Projector,
}

/// Comment la confirmation est demandée.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuitConfirmation {
    /// Dialogue egui dans la fenêtre opérateur (`OperatorPlugin`).
    Dialog,
    /// Sans fenêtre opérateur (`--no-operator`), aucune interface n'est permise sur le projecteur : on demande de
    /// fermer deux fois de suite, à moins de `within_s` secondes d'intervalle.
    DoubleClose {
        /// Délai maximal entre les deux demandes, en secondes.
        within_s: f64,
    },
}

/// Demande de confirmation en cours, lue par l'interface opérateur. Écrite uniquement par ce module.
#[derive(Resource, Debug, Default)]
pub struct QuitPrompt {
    /// Fenêtre qui a demandé à fermer ; `None` quand aucune confirmation n'est en attente.
    pub pending: Option<QuitSource>,
}

/// Réponse de l'opérateur au dialogue de sortie. Écrite par l'interface, lue par ce module.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitDecision {
    /// Quitter l'application (les deux fenêtres).
    Confirm,
    /// Rester dans l'application.
    Cancel,
}

/// Mémoire du mode [`QuitConfirmation::DoubleClose`] : heure (horloge réelle) de la dernière demande.
#[derive(Debug, Default)]
struct CloseGate {
    last_request_s: Option<f64>,
}

impl CloseGate {
    /// Enregistre une demande à l'instant `now_s` ; renvoie `true` si elle confirme la précédente.
    fn request(&mut self, now_s: f64, within_s: f64) -> bool {
        let confirmed = self.last_request_s.is_some_and(|last| now_s - last <= within_s);
        // Après une confirmation on repart de zéro ; sinon cette demande devient la référence.
        self.last_request_s = if confirmed { None } else { Some(now_s) };
        confirmed
    }
}

/// Installe le garde-fou de sortie. Remplace la fermeture automatique des fenêtres de Bevy : le binaire doit donc
/// régler `close_when_requested: false` et `ExitCondition::DontExit` sur `WindowPlugin`.
#[derive(Debug)]
pub struct QuitGuardPlugin {
    confirmation: QuitConfirmation,
}

impl QuitGuardPlugin {
    /// Confirmation par un dialogue dans la fenêtre opérateur.
    #[must_use]
    pub fn dialog() -> Self {
        Self {
            confirmation: QuitConfirmation::Dialog,
        }
    }

    /// Confirmation par une seconde demande de fermeture dans les 3 secondes (sans fenêtre opérateur).
    #[must_use]
    pub fn double_close() -> Self {
        Self {
            confirmation: QuitConfirmation::DoubleClose { within_s: 3.0 },
        }
    }
}

/// Mode de confirmation actif, copié dans une ressource pour les systèmes.
#[derive(Resource, Debug, Clone, Copy)]
struct ActiveConfirmation(QuitConfirmation);

impl Plugin for QuitGuardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<QuitPrompt>()
            .add_message::<QuitDecision>()
            .insert_resource(ActiveConfirmation(self.confirmation))
            .add_systems(
                Update,
                (handle_close_requests, apply_decision).chain().in_set(CfSet::Input),
            );
    }
}

fn handle_close_requests(
    mut requests: MessageReader<WindowCloseRequested>,
    primary: Query<(), With<PrimaryWindow>>,
    mode: Res<ActiveConfirmation>,
    time: Res<Time<Real>>,
    mut prompt: ResMut<QuitPrompt>,
    mut gate: Local<CloseGate>,
    mut exit: MessageWriter<AppExit>,
) {
    for request in requests.read() {
        let source = if primary.contains(request.window) {
            QuitSource::Projector
        } else {
            QuitSource::Operator
        };
        match mode.0 {
            QuitConfirmation::Dialog => {
                tracing::info!(?source, "demande de fermeture : confirmation attendue");
                prompt.pending = Some(source);
            }
            QuitConfirmation::DoubleClose { within_s } => {
                if gate.request(time.elapsed_secs_f64(), within_s) {
                    tracing::info!(?source, "fermeture confirmée par une seconde demande");
                    exit.write(AppExit::Success);
                } else {
                    tracing::warn!(?source, within_s, "fermer à nouveau dans ce délai pour quitter");
                }
            }
        }
    }
}

fn apply_decision(
    mut decisions: MessageReader<QuitDecision>,
    mut prompt: ResMut<QuitPrompt>,
    mut exit: MessageWriter<AppExit>,
) {
    for decision in decisions.read() {
        match decision {
            QuitDecision::Confirm => {
                tracing::info!("sortie confirmée par l'opérateur");
                prompt.pending = None;
                exit.write(AppExit::Success);
            }
            QuitDecision::Cancel => prompt.pending = None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(plugin: QuitGuardPlugin) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, plugin))
            .add_message::<WindowCloseRequested>();
        app
    }

    fn request_close(app: &mut App, window: Entity) {
        app.world_mut().write_message(WindowCloseRequested { window });
    }

    #[test]
    fn la_porte_double_demande_exige_deux_fermetures_proches() {
        let mut gate = CloseGate::default();
        assert!(!gate.request(10.0, 3.0), "la première demande ne quitte jamais");
        assert!(gate.request(12.0, 3.0), "la seconde, dans le délai, confirme");
        assert!(!gate.request(13.0, 3.0), "après confirmation on repart de zéro");
        assert!(
            !gate.request(20.0, 3.0),
            "trop tard : cette demande devient la nouvelle référence"
        );
        assert!(gate.request(21.0, 3.0));
    }

    #[test]
    fn fermer_une_fenetre_ne_quitte_pas_sans_confirmation() {
        let mut app = app(QuitGuardPlugin::dialog());
        let projector = app.world_mut().spawn(PrimaryWindow).id();
        request_close(&mut app, projector);
        app.update();
        assert!(app.should_exit().is_none());
        assert_eq!(app.world().resource::<QuitPrompt>().pending, Some(QuitSource::Projector));
    }

    #[test]
    fn la_demande_de_l_operateur_est_distinguee_du_projecteur() {
        let mut app = app(QuitGuardPlugin::dialog());
        let operator = app.world_mut().spawn_empty().id();
        request_close(&mut app, operator);
        app.update();
        assert_eq!(app.world().resource::<QuitPrompt>().pending, Some(QuitSource::Operator));
    }

    #[test]
    fn confirmer_quitte_et_annuler_garde_l_application() {
        let mut app = app(QuitGuardPlugin::dialog());
        let operator = app.world_mut().spawn_empty().id();
        request_close(&mut app, operator);
        app.update();

        app.world_mut().write_message(QuitDecision::Cancel);
        app.update();
        assert!(app.should_exit().is_none());
        assert_eq!(app.world().resource::<QuitPrompt>().pending, None);

        request_close(&mut app, operator);
        app.world_mut().write_message(QuitDecision::Confirm);
        app.update();
        assert_eq!(app.should_exit(), Some(AppExit::Success));
    }

    #[test]
    fn sans_operateur_une_seule_fermeture_ne_quitte_pas() {
        let mut app = app(QuitGuardPlugin::double_close());
        let projector = app.world_mut().spawn(PrimaryWindow).id();
        request_close(&mut app, projector);
        app.update();
        assert!(app.should_exit().is_none());
    }
}
