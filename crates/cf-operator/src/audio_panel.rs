//! Panneau audio de la fenêtre opérateur : choix de la source, lecteur de démo, vumètres, tempo, section, intensité,
//! kick, compteurs, décalage visuel.
//!
//! Ce module ne mesure ni ne pilote rien lui-même : il lit `AudioStatus` et `AudioFeatures` de `cf-render` et écrit des
//! `AudioCommand` (messages) et le `VisualLatency`. `draw` ne touche pas à Bevy : il se teste sans écran.

use bevy_egui::egui;
use cf_render::audio::{
    AudioCommand, AudioFeatures, AudioSource, AudioStatus, CaptureState, DeviceSelector, MAX_LATENCY_MS, PlayMode,
    PlayerState, Section, TempoSource, VisualLatency,
};

/// Couleur de l'indicateur de kick au plus fort de l'impulsion.
const KICK_ON: egui::Color32 = egui::Color32::from_rgb(255, 80, 120);
/// Couleur de l'indicateur de kick au repos.
const KICK_OFF: egui::Color32 = egui::Color32::from_rgb(50, 50, 60);

/// Dessine le panneau. Les ordres de l'opérateur s'ajoutent à `commands` ; le décalage visuel est écrit sur place.
pub(crate) fn draw(
    ui: &mut egui::Ui,
    status: &AudioStatus,
    features: &AudioFeatures,
    latency: &mut VisualLatency,
    commands: &mut Vec<AudioCommand>,
) {
    source_group(ui, status, commands);
    if status.demo.is_some() || status.source == AudioSource::Demo {
        demo_group(ui, status, commands);
    }
    analysis_group(ui, status, features);
    latency_group(ui, latency);
    if !status.notes.is_empty() {
        ui.group(|ui| {
            for note in &status.notes {
                ui.colored_label(ui.visuals().warn_fg_color, note);
            }
        });
    }
}

fn source_group(ui: &mut egui::Ui, status: &AudioStatus, commands: &mut Vec<AudioCommand>) {
    ui.group(|ui| {
        ui.strong("Source");
        ui.horizontal(|ui| {
            for source in AudioSource::ALL {
                let demo_missing = source == AudioSource::Demo && status.demo.is_none();
                let response = ui.add_enabled(
                    !demo_missing,
                    egui::Button::new(source.label()).selected(status.source == source),
                );
                if response.clicked() && status.source != source {
                    commands.push(AudioCommand::SelectSource(source));
                }
            }
        });
        for (source, current) in [
            (AudioSource::Microphone, &status.mic_device),
            (AudioSource::Line, &status.line_device),
        ] {
            ui.horizontal(|ui| {
                ui.label(format!("{} :", source.label()));
                if let Some(device) = device_combo(ui, source.label(), current, &status.devices) {
                    commands.push(AudioCommand::SelectDevice { source, device });
                }
            });
        }
        ui.horizontal(|ui| {
            if ui.button("Actualiser les périphériques").clicked() {
                commands.push(AudioCommand::RefreshDevices);
            }
            ui.weak(capture_text(status.capture.as_ref(), status.source));
        });
    });
}

/// Liste des périphériques d'une source ; renvoie le choix quand l'opérateur en change.
fn device_combo(ui: &mut egui::Ui, id: &str, current: &DeviceSelector, devices: &[String]) -> Option<DeviceSelector> {
    let mut chosen = None;
    egui::ComboBox::from_id_salt(("audio_device", id))
        .selected_text(selector_text(current))
        .width(260.0)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(*current == DeviceSelector::Default, selector_text(&DeviceSelector::Default))
                .clicked()
            {
                chosen = Some(DeviceSelector::Default);
            }
            for name in devices {
                let selected = matches!(current, DeviceSelector::Name(n) if n == name);
                if ui.selectable_label(selected, name).clicked() {
                    chosen = Some(DeviceSelector::Name(name.clone()));
                }
            }
        });
    chosen.filter(|choice| choice != current)
}

fn selector_text(selector: &DeviceSelector) -> String {
    match selector {
        DeviceSelector::Default => "périphérique par défaut".to_owned(),
        DeviceSelector::Name(name) => name.clone(),
    }
}

