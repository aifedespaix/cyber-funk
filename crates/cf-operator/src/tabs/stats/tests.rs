//! Tests de l'onglet Statistiques sans écran : copie du journal (filtres, coupe aux N dernières, variante warn +
//! erreurs), repère, pause, capacité, et dessin de l'onglet à deux tailles, avec et sans journal.

use std::collections::BTreeSet;

use bevy_egui::egui;
use cf_debuglog::{Level, LogBuffer, LogLine, SessionHeader};

use super::copy::{copy_text, copy_warn_errors_text};
use super::log_view::LogViewState;
use super::{StatsView, draw};
use crate::services::logbuf::mark_moment;

fn line(seq: u64, level: Level, ev: &str, label: &str) -> LogLine {
    LogLine {
        seq,
        uptime_ms: seq * 1000,
        frame: seq * 10,
        level,
        ev: ev.into(),
        label: label.into(),
    }
}

fn header() -> SessionHeader {
    SessionHeader {
        session: "session-X".into(),
        copied_at: "2026-11-07T10:00:00Z".into(),
        context: Vec::new(),
    }
}

/// Les lignes du journal dans la copie (après le séparateur `---`).
fn body(text: &str) -> Vec<&str> {
    text.split_once("---\n")
        .map(|(_, body)| body.lines().collect())
        .unwrap_or_default()
}

fn sample() -> Vec<LogLine> {
    vec![
        line(1, Level::Info, "cue.called", "KRMN · Démesure"),
        line(2, Level::Warn, "perf.slow_frame", "120 ms"),
        line(3, Level::Info, "cue.active", "KRMN · Démesure"),
        line(4, Level::Error, "log", "Path not found"),
        line(5, Level::Info, "cue.called", "KRMN · Envol"),
        line(6, Level::Info, "cue.called", "KRMN · Final"),
    ]
}

#[test]
fn copy_text_applies_levels_and_filter_then_takes_last_n() {
    let mut state = LogViewState::default();
    state.levels = BTreeSet::from([Level::Info]);
    state.text = "called".into();
    state.copy_n = 2;
    let text = copy_text(&state, &sample(), &header());
    let lines = body(&text);
    assert_eq!(lines.len(), 2, "{text}");
    // Filtre d'abord (1, 5, 6), puis les 2 dernières, dans l'ordre du journal.
    assert!(lines[0].starts_with("#5 "), "{text}");
    assert!(lines[1].starts_with("#6 "), "{text}");
    assert!(text.starts_with("session: session-X\n"), "{text}");
    assert!(text.contains("niveaux: INFO\n"), "{text}");
    assert!(text.contains("filtre: \"called\"\n"), "{text}");
}

#[test]
fn copy_text_on_empty_filter_result_has_header_only() {
    let mut state = LogViewState::default();
    state.text = "absent de tout".into();
    let text = copy_text(&state, &sample(), &header());
    assert!(text.starts_with("session: session-X\n"), "{text}");
    assert!(text.contains("lignes: 0 sur 6\n"), "{text}");
    assert!(text.ends_with("---\n"), "rien après le séparateur : {text}");
    assert_eq!(body(&text), Vec::<&str>::new());
}

#[test]
fn warn_error_variant_ignores_info_even_if_visible() {
    let mut state = LogViewState::default();
    // Seul INFO est affiché et un filtre texte est posé : la variante n'en tient aucun compte.
    state.levels = BTreeSet::from([Level::Info]);
    state.text = "cue".into();
    state.copy_n = 50;
    let text = copy_warn_errors_text(&state, &sample(), &header());
    let lines = body(&text);
    assert_eq!(lines.len(), 2, "{text}");
    assert!(lines[0].starts_with("#2 ") && lines[0].contains("WARN"), "{text}");
    assert!(lines[1].starts_with("#4 ") && lines[1].contains("ERROR"), "{text}");
    assert!(text.contains("niveaux: WARN, ERROR\n"), "{text}");
    assert!(text.contains("filtre: aucun\n"), "{text}");
}

