//! L'écriture automatique des réglages.

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, TaskPool, futures_lite::future};
use cf_show::persist::PersistError;

use super::Settings;
use super::debounce::Debounce;

/// Pas plus d'un avertissement par minute si le disque refuse l'écriture.
const WARN_EVERY_S: f64 = 60.0;

/// Où en est l'écriture.
#[derive(Resource, Default)]
pub(super) struct WriteState {
    debounce: Debounce,
    in_flight: Option<Task<Result<(), PersistError>>>,
    last_warn_s: Option<f64>,
}

/// `Last` : écrit les réglages une seconde après leur dernier changement, sur le pool d'E/S.
pub(super) fn save_when_changed(settings: Res<Settings>, mut state: ResMut<WriteState>, time: Res<Time<Real>>) {
    let now = time.elapsed_secs_f64();
    if let Some(task) = state.in_flight.as_mut()
        && let Some(result) = future::block_on(future::poll_once(task))
    {
        state.in_flight = None;
        if let Err(error) = result {
            let due = state.last_warn_s.is_none_or(|at| now - at >= WARN_EVERY_S);
            if due {
                state.last_warn_s = Some(now);
                tracing::warn!(%error, "réglages : écriture impossible");
            }
        }
    }
    let busy = state.in_flight.is_some();
    if state.debounce.step(settings.is_changed(), now, busy) {
        let value = settings.value.clone();
        let path = settings.path().to_path_buf();
        let pool = IoTaskPool::get_or_init(TaskPool::new);
        state.in_flight = Some(pool.spawn(async move { value.save(&path) }));
    }
}
