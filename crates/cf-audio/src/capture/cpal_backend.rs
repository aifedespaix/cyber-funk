//! Backend de capture sur le vrai matériel, via `cpal`.

use std::fmt;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, ErrorKind, SampleFormat, StreamConfig};

use super::{
    CaptureError, ConfigRange, DeviceSelector, InputBackend, InputDeviceInfo, InputFormat, OpenedInput, SampleType,
    choose_format, resolve_device,
};
use crate::feed::{FaultReporter, FeedFault, FeedWriter, InputSample, capacity_for, feed};
use crate::source::SourceKind;

/// Temps laissé à l'hôte audio pour démarrer un flux : au-delà, le superviseur réessaiera plutôt que de rester
/// bloqué (tous les backends ne respectent pas ce délai).
const OPEN_TIMEOUT: Duration = Duration::from_secs(2);

/// Capture sur l'hôte audio par défaut de la plateforme (`ALSA`/`PulseAudio`/`PipeWire`, `CoreAudio`, `WASAPI`).
pub struct CpalBackend {
    host: cpal::Host,
}

impl fmt::Debug for CpalBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CpalBackend").field("host", &self.host.id()).finish()
    }
}

impl Default for CpalBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CpalBackend {
    /// Backend sur l'hôte audio par défaut.
    #[must_use]
    pub fn new() -> Self {
        Self {
            host: cpal::default_host(),
        }
    }

    /// Les périphériques d'entrée avec leur description, dans l'ordre de l'hôte.
    fn enumerate(&self) -> Result<Vec<(cpal::Device, InputDeviceInfo)>, CaptureError> {
        let default_name = self.host.default_input_device().as_ref().and_then(device_name);
        let mut devices = Vec::new();
        for device in self.host.input_devices()? {
            // Un périphérique qui disparaît pendant l'énumération n'a plus de description : on l'ignore.
            let Some(name) = device_name(&device) else {
                continue;
            };
            let default_format = device.default_input_config().ok().and_then(|c| format_of(&c));
            let is_default = default_name.as_deref() == Some(name.as_str());
            devices.push((
                device,
                InputDeviceInfo {
                    name,
                    is_default,
                    default_format,
                },
            ));
        }
        Ok(devices)
    }
}

impl InputBackend for CpalBackend {
    type Stream = cpal::Stream;

    fn input_devices(&self) -> Result<Vec<InputDeviceInfo>, CaptureError> {
        Ok(self.enumerate()?.into_iter().map(|(_, info)| info).collect())
    }

    fn open(&self, selector: &DeviceSelector, ring_seconds: f32) -> Result<OpenedInput<cpal::Stream>, CaptureError> {
        let devices = self.enumerate()?;
        let infos: Vec<InputDeviceInfo> = devices.iter().map(|(_, info)| info.clone()).collect();
        let index = resolve_device(selector, &infos)?;
        let Some((device, info)) = devices.get(index) else {
            return Err(CaptureError::DeviceNotFound(selector.to_string()));
        };
        let ranges: Vec<ConfigRange> = device.supported_input_configs()?.map(|r| range_of(&r)).collect();
        let format = choose_format(info.default_format, &ranges)
            .ok_or_else(|| CaptureError::NoUsableFormat(info.name.clone()))?;
        let (writer, input) = feed(
            SourceKind::Capture,
            format.sample_rate,
            capacity_for(format.sample_rate, ring_seconds),
        )?;
        let config = StreamConfig {
            channels: format.channels,
            sample_rate: format.sample_rate,
            // Taille par défaut : une taille fixe plus petite baisserait la latence mais échoue sur certains pilotes.
            // À mesurer sur le matériel cible (J9).
            buffer_size: BufferSize::Default,
        };
        let stream = match format.sample {
            SampleType::I16 => build::<i16>(device, config, writer),
            SampleType::U16 => build::<u16>(device, config, writer),
            SampleType::I32 => build::<i32>(device, config, writer),
            SampleType::F32 => build::<f32>(device, config, writer),
        }?;
        stream.play()?;
        Ok(OpenedInput {
            stream,
            feed: input,
            device: info.name.clone(),
            format,
        })
    }
}

/// Construit le flux d'entrée typé.
///
/// Le callback de données ne fait qu'appeler [`FeedWriter::push_interleaved`] : conversion, mixage mono, copie dans
/// le ring `rtrb`, compteurs atomiques. Aucune allocation, aucun verrou, aucun journal, aucune I/O (voir la revue
/// ligne par ligne dans la PR J1-03).
fn build<S>(device: &cpal::Device, config: StreamConfig, mut writer: FeedWriter) -> Result<cpal::Stream, cpal::Error>
where
    S: InputSample + cpal::SizedSample,
{
    let channels = usize::from(config.channels);
    let faults = writer.fault_reporter();
    device.build_input_stream::<S, _, _>(
        config,
        move |data: &[S], _: &cpal::InputCallbackInfo| writer.push_interleaved(data, channels),
        move |error: cpal::Error| on_stream_error(&faults, error.kind()),
        Some(OPEN_TIMEOUT),
    )
}