#[test]
fn mark_moment_appears_in_snapshot_with_mark_event() {
    let buffer = LogBuffer::new(10);
    buffer.push(line(41, Level::Warn, "perf.slow_frame", "120 ms"));
    mark_moment(&buffer, "repère opérateur");
    let lines = buffer.snapshot();
    let mark = lines.last().expect("le repère manque");
    assert_eq!(mark.ev, "mark");
    assert_eq!(mark.level, Level::Info);
    assert_eq!(mark.label, "repère opérateur");
    assert_eq!(mark.seq, 42, "suit la dernière ligne connue");
    // Sur un tampon vide aussi.
    let empty = LogBuffer::new(4);
    mark_moment(&empty, "x");
    assert_eq!(empty.snapshot().len(), 1);
}

#[test]
fn pause_freezes_the_displayed_lines() {
    let buffer = LogBuffer::new(10);
    buffer.push(line(1, Level::Info, "a", ""));
    buffer.push(line(2, Level::Info, "b", ""));
    let mut state = LogViewState::default();
    state.toggle_pause(&buffer);
    assert!(state.paused);
    buffer.push(line(3, Level::Info, "c", ""));
    let live = buffer.snapshot();
    assert_eq!(live.len(), 3, "le tampon continue de se remplir");
    let shown: Vec<u64> = state.displayed(&live).iter().map(|l| l.seq).collect();
    assert_eq!(shown, [1, 2], "l'affichage est figé");
    state.toggle_pause(&buffer);
    assert!(!state.paused && state.frozen.is_none());
    assert_eq!(state.displayed(&live).len(), 3, "reprendre rend le direct");
}

#[test]
fn lowering_capacity_drops_oldest() {
    let buffer = LogBuffer::new(500);
    for seq in 1..=300 {
        buffer.push(line(seq, Level::Info, "e", ""));
    }
    let mut state = LogViewState::default();
    state.set_capacity(&buffer, 100);
    assert_eq!(state.capacity, 100);
    let lines = buffer.snapshot();
    assert_eq!(lines.len(), 100);
    assert_eq!(lines.first().map(|l| l.seq), Some(201));
    assert_eq!(lines.last().map(|l| l.seq), Some(300));
}

#[test]
fn the_last_level_cannot_be_switched_off() {
    let mut state = LogViewState::default();
    for level in [Level::Info, Level::Warn, Level::Error, Level::Debug] {
        state.toggle_level(level);
    }
    assert!(
        !state.levels.is_empty(),
        "un ensemble vide voudrait dire « tout » pour le filtre"
    );
}

// --- Dessin (sans fenêtre) ---

/// Dessine l'onglet dans une fenêtre de `size` pendant deux images ; rend la hauteur du journal à la dernière.
fn draw_at(size: egui::Vec2, buffer: Option<&LogBuffer>) -> f32 {
    let ctx = egui::Context::default();
    let mut state = LogViewState::default();
    let mut height = 0.0;
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..egui::RawInput::default()
        };
        ctx.run_ui(raw, |ui| {
            let mut view = StatsView::empty(&mut state);
            view.log = buffer;
            height = draw(ui, &mut view);
        })
        .drop_without_applying_deltas();
    }
    height
}

#[test]
fn the_tab_draws_with_and_without_a_log_at_two_sizes() {
    let buffer = LogBuffer::new(500);
    for l in sample() {
        buffer.push(l);
    }
    for size in [egui::vec2(640.0, 400.0), egui::vec2(1280.0, 720.0)] {
        let with = draw_at(size, Some(&buffer));
        assert!(with > 0.0, "journal sans hauteur à {size:?}");
        let without = draw_at(size, None);
        assert!(without > 0.0, "zone « Journal indisponible » sans hauteur à {size:?}");
    }
}
