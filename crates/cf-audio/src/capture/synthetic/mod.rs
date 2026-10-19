//! Backend de capture synthétique : des périphériques simulés qui génèrent un signal connu en temps réel, que l'on
//! peut débrancher et rebrancher. Sert aux tests sans matériel et de signal d'essai pour vérifier une installation.
//!
//! Chaque flux ouvert lance un thread générateur qui joue le rôle du callback `cpal` : il appelle exactement le même
//! [`FeedWriter::push_interleaved`], au même rythme (un bloc de `block_frames` trames par période), dans le format
//! d'échantillon et le nombre de canaux du périphérique simulé.

mod generator;

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};

use super::{
    CaptureError, DeviceSelector, InputBackend, InputDeviceInfo, InputFormat, OpenedInput, SampleType, resolve_device,
};
use crate::feed::{capacity_for, feed};
use crate::source::SourceKind;

pub use generator::Signal;

use generator::generate;

/// Un périphérique d'entrée simulé.
#[derive(Debug, Clone, PartialEq)]
pub struct SyntheticDevice {
    /// Nom sous lequel il est énuméré.
    pub name: String,
    /// Format livré au « callback ».
    pub format: InputFormat,
    /// Trames par appel du « callback » (480 à 48 kHz = 10 ms, une valeur courante).
    pub block_frames: usize,
    /// Signal généré, identique sur tous les canaux.
    pub signal: Signal,
    /// S'arrête de livrer (sans panne) après ce nombre de trames, comme une source mise en pause.
    pub total_frames: Option<u64>,
}

impl SyntheticDevice {
    /// Périphérique simulé à blocs de 10 ms, sans limite de durée.
    #[must_use]
    pub fn new(name: &str, format: InputFormat, signal: Signal) -> Self {
        Self {
            name: name.to_owned(),
            format,
            block_frames: (format.sample_rate as usize / 100).max(1),
            signal,
            total_frames: None,
        }
    }
}

#[derive(Debug)]
struct Entry {
    device: SyntheticDevice,
    plugged: Arc<AtomicBool>,
}

/// Banc de périphériques simulés, partagé entre celui qui branche et débranche (le test) et le backend.
#[derive(Debug, Clone, Default)]
pub struct SyntheticBus {
    entries: Arc<Mutex<Vec<Entry>>>,
}

impl SyntheticBus {
    /// Banc vide.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ajoute un périphérique, branché. Le premier ajouté est le périphérique par défaut.
    pub fn add(&self, device: SyntheticDevice) {
        self.with_entries(|entries| {
            entries.push(Entry {
                device,
                plugged: Arc::new(AtomicBool::new(true)),
            });
        });
    }

    /// Débranche un périphérique : son flux ouvert signale [`FeedFault::DeviceLost`] et s'arrête, et il disparaît de
    /// l'énumération. Renvoie faux s'il n'existe pas.
    #[must_use = "faux si aucun périphérique ne porte ce nom"]
    pub fn unplug(&self, name: &str) -> bool {
        self.set_plugged(name, false)
    }

    /// Rebranche un périphérique débranché. Renvoie faux s'il n'existe pas.
    #[must_use = "faux si aucun périphérique ne porte ce nom"]
    pub fn replug(&self, name: &str) -> bool {
        self.set_plugged(name, true)
    }

    /// Un backend qui ouvre les périphériques de ce banc.
    #[must_use]
    pub fn backend(&self) -> SyntheticBackend {
        SyntheticBackend { bus: self.clone() }
    }

    fn set_plugged(&self, name: &str, plugged: bool) -> bool {
        self.with_entries(|entries| {
            entries.iter_mut().find(|e| e.device.name == name).is_some_and(|entry| {
                if plugged {
                    // Un rebranchement est un nouveau périphérique pour l'OS : nouveau drapeau, l'ancien flux reste mort.
                    entry.plugged = Arc::new(AtomicBool::new(true));
                } else {
                    entry.plugged.store(false, Ordering::Release);
                }
                true
            })
        })
    }

    fn with_entries<R>(&self, f: impl FnOnce(&mut Vec<Entry>) -> R) -> R {
        // Verrou hors temps réel (test, superviseur) ; un empoisonnement ne rend pas les données incohérentes.
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        f(&mut entries)
    }
}

/// Backend qui ouvre les périphériques d'un [`SyntheticBus`].
#[derive(Debug, Clone)]
pub struct SyntheticBackend {
    bus: SyntheticBus,
}

impl InputBackend for SyntheticBackend {
    type Stream = SyntheticStream;

    fn input_devices(&self) -> Result<Vec<InputDeviceInfo>, CaptureError> {
        Ok(self
            .bus
            .with_entries(|entries| plugged_infos(entries).into_iter().map(|(_, info)| info).collect()))
    }