/// État de la capture en une ligne.
fn capture_text(capture: Option<&CaptureState>, source: AudioSource) -> String {
    match capture {
        None if source.is_capture() => "entrée non ouverte".to_owned(),
        None => "entrée fermée (ouverte au premier choix du micro ou de la ligne)".to_owned(),
        Some(CaptureState::Starting) => "entrée : démarrage".to_owned(),
        Some(CaptureState::Connected { device, format }) => format!("entrée : {device} ({format})"),
        Some(CaptureState::Waiting { reason, attempts }) => {
            format!("entrée absente, nouvel essai ({attempts}) : {reason}")
        }
        Some(CaptureState::Stopped) => "entrée arrêtée".to_owned(),
    }
}

fn demo_group(ui: &mut egui::Ui, status: &AudioStatus, commands: &mut Vec<AudioCommand>) {
    let Some(demo) = &status.demo else {
        return;
    };
    ui.group(|ui| {
        ui.strong("Lecteur de démo");
        let current = demo.track.and_then(|i| status.tracks.get(i));
        ui.label(current.map_or_else(
            || "aucune piste".to_owned(),
            |t| {
                if t.artist.is_empty() {
                    t.title.clone()
                } else {
                    format!("{} - {}", t.artist, t.title)
                }
            },
        ));
        ui.horizontal(|ui| {
            if ui.button("Précédent").clicked() {
                commands.push(AudioCommand::DemoPrev);
            }
            if ui.button(if demo.playing { "Pause" } else { "Lecture" }).clicked() {
                commands.push(AudioCommand::DemoToggle);
            }
            if ui.button("Suivant").clicked() {
                commands.push(AudioCommand::DemoNext);
            }
            ui.label(position_text(demo));
        });
        if !status.tracks.is_empty() {
            let mut picked = None;
            egui::ComboBox::from_id_salt("audio_track")
                .selected_text("Choisir une piste")
                .width(260.0)
                .show_ui(ui, |ui| {
                    for (i, track) in status.tracks.iter().enumerate() {
                        if ui.selectable_label(demo.track == Some(i), &track.title).clicked() {
                            picked = Some(i);
                        }
                    }
                });
            if let Some(index) = picked {
                commands.push(AudioCommand::DemoSelect(index));
            }
        }
        let mut volume = demo.volume;
        if ui
            .add(egui::Slider::new(&mut volume, 0.0..=1.0).text("Volume"))
            .changed()
        {
            commands.push(AudioCommand::DemoVolume(volume));
        }
        ui.weak(format!("Mode : {}", mode_text(demo.mode)));
    });
}

fn mode_text(mode: PlayMode) -> &'static str {
    match mode {
        PlayMode::Random => "aléatoire",
        PlayMode::Sequence => "dans l'ordre",
        PlayMode::Loop => "boucle",
    }
}

/// « 1:05 / 3:30 » (durée inconnue : « 1:05 »).
fn position_text(demo: &PlayerState) -> String {
    let clock = |s: f64| {
        let total = s.max(0.0) as u64;
        format!("{}:{:02}", total / 60, total % 60)
    };
    demo.duration_s.map_or_else(
        || clock(demo.position_s),
        |d| format!("{} / {}", clock(demo.position_s), clock(d)),
    )
}

fn analysis_group(ui: &mut egui::Ui, status: &AudioStatus, features: &AudioFeatures) {
    ui.group(|ui| {
        ui.strong("Analyse");
        ui.horizontal(|ui| {
            kick_light(ui, features.pulse);
            ui.label(if features.live { "signal présent" } else { "pas de signal" });
        });
        egui::Grid::new("audio_meters").num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
            meter(ui, "Basses", features.bass);
            meter(ui, "Médiums", features.mid);
            meter(ui, "Aigus", features.high);
            meter(ui, "Intensité", features.intensity);
        });
        ui.label(format!(
            "BPM : {} ({})   Section : {}",
            bpm_text(features),
            tempo_source_text(features.tempo_source),
            section_text(features.section)
        ));
        ui.weak(counters_text(status));
    });
}

/// Pastille qui s'allume à chaque kick et s'éteint avec l'impulsion (`pulse`, 1 → 0).
fn kick_light(ui: &mut egui::Ui, pulse: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 8.0, KICK_OFF.lerp_to_gamma(KICK_ON, pulse.clamp(0.0, 1.0)));
    ui.label("Kick");
}

fn meter(ui: &mut egui::Ui, label: &str, value: f32) {
    ui.label(label);
    ui.add(
        egui::ProgressBar::new(value.clamp(0.0, 1.0))
            .desired_width(240.0)
            .show_percentage(),
    );
    ui.end_row();
}

fn bpm_text(features: &AudioFeatures) -> String {
    features
        .bpm
        .map_or_else(|| "—".to_owned(), |bpm| format!("{bpm:.1}"))
}

