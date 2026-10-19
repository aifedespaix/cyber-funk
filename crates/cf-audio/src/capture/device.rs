//! Description des périphériques d'entrée, choix par nom et choix du format de flux.
//!
//! Logique pure, indépendante de `cpal`, pour être testée sans matériel : le backend `cpal` et le backend synthétique
//! la partagent.

use std::fmt;

use super::CaptureError;
use crate::feed::ANALYSIS_SAMPLE_RATE;

/// Format d'échantillon d'un flux de capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleType {
    /// Entier signé 16 bits.
    I16,
    /// Entier non signé 16 bits (silence à 32 768).
    U16,
    /// Entier signé 32 bits.
    I32,
    /// Flottant 32 bits.
    F32,
}

impl SampleType {
    /// Rang de préférence (plus grand = mieux) : le flottant évite toute conversion, puis la résolution la plus fine.
    const fn preference(self) -> u8 {
        match self {
            Self::F32 => 3,
            Self::I32 => 2,
            Self::I16 => 1,
            Self::U16 => 0,
        }
    }
}

impl fmt::Display for SampleType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::I16 => "i16",
            Self::U16 => "u16",
            Self::I32 => "i32",
            Self::F32 => "f32",
        })
    }
}

/// Format d'un flux d'entrée ouvert ou proposé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputFormat {
    /// Fréquence d'échantillonnage (Hz).
    pub sample_rate: u32,
    /// Nombre de canaux entrelacés (mixés en mono dans le callback).
    pub channels: u16,
    /// Format des échantillons.
    pub sample: SampleType,
}

impl fmt::Display for InputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} Hz, {} canaux, {}", self.sample_rate, self.channels, self.sample)
    }
}

/// Un périphérique d'entrée tel que l'énumère un backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDeviceInfo {
    /// Nom affiché par le système : c'est lui qui sert à retrouver le périphérique après un débranchement.
    pub name: String,
    /// Périphérique d'entrée par défaut du système.
    pub is_default: bool,
    /// Format proposé par défaut, s'il est utilisable.
    pub default_format: Option<InputFormat>,
}

/// Quel périphérique ouvrir.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DeviceSelector {
    /// Le périphérique d'entrée par défaut du système au moment de l'ouverture (il peut changer après un
    /// débranchement : la reconnexion suit alors le nouveau défaut).
    #[default]
    Default,
    /// Un périphérique désigné par son nom, ou par un fragment de nom (voir [`resolve_device`]).
    Name(String),
}

impl fmt::Display for DeviceSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => f.write_str("périphérique par défaut"),
            Self::Name(name) => write!(f, "« {name} »"),
        }
    }
}

/// Trouve le périphérique demandé dans une liste énumérée ; renvoie son rang.
///
/// Par nom, on essaie dans l'ordre : nom exact, nom exact sans tenir compte de la casse, puis fragment contenu dans
/// un seul nom. Le fragment rend la reconnexion robuste aux noms qui changent d'un branchement à l'autre (Windows
/// préfixe « 2- », « 3- »… selon le port USB) : l'opérateur peut choisir « Scarlett » plutôt que le nom complet.
///
/// # Errors
///
/// [`CaptureError::NoDefaultDevice`], [`CaptureError::DeviceNotFound`] ou [`CaptureError::AmbiguousDevice`].
pub fn resolve_device(selector: &DeviceSelector, devices: &[InputDeviceInfo]) -> Result<usize, CaptureError> {
    let query = match selector {
        DeviceSelector::Default => {
            return devices
                .iter()
                .position(|d| d.is_default)
                .ok_or(CaptureError::NoDefaultDevice);
        }
        DeviceSelector::Name(query) => query,
    };
    if let Some(index) = devices.iter().position(|d| d.name == *query) {
        return Ok(index);
    }
    let lowered = query.to_lowercase();
    if let Some(index) = devices.iter().position(|d| d.name.to_lowercase() == lowered) {
        return Ok(index);
    }
    let matches: Vec<usize> = devices
        .iter()
        .enumerate()
        .filter(|(_, d)| d.name.to_lowercase().contains(&lowered))
        .map(|(i, _)| i)
        .collect();
    match matches.as_slice() {
        [] => Err(CaptureError::DeviceNotFound(query.clone())),
        [only] => Ok(*only),
        several => Err(CaptureError::AmbiguousDevice {
            query: query.clone(),
            candidates: several
                .iter()
                .filter_map(|&i| devices.get(i))
                .map(|d| d.name.clone())
                .collect(),
        }),
    }
}

/// Plage de configurations proposée par un périphérique (abstraction de `cpal::SupportedStreamConfigRange`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigRange {
    /// Nombre de canaux.
    pub channels: u16,
    /// Fréquence minimale (Hz).
    pub min_rate: u32,
    /// Fréquence maximale (Hz).
    pub max_rate: u32,
    /// Format des échantillons, `None` s'il n'est pas géré (i8, i24, f64…).
    pub sample: Option<SampleType>,
}

