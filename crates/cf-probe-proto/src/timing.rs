//! Délais partagés : l'application et le client doivent s'accorder, sinon le client abandonne alors que
//! l'application est encore en plein pas (et rejette le suivant).

use std::time::Duration;

/// Délai de base avant que l'application réponde `Error("délai")`.
pub const BASE_REPLY_TIMEOUT: Duration = Duration::from_secs(2);
/// Temps accordé à chaque image d'un `Step`, en plus du délai de base (rendu logiciel : plusieurs centaines de ms).
pub const STEP_FRAME_BUDGET: Duration = Duration::from_millis(250);
/// Plafond du délai d'un `Step`, quel que soit le nombre d'images.
pub const MAX_STEP_TIMEOUT: Duration = Duration::from_secs(300);
/// Plafond d'images d'un `Step` ; au-delà l'application écrête.
pub const MAX_STEP_FRAMES: u32 = 1200;
/// Marge ajoutée par le client : l'application répond (ok ou délai) toujours avant qu'il abandonne.
pub const CLIENT_MARGIN: Duration = Duration::from_secs(1);

/// Délai d'un `Step` côté application pour un délai de base `base` donné (plafonné, jamais sous `base`).
#[must_use]
pub fn step_timeout_with(base: Duration, frames: u32) -> Duration {
    (base + STEP_FRAME_BUDGET.saturating_mul(frames)).min(MAX_STEP_TIMEOUT.max(base))
}

/// Délai d'un `Step` côté application avec le délai de base par défaut.
#[must_use]
pub fn step_timeout(frames: u32) -> Duration {
    step_timeout_with(BASE_REPLY_TIMEOUT, frames)
}

/// Délai de lecture du client pour un `Step` : toujours strictement plus long que celui de l'application.
#[must_use]
pub fn client_step_read_timeout(frames: u32) -> Duration {
    step_timeout(frames.min(MAX_STEP_FRAMES)) + CLIENT_MARGIN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_client_attend_plus_longtemps_que_l_application() {
        for frames in [0, 1, 10, 100, 1199, MAX_STEP_FRAMES, MAX_STEP_FRAMES + 1, u32::MAX] {
            assert!(
                client_step_read_timeout(frames) > step_timeout(frames.min(MAX_STEP_FRAMES)),
                "{frames}"
            );
            assert!(client_step_read_timeout(frames) > step_timeout(frames), "{frames}");
        }
        assert_eq!(step_timeout(MAX_STEP_FRAMES), MAX_STEP_TIMEOUT);
    }
}
