//! Sortie sur une vraie carte son, via `cpal`.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};

use super::{ActiveOutput, OutputBackend, PREFERRED_SAMPLE_RATE};
use crate::player::error::{PlayerError, Result};
use crate::player::render::Renderer;

/// Trames converties par passe quand la carte n'accepte pas du `f32` : tampon de conversion alloué une fois, avant
/// le démarrage du flux.
const CONVERT_FRAMES: usize = 1024;

/// Périphérique de sortie `cpal` et configuration choisie.
pub struct CpalOutput {
    device: cpal::Device,
    config: StreamConfig,
    format: SampleFormat,
    name: String,
}

impl std::fmt::Debug for CpalOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CpalOutput")
            .field("name", &self.name)
            .field("config", &self.config)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl CpalOutput {
    /// Périphérique de sortie par défaut de l'hôte par défaut.
    ///
    /// # Errors
    ///
    /// [`PlayerError::NoOutputDevice`] sans carte son, ou une erreur `cpal` si sa configuration est illisible.
    pub fn default_device() -> Result<Self> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or(PlayerError::NoOutputDevice)?;
        Self::from_device(device)
    }

    /// Premier périphérique de sortie dont le nom contient `name` (insensible à la casse).
    ///
    /// # Errors
    ///
    /// [`PlayerError::DeviceNotFound`] si aucun ne correspond.
    pub fn by_name(name: &str) -> Result<Self> {
        let wanted = name.to_lowercase();
        let device = cpal::default_host()
            .output_devices()?
            .find(|d| {
                d.description()
                    .is_ok_and(|desc| desc.name().to_lowercase().contains(&wanted))
            })
            .ok_or_else(|| PlayerError::DeviceNotFound(name.to_owned()))?;
        Self::from_device(device)
    }

    /// Noms des périphériques de sortie de l'hôte par défaut.
    ///
    /// # Errors
    ///
    /// Erreur `cpal` si l'énumération échoue.
    pub fn device_names() -> Result<Vec<String>> {
        Ok(cpal::default_host()
            .output_devices()?
            .filter_map(|d| d.description().ok().map(|desc| desc.name().to_owned()))
            .collect())
    }

    /// Nom du périphérique choisi.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Choisit 48 kHz en `f32` stéréo si le périphérique le permet, sinon sa configuration par défaut.
    fn from_device(device: cpal::Device) -> Result<Self> {
        let name = device
            .description()
            .map_or_else(|_| "?".to_owned(), |d| d.name().to_owned());
        let preferred = device
            .supported_output_configs()?
            .filter_map(|range| range.try_with_sample_rate(PREFERRED_SAMPLE_RATE))
            .min_by_key(|c| {
                // Moins bon = plus grand : on veut du f32, puis deux voies.
                (
                    u8::from(c.sample_format() != SampleFormat::F32),
                    c.channels().abs_diff(2),
                )
            });
        let supported = match preferred {
            Some(config) => config,
            None => device.default_output_config()?,
        };
        Ok(Self {
            config: supported.config(),
            format: supported.sample_format(),
            device,
            name,
        })
    }
}

impl OutputBackend for CpalOutput {
    fn sample_rate(&self) -> u32 {
        self.config.sample_rate
    }

    fn start(self: Box<Self>, renderer: Renderer) -> Result<Box<dyn ActiveOutput>> {
        let stream = match self.format {
            SampleFormat::F32 => build_f32(&self.device, &self.config, renderer)?,
            SampleFormat::I16 => build_converted::<i16>(&self.device, &self.config, renderer)?,
            SampleFormat::U16 => build_converted::<u16>(&self.device, &self.config, renderer)?,
            SampleFormat::I32 => build_converted::<i32>(&self.device, &self.config, renderer)?,
            SampleFormat::F64 => build_converted::<f64>(&self.device, &self.config, renderer)?,
            other => return Err(PlayerError::UnsupportedSampleFormat(other.to_string())),
        };
        stream.play()?;
        tracing::info!(device = %self.name, rate = self.config.sample_rate, channels = self.config.channels, format = %self.format, "sortie audio ouverte");
        Ok(Box::new(CpalStream { _stream: stream }))
    }
}

/// Flux `cpal` en cours ; le lâcher l'arrête.
struct CpalStream {
    _stream: cpal::Stream,
}

impl std::fmt::Debug for CpalStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CpalStream")
    }
}

impl ActiveOutput for CpalStream {}

/// Callback d'erreur du flux : ni journal ni allocation (il peut tourner sur le thread audio), on compte.
fn error_callback(renderer: &Renderer) -> impl FnMut(cpal::Error) + Send + 'static {
    let reporter = renderer.error_reporter();
    move |_err| reporter.report()
}

fn build_f32(device: &cpal::Device, config: &StreamConfig, mut renderer: Renderer) -> Result<cpal::Stream> {
    let channels = usize::from(config.channels);
    let on_error = error_callback(&renderer);
    // Callback temps réel : un seul appel, `Renderer::render`, qui n'alloue ni ne verrouille (voir render.rs).
    let stream = device.build_output_stream::<f32, _, _>(
        *config,
        move |data: &mut [f32], _| renderer.render(data, channels),
        on_error,
        None,
    )?;
    Ok(stream)
}

fn build_converted<T>(device: &cpal::Device, config: &StreamConfig, mut renderer: Renderer) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let channels = usize::from(config.channels);
    let on_error = error_callback(&renderer);
    // Alloué ici, une fois, avant le démarrage : le callback ne fait que le réutiliser.
    let mut scratch = vec![0.0_f32; CONVERT_FRAMES * channels];
    let stream = device.build_output_stream::<T, _, _>(
        *config,
        move |data: &mut [T], _| {
            for out in data.chunks_mut(scratch.len()) {
                let part = &mut scratch[..out.len()];
                renderer.render(part, channels);
                for (dst, src) in out.iter_mut().zip(part.iter()) {
                    *dst = T::from_sample(*src);
                }
            }
        },
        on_error,
        None,
    )?;
    Ok(stream)
}