    fn open(&self, selector: &DeviceSelector, ring_seconds: f32) -> Result<OpenedInput<SyntheticStream>, CaptureError> {
        let (device, plugged) = self.bus.with_entries(|entries| {
            let plugged_list = plugged_infos(entries);
            let infos: Vec<InputDeviceInfo> = plugged_list.iter().map(|(_, info)| info.clone()).collect();
            let index = resolve_device(selector, &infos)?;
            plugged_list
                .get(index)
                .and_then(|&(entry_index, _)| entries.get(entry_index))
                .map(|entry| (entry.device.clone(), Arc::clone(&entry.plugged)))
                .ok_or_else(|| CaptureError::DeviceNotFound(selector.to_string()))
        })?;
        let format = device.format;
        let (writer, input) = feed(
            SourceKind::Capture,
            format.sample_rate,
            capacity_for(format.sample_rate, ring_seconds),
        )?;
        let stop = Arc::new(AtomicBool::new(false));
        let name = device.name.clone();
        let thread_stop = Arc::clone(&stop);
        let join = thread::Builder::new()
            .name(format!("cf-synth {name}"))
            .spawn(move || match device.format.sample {
                SampleType::I16 => generate::<i16>(&device, writer, &plugged, &thread_stop),
                SampleType::U16 => generate::<u16>(&device, writer, &plugged, &thread_stop),
                SampleType::I32 => generate::<i32>(&device, writer, &plugged, &thread_stop),
                SampleType::F32 => generate::<f32>(&device, writer, &plugged, &thread_stop),
            })
            .map_err(|source| CaptureError::Spawn {
                name: "cf-synth",
                source,
            })?;
        Ok(OpenedInput {
            stream: SyntheticStream { stop, join: Some(join) },
            feed: input,
            device: name,
            format,
        })
    }
}

/// Les entrées branchées, avec leur rang dans le banc ; la première du banc est le défaut.
fn plugged_infos(entries: &[Entry]) -> Vec<(usize, InputDeviceInfo)> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.plugged.load(Ordering::Acquire))
        .map(|(i, e)| {
            (
                i,
                InputDeviceInfo {
                    name: e.device.name.clone(),
                    is_default: i == 0,
                    default_format: Some(e.device.format),
                },
            )
        })
        .collect()
}

/// Garde d'un flux synthétique : la lâcher arrête et rejoint son thread générateur.
pub struct SyntheticStream {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl fmt::Debug for SyntheticStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyntheticStream")
            .field("stopped", &self.stop.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl Drop for SyntheticStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            // Le générateur ne panique pas ; si c'était le cas, il n'y aurait rien à récupérer de plus ici.
            let _ = join.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::feed::FeedFault;

    fn format(sample_rate: u32, channels: u16, sample: SampleType) -> InputFormat {
        InputFormat {
            sample_rate,
            channels,
            sample,
        }
    }

    #[test]
    fn unplugged_devices_disappear_and_streams_report_loss() {
        let bus = SyntheticBus::new();
        bus.add(SyntheticDevice::new(
            "Synth A",
            format(48_000, 2, SampleType::I16),
            Signal::Silence,
        ));
        bus.add(SyntheticDevice::new(
            "Synth B",
            format(44_100, 1, SampleType::F32),
            Signal::Silence,
        ));
        let backend = bus.backend();
        assert_eq!(backend.input_devices().unwrap().len(), 2);
        let opened = backend.open(&DeviceSelector::Name("Synth A".into()), 0.5).unwrap();
        assert_eq!(opened.format.sample_rate, 48_000);
        let stats = Arc::clone(opened.feed.stats());
        assert!(bus.unplug("Synth A"));
        let names: Vec<String> = backend.input_devices().unwrap().into_iter().map(|d| d.name).collect();
        assert_eq!(names, ["Synth B"]);
        let deadline = Instant::now() + Duration::from_secs(2);
        while stats.fault().is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(stats.fault(), Some(FeedFault::DeviceLost));
        assert!(matches!(
            backend.open(&DeviceSelector::Name("Synth A".into()), 0.5),
            Err(CaptureError::DeviceNotFound(_))
        ));
        assert!(bus.replug("Synth A"));
        assert!(backend.open(&DeviceSelector::Name("Synth A".into()), 0.5).is_ok());
        assert!(!bus.unplug("absent"));
    }

    #[test]
    fn finite_source_stops_after_total_frames() {
        let bus = SyntheticBus::new();
        let mut device = SyntheticDevice::new("Synth", format(48_000, 2, SampleType::U16), Signal::Silence);
        device.total_frames = Some(1_000);
        device.block_frames = 256;
        bus.add(device);
        let opened = bus.backend().open(&DeviceSelector::Default, 0.5).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while opened.feed.stats().written() < 1_000 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        thread::sleep(Duration::from_millis(50));
        assert_eq!(opened.feed.stats().written(), 1_000);
        assert_eq!(opened.feed.stats().fault(), None);
    }
}
