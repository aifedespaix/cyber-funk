//! Sorties du lecteur. Le [`super::Player`] ne connaît que le trait [`OutputBackend`] : la carte son ([`CpalOutput`])
//! se remplace par une sortie nulle ([`NullOutput`], [`ManualOutput`]) pour tester décodage, playlist et modes de
//! lecture sans matériel.

mod cpal_output;
mod null;

use std::fmt;

pub use cpal_output::CpalOutput;
pub use null::{ManualDriver, ManualOutput, NullOutput};

use super::error::Result;
use super::render::Renderer;

/// Fréquence de sortie préférée (Hz) : celle du thread d'analyse (skill `realtime-audio`), ce qui évite un second
/// rééchantillonnage du signal envoyé à l'analyse.
pub const PREFERRED_SAMPLE_RATE: u32 = 48_000;

/// Un pilote de sortie : il annonce sa fréquence, puis reçoit le [`Renderer`] et l'appelle depuis son propre thread
/// (temps réel pour une vraie carte son).
pub trait OutputBackend: Send + 'static {
    /// Fréquence d'échantillonnage de la sortie (Hz). Le décodeur rééchantillonne les pistes vers elle.
    fn sample_rate(&self) -> u32;

    /// Démarre la sortie. La valeur rendue la garde en vie : la lâcher arrête la sortie.
    ///
    /// # Errors
    ///
    /// Rend une erreur si le flux ne peut pas être créé ou démarré.
    fn start(self: Box<Self>, renderer: Renderer) -> Result<Box<dyn ActiveOutput>>;
}

/// Une sortie démarrée ; sa destruction arrête le flux.
pub trait ActiveOutput: Send + fmt::Debug {}
