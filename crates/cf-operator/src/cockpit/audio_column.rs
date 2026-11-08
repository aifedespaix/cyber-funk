//! L'en-tête de la colonne de droite : source du son (Musiques ou Micro), sortie audio, entrée du micro, MIDI.

use bevy_egui::egui;
use cf_audio::capture::DeviceSelector;
use cf_render::audio::{AudioCommand, AudioSource, AudioStatus};

/// Ce que l'opérateur a fait dans l'en-tête pendant l'image.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct AudioHeaderOut {
    /// Ordres au moteur audio.
    pub commands: Vec<AudioCommand>,
    /// Nouvel état du contrôleur MIDI, s'il a été basculé.
    pub midi: Option<bool>,
}

/// Libellé d'un périphérique choisi.
pub(crate) fn device_label(device: &DeviceSelector) -> String {
    match device {
        DeviceSelector::Default => "Par défaut du système".to_owned(),
        DeviceSelector::Name(name) => name.clone(),
    }
}

/// La source dont l'entrée se choisit : le micro et la ligne ont un périphérique d'entrée, les musiques non.
pub(crate) fn input_source(source: AudioSource) -> Option<AudioSource> {
    matches!(source, AudioSource::Microphone | AudioSource::Line).then_some(source)
}

/// Dessine l'en-tête. `midi` : le contrôleur MIDI est écouté.
pub(crate) fn audio_header(ui: &mut egui::Ui, status: &AudioStatus, midi: bool) -> AudioHeaderOut {
    let mut out = AudioHeaderOut::default();
    ui.heading("Audio");
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.label("Source");
        for (source, label) in [(AudioSource::Demo, "Musiques"), (AudioSource::Microphone, "Micro")] {
            if ui
                .add(egui::Button::selectable(status.source == source, label))
                .clicked()
                && status.source != source
            {
                out.commands.push(AudioCommand::SelectSource(source));
            }
        }
        if status.source == AudioSource::Line {
            ui.add(egui::Button::selectable(true, "Ligne"));
        }
    });
    ui.horizontal(|ui| {
        ui.label("Sortie");
        device_combo(ui, "audio_output", &status.output_device, &status.outputs, |device| {
            out.commands.push(AudioCommand::SelectOutput(device));
        });
        if ui
            .small_button("⟳")
            .on_hover_text("Relire la liste des sorties audio")
            .clicked()
        {
            out.commands.push(AudioCommand::RefreshOutputs);
        }
    });
    if let Some(source) = input_source(status.source) {
        let current = if source == AudioSource::Line {
            &status.line_device
        } else {
            &status.mic_device
        };
        ui.horizontal(|ui| {
            ui.label("Entrée");
            device_combo(ui, "audio_input", current, &status.devices, |device| {
                out.commands.push(AudioCommand::SelectDevice { source, device });
            });
            if ui
                .small_button("⟳")
                .on_hover_text("Relire la liste des entrées audio")
                .clicked()
            {
                out.commands.push(AudioCommand::RefreshDevices);
            }
        });
    }
    ui.horizontal(|ui| {
        ui.label("MIDI");
        let label = if midi {
            "● Contrôleur activé"
        } else {
            "○ Contrôleur désactivé"
        };
        if ui
            .add(egui::Button::selectable(midi, label))
            .on_hover_text("Active ou coupe l'écoute des pads et potards MIDI.")
            .clicked()
        {
            out.midi = Some(!midi);
        }
    });
    for note in status.notes.iter().rev().take(2) {
        ui.colored_label(egui::Color32::from_rgb(230, 170, 60), note);
    }
    out
}

/// Liste déroulante de périphériques : le choix par défaut puis ceux que le système a listés.
fn device_combo(
    ui: &mut egui::Ui,
    salt: &str,
    current: &DeviceSelector,
    names: &[String],
    mut pick: impl FnMut(DeviceSelector),
) {
    egui::ComboBox::from_id_salt(salt)
        .width(ui.available_width() - 40.0)
        .selected_text(device_label(current))
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(
                    *current == DeviceSelector::Default,
                    device_label(&DeviceSelector::Default),
                )
                .clicked()
            {
                pick(DeviceSelector::Default);
            }
            for name in names {
                let selected = matches!(current, DeviceSelector::Name(chosen) if chosen == name);
                if ui.selectable_label(selected, name).clicked() {
                    pick(DeviceSelector::Name(name.clone()));
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn musiques_n_affiche_pas_l_entree() {
        assert_eq!(input_source(AudioSource::Demo), None);
    }

    #[test]
    fn micro_et_ligne_affichent_l_entree() {
        assert_eq!(input_source(AudioSource::Microphone), Some(AudioSource::Microphone));
        assert_eq!(input_source(AudioSource::Line), Some(AudioSource::Line));
    }

    #[test]
    fn un_peripherique_se_lit_en_clair() {
        assert_eq!(device_label(&DeviceSelector::Default), "Par défaut du système");
        assert_eq!(device_label(&DeviceSelector::Name("Casque".into())), "Casque");
    }
}