/// Callback d'erreur : classe la panne et la range dans un atomique, rien d'autre. Le superviseur de capture la lit
/// et journalise depuis son propre thread. (L'objet `cpal::Error` reçu est construit par `cpal` ; sa libération ici
/// est la seule opération mémoire, sur un chemin exceptionnel, hors callback de données.)
fn on_stream_error(faults: &FaultReporter, kind: ErrorKind) {
    match kind {
        ErrorKind::Xrun => faults.report_xrun(),
        // Le flux continue : rerouté par le système, ou simplement sans priorité temps réel.
        ErrorKind::DeviceChanged | ErrorKind::RealtimeDenied => {}
        ErrorKind::DeviceNotAvailable => faults.report(FeedFault::DeviceLost),
        ErrorKind::StreamInvalidated => faults.report(FeedFault::Invalidated),
        // Tout le reste est traité comme fatal pour ce flux : le rouvrir coûte une courte coupure, l'ignorer
        // risquerait une capture figée sans le savoir.
        _ => faults.report(FeedFault::Backend),
    }
}

fn device_name(device: &cpal::Device) -> Option<String> {
    device.description().ok().map(|d| d.name().to_owned())
}

fn sample_type_of(format: SampleFormat) -> Option<SampleType> {
    match format {
        SampleFormat::I16 => Some(SampleType::I16),
        SampleFormat::U16 => Some(SampleType::U16),
        SampleFormat::I32 => Some(SampleType::I32),
        SampleFormat::F32 => Some(SampleType::F32),
        _ => None,
    }
}

fn format_of(config: &cpal::SupportedStreamConfig) -> Option<InputFormat> {
    Some(InputFormat {
        sample_rate: config.sample_rate(),
        channels: config.channels(),
        sample: sample_type_of(config.sample_format())?,
    })
}

fn range_of(range: &cpal::SupportedStreamConfigRange) -> ConfigRange {
    ConfigRange {
        channels: range.channels(),
        min_rate: range.min_sample_rate(),
        max_rate: range.max_sample_rate(),
        sample: sample_type_of(range.sample_format()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::feed;

    #[test]
    fn stream_errors_are_classified() {
        let (writer, input) = feed(SourceKind::Capture, 48_000, 16).unwrap();
        let faults = writer.fault_reporter();
        on_stream_error(&faults, ErrorKind::Xrun);
        on_stream_error(&faults, ErrorKind::DeviceChanged);
        on_stream_error(&faults, ErrorKind::RealtimeDenied);
        assert_eq!(input.stats().fault(), None);
        assert_eq!(input.stats().xruns(), 1);
        on_stream_error(&faults, ErrorKind::DeviceNotAvailable);
        assert_eq!(input.stats().fault(), Some(FeedFault::DeviceLost));
    }

    #[test]
    fn invalidated_and_unknown_errors_are_fatal() {
        let (writer, input) = feed(SourceKind::Capture, 48_000, 16).unwrap();
        on_stream_error(&writer.fault_reporter(), ErrorKind::StreamInvalidated);
        assert_eq!(input.stats().fault(), Some(FeedFault::Invalidated));
        let (writer, input) = feed(SourceKind::Capture, 48_000, 16).unwrap();
        on_stream_error(&writer.fault_reporter(), ErrorKind::BackendError);
        assert_eq!(input.stats().fault(), Some(FeedFault::Backend));
    }

    #[test]
    fn maps_only_handled_sample_formats() {
        assert_eq!(sample_type_of(SampleFormat::F32), Some(SampleType::F32));
        assert_eq!(sample_type_of(SampleFormat::U16), Some(SampleType::U16));
        assert_eq!(sample_type_of(SampleFormat::I24), None);
        assert_eq!(sample_type_of(SampleFormat::F64), None);
    }

    /// Sans matériel, l'énumération doit au moins ne pas paniquer (elle peut échouer proprement ou être vide).
    #[test]
    fn enumeration_never_panics() {
        let backend = CpalBackend::new();
        match backend.input_devices() {
            Ok(devices) => assert!(devices.iter().all(|d| !d.name.is_empty())),
            Err(error) => assert_ne!(error.to_string(), ""),
        }
    }
}
