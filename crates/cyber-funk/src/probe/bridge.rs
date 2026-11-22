//! Le pont côté Bevy : vide les requêtes de la sonde et les traduit en messages `ControlAction`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bevy::prelude::*;
use cf_control::{Action, Level, TimedAction};
use cf_probe_proto::{ProbeReply, ProbeRequest};
use cf_render::{ControlAction, ControlBridge, ControlsStatus, submit_action};

use super::server::Pending;

/// Requêtes lues au plus par image : un client bavard ne retient pas l'image (le reste attend la suivante).
const MAX_REQUESTS_PER_FRAME: usize = 64;

/// L'extrémité Bevy du canal du serveur. Le `Mutex` ne sert qu'à rendre le `Receiver` `Sync` (une ressource doit
/// l'être) : un seul système le lit, le verrou n'est jamais disputé.
#[derive(Resource, Debug)]
pub struct ProbeInbox(pub Mutex<Receiver<Pending>>);

impl ProbeInbox {
    /// Enveloppe l'extrémité de lecture du serveur.
    #[must_use]
    pub fn new(receiver: Receiver<Pending>) -> Self {
        Self(Mutex::new(receiver))
    }
}

/// Compteurs de la sonde, lisibles côté Bevy (panneau, tests) ; écrits par les threads du serveur.
#[derive(Resource, Debug, Clone, Default)]
pub struct ProbeStats {
    /// Lignes reçues puis refusées : JSON invalide ou ligne trop longue.
    pub malformed: Arc<AtomicU64>,
}

impl ProbeStats {
    /// Nombre de lignes refusées depuis le début.
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "lecture prévue par le panneau et la sonde d état (tâches suivantes)"
        )
    )]
    #[must_use]
    pub fn malformed(&self) -> u64 {
        self.malformed.load(Ordering::Relaxed)
    }
}

/// Vide le canal sans jamais bloquer (`try_recv`) et répond à chaque requête.
///
/// Sans `ControlBridge` (application de test), le message est publié directement, horodaté par le temps de simulation.
pub fn drain_probe(
    inbox: Res<ProbeInbox>,
    time: Option<Res<Time<Virtual>>>,
    mut bridge: Option<ResMut<ControlBridge>>,
    mut status: Option<ResMut<ControlsStatus>>,
    mut out: MessageWriter<ControlAction>,
) {
    for _ in 0..MAX_REQUESTS_PER_FRAME {
        // Un verrou empoisonné ne peut venir que d'un panic ailleurs : on suspend la sonde plutôt que de planter.
        let Ok(receiver) = inbox.0.lock() else {
            return;
        };
        let pending = match receiver.try_recv() {
            Ok(pending) => pending,
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => return,
        };
        drop(receiver);
        // Le client a déjà reçu `Error("délai")` : appliquer maintenant doublerait l'effet de son nouvel essai.
        if Instant::now() > pending.deadline {
            tracing::warn!("sonde : requête périmée jetée");
            continue;
        }
        let reply = match pending.request {
            ProbeRequest::Ping => ProbeReply::Pong,
            ProbeRequest::SendAction(action) => match validate(&action) {
                Ok(()) => {
                    // Même chemin que le clavier et l'opérateur : le tap/arbitre de tempo est routé, et le chef du
                    // spectacle (donc le limiteur photosensible) voit l'action comme n'importe quelle autre.
                    if let (Some(bridge), Some(status)) = (bridge.as_deref_mut(), status.as_deref_mut()) {
                        submit_action(bridge, status, &mut out, action);
                    } else {
                        let time_s = time.as_ref().map_or(0.0, |time| time.elapsed_secs_f64());
                        out.write(ControlAction(TimedAction { time_s, action }));
                    }
                    ProbeReply::Ack
                }
                Err(why) => ProbeReply::Error(why),
            },
            // Branchées par les tâches suivantes (état, capture, pas à pas, trace de synchronisation).
            ProbeRequest::State
            | ProbeRequest::Capture { .. }
            | ProbeRequest::Step { .. }
            | ProbeRequest::StartSyncRecording
            | ProbeRequest::StopSyncRecording => ProbeReply::Error("non implémenté".into()),
        };
        // Le client a pu partir ou expirer : sa réponse n'a plus de destinataire, ce n'est pas une erreur.
        let _ = pending.reply.send(reply);
    }
}

/// Refuse les valeurs hors bornes au lieu de les ramener silencieusement dans l'intervalle : une sonde qui écrête
/// masquerait un test qui envoie une valeur absurde.
fn validate(action: &Action) -> Result<(), String> {
    match action {
        Action::Master(level) | Action::Macro { value: level, .. } | Action::LayerFade { value: level, .. } => {
            validate_level(*level)
        }
        Action::NudgePhase { beats } if !beats.is_finite() => Err(format!("décalage non fini : {beats}")),
        _ => Ok(()),
    }
}

fn validate_level(level: Level) -> Result<(), String> {
    match level {
        Level::Set(v) if !(0.0..=1.0).contains(&v) => Err(format!("niveau hors de 0..1 : {v}")),
        Level::Shift(v) if !(-1.0..=1.0).contains(&v) => Err(format!("décalage hors de -1..1 : {v}")),
        Level::Set(_) | Level::Shift(_) => Ok(()),
    }
}