/// Choisit le format d'ouverture.
///
/// 1. Une plage qui contient 48 kHz évite tout rééchantillonnage : on la prend, en préférant le nombre de canaux du
///    format par défaut, puis le meilleur format d'échantillon, puis le moins de canaux (moins de travail au mixage).
/// 2. Sinon, le format par défaut du système s'il est géré.
/// 3. Sinon, la première plage gérée, à la fréquence la plus proche de 48 kHz.
#[must_use]
pub fn choose_format(default: Option<InputFormat>, ranges: &[ConfigRange]) -> Option<InputFormat> {
    let usable = || ranges.iter().filter_map(|r| r.sample.map(|sample| (r, sample)));
    let default_channels = default.map(|f| f.channels);
    let at_48k = usable()
        .filter(|(r, _)| r.min_rate <= ANALYSIS_SAMPLE_RATE && ANALYSIS_SAMPLE_RATE <= r.max_rate && r.channels > 0)
        .max_by_key(|(r, sample)| {
            (
                Some(r.channels) == default_channels,
                sample.preference(),
                std::cmp::Reverse(r.channels),
            )
        });
    if let Some((range, sample)) = at_48k {
        return Some(InputFormat {
            sample_rate: ANALYSIS_SAMPLE_RATE,
            channels: range.channels,
            sample,
        });
    }
    if let Some(format) = default.filter(|f| f.channels > 0 && f.sample_rate > 0) {
        return Some(format);
    }
    usable()
        .find(|(r, _)| r.channels > 0 && r.max_rate > 0)
        .map(|(range, sample)| InputFormat {
            sample_rate: ANALYSIS_SAMPLE_RATE.clamp(range.min_rate.max(1), range.max_rate),
            channels: range.channels,
            sample,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, is_default: bool) -> InputDeviceInfo {
        InputDeviceInfo {
            name: name.to_owned(),
            is_default,
            default_format: None,
        }
    }

    fn list() -> Vec<InputDeviceInfo> {
        vec![
            device("Microphone (Realtek)", true),
            device("Microphone (2- Scarlett 2i2 USB)", false),
            device("Ligne (Traktor Audio 6)", false),
            device("Ligne (traktor audio 6) ", false),
        ]
    }

    #[test]
    fn resolves_default_and_exact_names() {
        let devices = list();
        assert_eq!(resolve_device(&DeviceSelector::Default, &devices).unwrap(), 0);
        let exact = DeviceSelector::Name("Ligne (Traktor Audio 6)".into());
        assert_eq!(resolve_device(&exact, &devices).unwrap(), 2);
        let other_case = DeviceSelector::Name("MICROPHONE (REALTEK)".into());
        assert_eq!(resolve_device(&other_case, &devices).unwrap(), 0);
    }

    #[test]
    fn unique_fragment_survives_a_renamed_port() {
        let mut devices = list();
        let fragment = DeviceSelector::Name("scarlett 2i2".into());
        assert_eq!(resolve_device(&fragment, &devices).unwrap(), 1);
        // Rebranché sur un autre port : Windows renomme « 2- » en « 3- ».
        devices[1].name = "Microphone (3- Scarlett 2i2 USB)".into();
        assert_eq!(resolve_device(&fragment, &devices).unwrap(), 1);
    }

    #[test]
    fn reports_missing_and_ambiguous_devices() {
        let devices = list();
        let missing = DeviceSelector::Name("Focusrite".into());
        assert!(matches!(
            resolve_device(&missing, &devices),
            Err(CaptureError::DeviceNotFound(_))
        ));
        let ambiguous = DeviceSelector::Name("traktor".into());
        assert!(matches!(
            resolve_device(&ambiguous, &devices),
            Err(CaptureError::AmbiguousDevice { ref candidates, .. }) if candidates.len() == 2
        ));
        let no_default = vec![device("A", false)];
        assert!(matches!(
            resolve_device(&DeviceSelector::Default, &no_default),
            Err(CaptureError::NoDefaultDevice)
        ));
    }

    fn range(channels: u16, min_rate: u32, max_rate: u32, sample: Option<SampleType>) -> ConfigRange {
        ConfigRange {
            channels,
            min_rate,
            max_rate,
            sample,
        }
    }

    #[test]
    fn prefers_48k_with_default_channels_then_float() {
        let default = Some(InputFormat {
            sample_rate: 44_100,
            channels: 2,
            sample: SampleType::I16,
        });
        let ranges = [
            range(1, 8_000, 96_000, Some(SampleType::F32)),
            range(2, 8_000, 96_000, Some(SampleType::I16)),
            range(2, 8_000, 96_000, Some(SampleType::F32)),
            range(2, 8_000, 96_000, None),
        ];
        let chosen = choose_format(default, &ranges).unwrap();
        assert_eq!(
            chosen,
            InputFormat {
                sample_rate: 48_000,
                channels: 2,
                sample: SampleType::F32
            }
        );
    }

    #[test]
    fn falls_back_to_default_then_nearest_rate() {
        let default = InputFormat {
            sample_rate: 44_100,
            channels: 2,
            sample: SampleType::U16,
        };
        let only_44k = [range(2, 44_100, 44_100, Some(SampleType::U16))];
        assert_eq!(choose_format(Some(default), &only_44k), Some(default));
        let chosen = choose_format(None, &only_44k).unwrap();
        assert_eq!(chosen.sample_rate, 44_100);
        let high = [range(4, 88_200, 192_000, Some(SampleType::I32))];
        assert_eq!(choose_format(None, &high).unwrap().sample_rate, 88_200);
        let unusable = [range(2, 8_000, 96_000, None)];
        assert_eq!(choose_format(None, &unusable), None);
    }
}
