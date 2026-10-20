//! Panneau des contrôles de la fenêtre opérateur : où en est le spectacle (créneau, cue), ports MIDI vus, dernier
//! message, dernière action, source de tempo.
//!
//! Lecture seule : ce module lit `CurrentShow`, `ControlsStatus` et `AudioFeatures`. `draw` ne touche pas à Bevy et se
//! teste sans écran.

use bevy_egui::egui;
use cf_render::audio::{AudioFeatures, TempoSource};
use cf_render::{ControlsStatus, LastInput};
use cf_show::director::ShowState;

/// Dessine le panneau. `show` est `None` tant qu'aucun spectacle n'est chargé.
pub(crate) fn draw(ui: &mut egui::Ui, show: Option<&ShowState>, status: &ControlsStatus, features: &AudioFeatures) {
    show_group(ui, show);
    controls_group(ui, status);
    tempo_group(ui, status, features);
    if !status.notes.is_empty() {
        card(ui, |ui| {
            for note in &status.notes {
                ui.colored_label(ui.visuals().warn_fg_color, note);
            }
        });
    }
}

/// Cadre pleine largeur, comme ceux du panneau audio.
fn card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.group(|ui| {
        ui.set_min_width(ui.available_width());
        add_contents(ui);
    });
}

fn show_group(ui: &mut egui::Ui, show: Option<&ShowState>) {
    card(ui, |ui| {
        ui.strong("Spectacle");
        let Some(state) = show else {
            ui.weak("Aucun show chargé (lancer avec --show).");
            return;
        };
        // Numéros affichés à partir de 1 : c'est ce que l'opérateur lit sur sa feuille de conduite.
        ui.label(format!(
            "Créneau {} · cue {} · mode {:?}",
            state.slot + 1,
            state.cue + 1,
            state.mode
        ));
        if let Some(pending) = &state.pending {
            ui.label(format!(
                "Appelé : cue {} (entrée {:?}, depuis {:.1} s)",
                pending.cue + 1,
                pending.entry,
                pending.waited
            ));
        }
        if let Some(handover) = &state.handover {
            ui.label(format!(
                "Passation : créneau {} vers {} ({:?})",
                handover.from + 1,
                handover.to + 1,
                handover.phase
            ));
        }
        if state.hold {
            ui.colored_label(ui.visuals().warn_fg_color, "Hold : avancée automatique figée");
        }
        if state.blackout_on {
            ui.colored_label(ui.visuals().warn_fg_color, "Noir demandé");
        }
    });
}

fn controls_group(ui: &mut egui::Ui, status: &ControlsStatus) {
    card(ui, |ui| {
        ui.strong("Contrôles");
        ui.label(ports_text(status));
        ui.label(format!("Dernier message : {}", input_text(status.last_input.as_ref())));
        let action = status.last_action.as_ref().map_or_else(
            || "—".to_owned(),
            |(time_s, action)| format!("{action:?} (à {time_s:.1} s)"),
        );
        ui.label(format!("Dernière action : {action}"));
        if status.midi_running {
            ui.weak(format!(
                "MIDI : {} messages, {} jetés",
                status.midi_delivered, status.midi_dropped
            ));
        }
    });
}

fn tempo_group(ui: &mut egui::Ui, status: &ControlsStatus, features: &AudioFeatures) {
    card(ui, |ui| {
        ui.strong("Tempo");
        ui.label(format!(
            "Source active : {} · {} BPM · confiance {:.0} %",
            source_text(features.tempo_source),
            features.bpm.map_or_else(|| "—".to_owned(), |bpm| format!("{bpm:.1}")),
            features.confidence * 100.0
        ));
        ui.weak(format!(
            "Horloge MIDI : {} ({:?}) · tap : {}",
            bpm_text(status.clock_bpm),
            status.transport,
            bpm_text(status.tap_bpm)
        ));
    });
}

fn bpm_text(bpm: Option<f64>) -> String {
    bpm.map_or_else(|| "—".to_owned(), |bpm| format!("{bpm:.1} BPM"))
}

fn source_text(source: TempoSource) -> &'static str {
    match source {
        TempoSource::Clock => "horloge MIDI",
        TempoSource::Link => "Link",
        TempoSource::Audio => "audio",
        TempoSource::Tap => "tap",
    }
}

/// Les ports MIDI : ouverts d'abord, puis le nombre de ports présents.
fn ports_text(status: &ControlsStatus) -> String {
    if !status.midi_running {
        return "MIDI : désactivé".to_owned();
    }
    if status.ports.connected.is_empty() {
        return format!("MIDI : aucun port ouvert ({} vus)", status.ports.available.len());
    }
    let names: Vec<&str> = status.ports.connected.iter().map(|p| p.name.as_str()).collect();
    format!("MIDI : {} ({} vus)", names.join(", "), status.ports.available.len())
}

/// Le dernier geste en clair.
fn input_text(input: Option<&LastInput>) -> String {
    match input {
        None => "—".to_owned(),
        Some(LastInput::Key(key)) => {
            let name = key.ch.map_or_else(
                || key.code.map_or_else(|| "?".to_owned(), |code| format!("{code:?}")),
                |c| c.to_uppercase().to_string(),
            );
            format!("clavier {name} ({})", if key.pressed { "appui" } else { "relâché" })
        }
        Some(LastInput::Midi(event)) => format!("MIDI port {} : {:?}", event.port.0, event.message),
    }
}

#[cfg(test)]
mod tests {
    use cf_control::{KeyCode, KeyEvent, Modifiers};

    use super::*;

    fn run(show: Option<&ShowState>, status: &ControlsStatus) {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            draw(ui, show, status, &AudioFeatures::default());
        });
        output.drop_without_applying_deltas();
    }

    #[test]
    fn le_panneau_se_dessine_sans_show_ni_midi() {
        run(None, &ControlsStatus::default());
    }

    #[test]
    fn les_textes_disent_l_etat() {
        let mut status = ControlsStatus::default();
        assert_eq!(ports_text(&status), "MIDI : désactivé");
        status.midi_running = true;
        assert!(ports_text(&status).contains("aucun port ouvert"));
        assert_eq!(input_text(None), "—");
        let key = KeyEvent::code_press(KeyCode::ArrowRight, Modifiers::default());
        assert_eq!(input_text(Some(&LastInput::Key(key))), "clavier ArrowRight (appui)");
        let letter = KeyEvent::char_press('g', Modifiers::default());
        assert_eq!(input_text(Some(&LastInput::Key(letter))), "clavier G (appui)");
        assert_eq!(bpm_text(None), "—");
        assert_eq!(bpm_text(Some(128.04)), "128.0 BPM");
    }
}