fn tempo_source_text(source: TempoSource) -> &'static str {
    match source {
        TempoSource::Clock => "horloge MIDI",
        TempoSource::Link => "Link",
        TempoSource::Audio => "audio",
        TempoSource::Tap => "tap",
    }
}

fn section_text(section: Section) -> &'static str {
    match section {
        Section::Calm => "calme",
        Section::Groove => "groove",
        Section::Peak => "sommet",
    }
}

/// Compteurs d'incidents : sous-alimentations du lecteur, pertes d'analyse, latence.
fn counters_text(status: &AudioStatus) -> String {
    let underruns = status.demo.as_ref().map_or(0, |d| d.underruns);
    let analysis = &status.analysis;
    format!(
        "Sous-alimentations : {underruns} · échantillons jetés : {} · événements perdus : {} · latence d'analyse : {:.1} ms (max {:.1} ms)",
        analysis.input_dropped + status.demo.as_ref().map_or(0, |d| d.analysis_dropped),
        analysis.dropped_events,
        analysis.latency_last.as_secs_f64() * 1000.0,
        analysis.latency_max.as_secs_f64() * 1000.0,
    )
}

fn latency_group(ui: &mut egui::Ui, latency: &mut VisualLatency) {
    ui.group(|ui| {
        ui.strong("Décalage visuel");
        let mut ms = latency.ms();
        if ui
            .add(egui::Slider::new(&mut ms, 0.0..=MAX_LATENCY_MS).suffix(" ms"))
            .changed()
        {
            latency.set_ms(ms);
        }
        ui.weak("Retarde l'image pour la recaler sur un son qui arrive plus tard (sonorisation, projecteur).");
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use cf_render::audio::{AnalysisStatsSnapshot, Track};

    use super::*;

    fn track(title: &str) -> Track {
        Track {
            title: title.into(),
            artist: "Artiste".into(),
            path: title.into(),
            local: false,
        }
    }

    fn demo_state() -> PlayerState {
        PlayerState {
            playing: true,
            track: Some(1),
            position_s: 65.0,
            position_frames: 65 * 48_000,
            duration_s: Some(210.0),
            volume: 0.8,
            muted: false,
            mode: PlayMode::Random,
            rendered_frames: 0,
            underruns: 3,
            analysis_dropped: 0,
            device_errors: 0,
        }
    }

    fn run(status: &AudioStatus, features: &AudioFeatures, latency: &mut VisualLatency) -> Vec<AudioCommand> {
        let ctx = egui::Context::default();
        let mut commands = Vec::new();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            draw(ui, status, features, latency, &mut commands);
        });
        output.drop_without_applying_deltas();
        commands
    }

    #[test]
    fn le_panneau_se_dessine_sans_pont_et_sans_ordre() {
        let mut latency = VisualLatency::default();
        let commands = run(&AudioStatus::default(), &AudioFeatures::default(), &mut latency);
        assert!(commands.is_empty());
    }

    #[test]
    fn le_panneau_se_dessine_avec_une_demo_en_cours() {
        let status = AudioStatus {
            source: AudioSource::Demo,
            demo: Some(demo_state()),
            tracks: Arc::from(vec![track("Un"), track("Deux")]),
            devices: vec!["Micro A".into(), "Ligne B".into()],
            notes: vec!["une note".into()],
            analysis: AnalysisStatsSnapshot {
                latency_last: Duration::from_millis(8),
                ..AnalysisStatsSnapshot::default()
            },
            ..AudioStatus::default()
        };
        let mut latency = VisualLatency::new(120.0);
        let commands = run(&status, &AudioFeatures::default(), &mut latency);
        assert!(commands.is_empty(), "rien n'est cliqué : {commands:?}");
        assert!((latency.ms() - 120.0).abs() < f32::EPSILON);
    }

    #[test]
    fn les_textes_disent_l_etat() {
        assert_eq!(position_text(&demo_state()), "1:05 / 3:30");
        assert_eq!(bpm_text(&AudioFeatures::default()), "—");
        assert!(capture_text(None, AudioSource::Demo).contains("fermée"));
        assert!(capture_text(None, AudioSource::Line).contains("non ouverte"));
        let status = AudioStatus {
            demo: Some(demo_state()),
            ..AudioStatus::default()
        };
        assert!(counters_text(&status).contains("Sous-alimentations : 3"));
        assert_eq!(selector_text(&DeviceSelector::Default), "périphérique par défaut");
    }
}
