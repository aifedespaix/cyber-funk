//! Colonne de droite de la vue Live : tempo et source, vumètres, section, drop préparé, santé (i/s, GPU, MIDI).
//!
//! L'aperçu de la sortie (J8-05, `crate::preview`) se place au-dessus.

use bevy_egui::egui::{self, RichText};
use cf_control::Action;
use cf_render::audio::{AudioFeatures, Section, TempoSource};
use cf_render::{ControlsStatus, DiagnosticsSnapshot};
use cf_show::drop::DropPlan;

use super::commands::{CommandContext, LiveCommand};
use super::view::LiveData;
use super::widgets::{WARN_TEXT, action_button, level_bar};

/// En dessous de ce nombre d'images par seconde la santé est signalée.
const LOW_FPS: f32 = 50.0;

pub(crate) fn draw(ui: &mut egui::Ui, data: &LiveData<'_>, ctx: &CommandContext<'_>, orders: &mut Vec<Action>) {
    card(ui, crate::preview::card);
    card(ui, |ui| tempo_card(ui, data, ctx, orders));
    card(ui, |ui| meters_card(ui, data.features));
    card(ui, |ui| drop_card(ui, data.drop_plan, data.state.drop_now));
    card(ui, |ui| health_card(ui, data.diagnostics, data.controls, data.features));
}

fn card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.group(|ui| {
        ui.set_min_width(ui.available_width());
        add_contents(ui);
    });
}

fn tempo_card(ui: &mut egui::Ui, data: &LiveData<'_>, ctx: &CommandContext<'_>, orders: &mut Vec<Action>) {
    ui.strong("TEMPO");
    let Some(features) = data.features else {
        ui.label("Audio non branché.");
        return;
    };
    ui.label(
        RichText::new(format!(
            "{} BPM · {}",
            features.bpm.map_or_else(|| "—".to_owned(), |bpm| format!("{bpm:.1}")),
            source_text(features.tempo_source)
        ))
        .heading(),
    );
    level_bar(ui, "confiance", features.confidence);
    ui.label(format!(
        "mesure {} · phrase {} · section : {}",
        features.bar,
        features.phrase,
        section_text(features.section)
    ));
    if let Some(controls) = data.controls {
        let clock = controls.clock_bpm.map_or_else(
            || "pas d'horloge MIDI".to_owned(),
            |bpm| format!("horloge MIDI {bpm:.1}"),
        );
        ui.label(clock);
    }
    ui.horizontal_wrapped(|ui| {
        let size = egui::vec2(0.0, 40.0);
        for command in [
            LiveCommand::TapTempo,
            LiveCommand::Resync,
            LiveCommand::NudgeBack,
            LiveCommand::NudgeForward,
        ] {
            action_button(ui, command, ctx, orders, size, false);
        }
    });
}

fn meters_card(ui: &mut egui::Ui, features: Option<&AudioFeatures>) {
    ui.strong("VUMÈTRES");
    let Some(f) = features else {
        ui.label("—");
        return;
    };
    level_bar(ui, "basses", f.bass);
    level_bar(ui, "médiums", f.mid);
    level_bar(ui, "aigus", f.high);
    level_bar(ui, "volume", f.loud);
    if !f.live {
        ui.colored_label(WARN_TEXT, "Aucun signal audio : le monde ralentit.");
    }
}

fn drop_card(ui: &mut egui::Ui, plan: Option<&DropPlan>, striking: bool) {
    ui.strong("DROP PRÉPARÉ");
    match plan {
        Some(plan) => ui.label(drop_text(plan)),
        None => ui.label("aucun (rien n'est préparé hors des montées)"),
    };
    if striking {
        ui.label(RichText::new("le drop frappe").strong().color(WARN_TEXT));
    }
}

fn health_card(
    ui: &mut egui::Ui,
    diagnostics: Option<&DiagnosticsSnapshot>,
    controls: Option<&ControlsStatus>,
    features: Option<&AudioFeatures>,
) {
    ui.strong("SANTÉ");
    match diagnostics {
        Some(d) => {
            let line = health_text(d);
            if d.fps > 0.0 && d.fps < LOW_FPS {
                ui.colored_label(WARN_TEXT, line);
            } else {
                ui.label(line);
            }
        }
        None => {
            ui.label("Diagnostic non branché.");
        }
    }
    if let Some(controls) = controls {
        ui.label(midi_text(controls));
        for note in &controls.notes {
            ui.colored_label(WARN_TEXT, note);
        }
    }
    if features.is_some_and(|f| !f.live) {
        ui.colored_label(WARN_TEXT, "Audio : pas de signal");
    }
}

/// `60 i/s · GPU 9,8 ms · 1 412 objets vus`.
pub(crate) fn health_text(d: &DiagnosticsSnapshot) -> String {
    let gpu = d
        .gpu_total_ms
        .map_or_else(|| "GPU n/d".to_owned(), |ms| format!("GPU {ms:.1} ms"));
    format!(
        "{:.0} i/s · {gpu} · {} objets vus · p99 {:.1} ms",
        d.fps, d.visible_entities, d.frame_p99_ms
    )
}

/// `MIDI : X1 mk3, DDJ-FLX4` ou la raison de l'absence.
pub(crate) fn midi_text(status: &ControlsStatus) -> String {
    if !status.midi_running {
        return "MIDI : désactivé (le clavier reste actif)".to_owned();
    }
    if status.ports.connected.is_empty() {
        return "MIDI : aucun contrôleur ouvert".to_owned();
    }
    let names: Vec<&str> = status.ports.connected.iter().map(|p| p.name.as_str()).collect();
    format!("MIDI : {}", names.join(", "))
}

/// `wheel + slalom` : la cascade, le plan, le titre du drop préparé.
pub(crate) fn drop_text(plan: &DropPlan) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(stunt) = &plan.stunt {
        parts.push(stunt.to_string());
    }
    if let Some(shot) = &plan.shot {
        parts.push(shot.to_string());
    }
    if let Some(title) = &plan.title {
        parts.push(format!("« {title} »"));
    }
    let what = if parts.is_empty() {
        "effets seuls".to_owned()
    } else {
        parts.join(" + ")
    };
    if plan.forced { format!("{what} (forcé)") } else { what }
}

fn source_text(source: TempoSource) -> &'static str {
    match source {
        TempoSource::Clock => "horloge MIDI",
        TempoSource::Link => "Link",
        TempoSource::File => "fichier",
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
