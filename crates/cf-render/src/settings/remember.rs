//! Ce que le logiciel retient : recopie des choix de l'opérateur dans [`Settings`], et retour au démarrage.
//!
//! Les conversions sont pures (testées sans Bevy) ; seuls les deux systèmes à la fin touchent l'`App`.

use bevy::prelude::*;
use cf_audio::capture::DeviceSelector;
use cf_show::persist::AppSettings;

use super::Settings;
use crate::audio::{AudioCommand, AudioConfig, AudioSource};
use crate::quality::QualityMode;

/// Le nom écrit dans le fichier pour une source audio.
#[must_use]
pub fn audio_source_name(source: AudioSource) -> &'static str {
    match source {
        AudioSource::Microphone => "Microphone",
        AudioSource::Line => "Line",
        AudioSource::Demo => "Demo",
    }
}

/// La source qu'un nom du fichier désigne ; un nom inconnu (fichier d'une autre version) n'en désigne aucune.
#[must_use]
pub fn audio_source_from_name(name: &str) -> Option<AudioSource> {
    match name {
        "Microphone" => Some(AudioSource::Microphone),
        "Line" => Some(AudioSource::Line),
        "Demo" => Some(AudioSource::Demo),
        _ => None,
    }
}

fn selector(name: Option<&String>) -> DeviceSelector {
    name.map_or(DeviceSelector::Default, |name| DeviceSelector::Name(name.clone()))
}

/// Applique les réglages relus à la configuration audio du démarrage.
///
/// `explicit_source` : la source demandée sur la ligne de commande, qui l'emporte. Un périphérique absent ce jour-là
/// n'est pas filtré ici : le nom reste écrit dans les réglages, et la capture se replie d'elle-même (J9-02).
pub fn apply_audio(settings: &AppSettings, config: &mut AudioConfig, explicit_source: Option<AudioSource>) {
    if let Some(source) = explicit_source.or_else(|| settings.audio_source.as_deref().and_then(audio_source_from_name))
    {
        config.initial_source = source;
    }
    config.mic_device = selector(settings.mic_device.as_ref());
    config.line_device = selector(settings.line_device.as_ref());
}

/// Le texte écrit pour un mode de qualité : `None` pour l'automatique, le niveau sinon.
#[must_use]
pub fn quality_name(mode: QualityMode) -> Option<String> {
    match mode {
        QualityMode::Auto => None,
        QualityMode::Fixed(level) => Some(level.to_string()),
    }
}

/// Le mode de qualité d'un texte du fichier ; tout ce qui n'est pas un niveau lisible donne l'automatique.
#[must_use]
pub fn quality_from_name(name: Option<&str>) -> QualityMode {
    name.and_then(|text| text.parse::<usize>().ok())
        .map_or(QualityMode::Auto, QualityMode::Fixed)
}

/// Écrit `new` dans `slot` seulement s'il diffère, pour ne pas réveiller l'écriture automatique pour rien.
fn set_if_different<T: PartialEq>(slot: &mut T, new: T) {
    if *slot != new {
        *slot = new;
    }
}

/// `Update` : recopie la source et les périphériques que l'opérateur choisit dans les réglages.
pub(super) fn remember_audio(mut commands: MessageReader<AudioCommand>, mut settings: ResMut<Settings>) {
    for command in commands.read() {
        match command {
            AudioCommand::SelectSource(source) => {
                let name = Some(audio_source_name(*source).to_owned());
                if settings.value.audio_source != name {
                    settings.value.audio_source = name;
                }
            }
            AudioCommand::SelectDevice { source, device } => {
                let name = match device {
                    DeviceSelector::Default => None,
                    DeviceSelector::Name(name) => Some(name.clone()),
                };
                let current = match source {
                    AudioSource::Microphone => &settings.value.mic_device,
                    AudioSource::Line => &settings.value.line_device,
                    AudioSource::Demo => continue,
                };
                if *current != name {
                    match source {
                        AudioSource::Microphone => settings.value.mic_device = name,
                        _ => settings.value.line_device = name,
                    }
                }
            }
            _ => {}
        }
    }
}

/// `Startup` : remet le mode de qualité fixé la dernière fois (l'automatique reste le défaut).
pub(super) fn restore_quality(settings: Res<Settings>, mut mode: ResMut<QualityMode>) {
    let wanted = quality_from_name(settings.value.quality.as_deref());
    set_if_different(&mut *mode, wanted);
}

/// `Update` : écrit dans les réglages le mode de qualité quand l'opérateur le change.
pub(super) fn remember_quality(mode: Res<QualityMode>, mut settings: ResMut<Settings>) {
    if !mode.is_changed() {
        return;
    }
    let name = quality_name(*mode);
    if settings.value.quality != name {
        settings.value.quality = name;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_sources_font_l_aller_retour() {
        for source in [AudioSource::Microphone, AudioSource::Line, AudioSource::Demo] {
            assert_eq!(audio_source_from_name(audio_source_name(source)), Some(source));
        }
        assert_eq!(audio_source_from_name("Satellite"), None);
    }

    #[test]
    fn la_ligne_de_commande_l_emporte_sur_la_source_retenue() {
        let settings = AppSettings {
            audio_source: Some("Line".into()),
            ..AppSettings::default()
        };
        let mut config = AudioConfig::default();
        apply_audio(&settings, &mut config, Some(AudioSource::Microphone));
        assert_eq!(config.initial_source, AudioSource::Microphone);
        apply_audio(&settings, &mut config, None);
        assert_eq!(config.initial_source, AudioSource::Line);
    }

    #[test]
    fn un_nom_de_source_inconnu_laisse_la_source_par_defaut() {
        let settings = AppSettings {
            audio_source: Some("Satellite".into()),
            ..AppSettings::default()
        };
        let mut config = AudioConfig::default();
        apply_audio(&settings, &mut config, None);
        assert_eq!(config.initial_source, AudioConfig::default().initial_source);
    }

    #[test]
    fn un_peripherique_retenu_est_passe_par_son_nom_sans_etre_efface() {
        let settings = AppSettings {
            line_device: Some("Carte absente aujourd'hui".into()),
            ..AppSettings::default()
        };
        let mut config = AudioConfig::default();
        apply_audio(&settings, &mut config, None);
        assert_eq!(
            config.line_device,
            DeviceSelector::Name("Carte absente aujourd'hui".into())
        );
        assert_eq!(config.mic_device, DeviceSelector::Default);
        assert_eq!(settings.line_device.as_deref(), Some("Carte absente aujourd'hui"));
    }

    #[test]
    fn la_qualite_fait_l_aller_retour_et_un_texte_illisible_donne_l_automatique() {
        assert_eq!(
            quality_from_name(quality_name(QualityMode::Fixed(2)).as_deref()),
            QualityMode::Fixed(2)
        );
        assert_eq!(quality_name(QualityMode::Auto), None);
        assert_eq!(quality_from_name(Some("tres haute")), QualityMode::Auto);
        assert_eq!(quality_from_name(None), QualityMode::Auto);
    }
}
