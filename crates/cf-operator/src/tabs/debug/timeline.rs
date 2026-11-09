//! Le bandeau « Timeline » de l'onglet Debug : la séquence que l'opérateur a jouée, de la seconde 0 à la fin du dernier
//! événement, avec une barre colorée et nommée par événement (une piste par colonne) et une tête de lecture.
//!
//! Le dessin ne lit que la timeline et n'écrit que des [`SandboxGesture`](super::sandbox::SandboxGesture) : rejouer,
//! effacer. « Copier le rapport » met le texte de la session dans le presse-papiers, pour le coller dans une demande à
//! un agent. Les calculs de position sont des fonctions pures, testées sans interface.

use bevy_egui::egui;
use cf_render::{DebugTimeline, EventCategory, EventJournal, JournalState, ShotSpan, TimelineSpan};

use super::Gesture;
use super::columns::{column_color, column_title};
use super::sandbox::SandboxGesture;
use super::shot_names::{shot_name, shot_title};

/// Hauteur du bandeau (px).
pub(crate) const TIMELINE_HEIGHT: f32 = 150.0;

/// Hauteur de fenêtre à partir de laquelle la timeline s'ajoute au journal.
pub(crate) const MIN_HEIGHT_FOR_TIMELINE: f32 = 600.0;

/// Hauteur d'une piste (px) et largeur de son titre (px).
const LANE_HEIGHT: f32 = 18.0;
const LANE_LABEL_WIDTH: f32 = 70.0;
/// Teinte de la piste « Caméra » (alternée d'un plan à l'autre pour séparer deux plans qui se suivent).
const CAMERA_COLOR: egui::Color32 = egui::Color32::from_rgb(0xb0, 0x8c, 0xff);

/// Durée minimale affichée (s) : une séquence de deux secondes ne remplit pas toute la largeur d'un coup.
const MIN_SPAN_S: f64 = 10.0;

/// Les pas de graduation possibles (s).
const TICK_STEPS_S: [f64; 8] = [1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 300.0];

/// Écart minimal entre deux graduations (px).
const MIN_TICK_PX: f32 = 46.0;

/// Durée affichée pour une séquence de `length_s` secondes.
pub(crate) fn shown_length_s(length_s: f64) -> f64 {
    length_s.max(MIN_SPAN_S)
}

/// Abscisse (px) de l'instant `t_s` sur une piste qui commence en `left` et mesure `width`.
pub(crate) fn x_of(t_s: f64, shown_s: f64, left: f32, width: f32) -> f32 {
    left + (t_s / shown_s).clamp(0.0, 1.0) as f32 * width
}

/// Le pas de graduation (s) qui laisse au moins [`MIN_TICK_PX`] entre deux traits.
pub(crate) fn tick_step_s(shown_s: f64, width: f32) -> f64 {
    let px_per_s = f64::from(width) / shown_s.max(1e-9);
    TICK_STEPS_S
        .into_iter()
        .find(|step| step * px_per_s >= f64::from(MIN_TICK_PX))
        .unwrap_or(TICK_STEPS_S[TICK_STEPS_S.len() - 1])
}

/// Le tronçon de la piste `lane` qui couvre l'instant `t_s` (le plus récent d'abord), `playhead_s` fermant ceux qui
/// tournent encore.
pub(crate) fn span_at(spans: &[TimelineSpan], lane: EventCategory, t_s: f64, playhead_s: f64) -> Option<&TimelineSpan> {
    spans
        .iter()
        .rev()
        .filter(|span| span.category == lane)
        .find(|span| t_s >= span.start_s && t_s <= span.end_s.unwrap_or(playhead_s).max(span.start_s))
}

/// Le plan de caméra qui couvre l'instant `t_s` (`playhead_s` ferme celui qui est encore à l'antenne).
pub(crate) fn shot_at(timeline: &DebugTimeline, t_s: f64, playhead_s: f64) -> Option<ShotSpan> {
    timeline
        .shot_spans()
        .find(|shot| t_s >= shot.start_s && t_s <= shot.end_s.unwrap_or(playhead_s).max(shot.start_s))
}

/// Le temps affiché en `m:ss.d`.
pub(crate) fn clock_text(seconds: f64) -> String {
    let whole = seconds.max(0.0);
    let minutes = (whole / 60.0).floor() as u32;
    format!("{minutes}:{:04.1}", whole - f64::from(minutes) * 60.0)
}

/// Le rapport complet d'une session : la timeline, puis ce que le monde a refusé (avec la raison). C'est ce qu'un agent
/// a besoin de lire pour comprendre « il ne s'est rien passé ».
pub(crate) fn full_report(timeline: &DebugTimeline, journal: Option<&EventJournal>, now_s: f64) -> String {
    let mut out = timeline.report(now_s);
    let refused: Vec<_> = journal
        .into_iter()
        .flat_map(EventJournal::iter)
        .filter_map(|entry| match entry.state {
            JournalState::Refused(reason) => Some(format!(
                "  {:7.1} s  {} — refusé : {}\n",
                entry.at_s,
                entry.event_id,
                reason.text()
            )),
            _ => None,
        })
        .collect();
    if !refused.is_empty() {
        out.push_str("Demandes refusées :\n");
        out.extend(refused);
    }
    out
}

/// Ce que le bandeau affiche.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TimelineView<'a> {
    /// La timeline (`None` : absente, on le dit).
    pub timeline: Option<&'a DebugTimeline>,
    /// Temps de simulation (s).
    pub now_s: f64,
    /// Un rejeu est en cours.
    pub replaying: bool,
}

/// Le bandeau. `report` n'est appelé que sur le clic de « Copier le rapport ».
pub(crate) fn draw(
    ui: &mut egui::Ui,
    view: TimelineView<'_>,
    gestures: &mut Vec<Gesture>,
    report: impl FnOnce(&DebugTimeline) -> String,
) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let Some(timeline) = view.timeline else {
            ui.strong("Timeline");
            ui.weak("Timeline absente (monde absent).");
            return;
        };
        let length = timeline.length_s(view.now_s);
        let playhead = timeline.playhead_s(view.now_s);
        ui.horizontal_wrapped(|ui| {
            ui.strong("Timeline");
            ui.monospace(format!("▶ {} / {}", clock_text(playhead), clock_text(length)));
            match timeline.current_shot() {
                Some(id) => {
                    ui.label(egui::RichText::new(format!("📷 {}", shot_title(id))).color(CAMERA_COLOR))
                        .on_hover_text(format!("Plan à l'antenne : {id}"));
                }
                None => {
                    ui.weak("📷 aucun plan");
                }
            }
            if view.replaying {
                ui.colored_label(egui::Color32::from_rgb(230, 190, 90), "rejeu en cours");
            }
            let can_replay = !timeline.requests().is_empty() && !view.replaying;
            if ui
                .add_enabled(can_replay, egui::Button::new("↻ Rejouer"))
                .on_hover_text("Nettoie, puis relance les mêmes demandes aux mêmes instants.")
                .clicked()
            {
                gestures.push(Gesture::Sandbox(SandboxGesture::Replay));
            }
            if ui
                .add_enabled(!timeline.is_empty(), egui::Button::new("🗑 Effacer"))
                .on_hover_text("Remet la timeline à zéro (les événements en cours continuent).")
                .clicked()
            {
                gestures.push(Gesture::Sandbox(SandboxGesture::ClearTimeline));
            }
            if ui
                .add_enabled(!timeline.is_empty(), egui::Button::new("📋 Copier le rapport"))
                .on_hover_text(
                    "Copie ce qui a tourné, quand et combien de temps : à coller dans une demande à un agent.",
                )
                .clicked()
            {
                ui.ctx().copy_text(report(timeline));
            }
        });
        draw_tracks(ui, timeline, length, playhead);
    });
}

/// Les quatre pistes, la graduation et la tête de lecture.
fn draw_tracks(ui: &mut egui::Ui, timeline: &DebugTimeline, length: f64, playhead: f64) {
    let shown = shown_length_s(length.max(playhead));
    let lanes = EventCategory::ALL;
    let rows = lanes.len() + 1;
    let height = LANE_HEIGHT * rows as f32 + 14.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let left = rect.left() + LANE_LABEL_WIDTH;
    let width = (rect.right() - left - 4.0).max(10.0);
    let text_color = ui.visuals().text_color();
    let faint = ui.visuals().widgets.noninteractive.bg_stroke.color;

    for (i, lane) in lanes.into_iter().enumerate() {
        let top = rect.top() + LANE_HEIGHT * i as f32;
        let lane_rect = egui::Rect::from_min_size(egui::pos2(left, top), egui::vec2(width, LANE_HEIGHT - 2.0));
        painter.rect_filled(lane_rect, 2.0, faint.gamma_multiply(0.25));
        painter.text(
            egui::pos2(rect.left(), top + LANE_HEIGHT / 2.0 - 1.0),
            egui::Align2::LEFT_CENTER,
            column_title(lane),
            egui::FontId::proportional(11.0),
            text_color,
        );
        for span in timeline.spans().iter().filter(|span| span.category == lane) {
            let end = span.end_s.unwrap_or(playhead).max(span.start_s);
            let (x0, x1) = (x_of(span.start_s, shown, left, width), x_of(end, shown, left, width));
            let bar = egui::Rect::from_min_max(
                egui::pos2(x0, top + 1.0),
                egui::pos2(x1.max(x0 + 3.0), top + LANE_HEIGHT - 3.0),
            );
            // Un événement qui tourne encore est plus clair : sa fin n'est pas connue.
            let fill = if span.end_s.is_some() {
                column_color(lane)
            } else {
                column_color(lane).gamma_multiply(0.6)
            };
            painter.rect_filled(bar, 3.0, fill);
            painter.with_clip_rect(bar).text(
                bar.left_center() + egui::vec2(3.0, 0.0),
                egui::Align2::LEFT_CENTER,
                &span.label,
                egui::FontId::proportional(11.0),
                egui::Color32::BLACK,
            );
        }
    }

    // Piste « Caméra » : un bloc par plan, du plus ancien au plus récent.
    let camera_top = rect.top() + LANE_HEIGHT * lanes.len() as f32;
    let camera_rect = egui::Rect::from_min_size(egui::pos2(left, camera_top), egui::vec2(width, LANE_HEIGHT - 2.0));
    painter.rect_filled(camera_rect, 2.0, faint.gamma_multiply(0.25));
    painter.text(
        egui::pos2(rect.left(), camera_top + LANE_HEIGHT / 2.0 - 1.0),
        egui::Align2::LEFT_CENTER,
        "Caméra",
        egui::FontId::proportional(11.0),
        text_color,
    );
    for (i, shot) in timeline.shot_spans().enumerate() {
        let end = shot.end_s.unwrap_or(playhead).max(shot.start_s);
        let (x0, x1) = (x_of(shot.start_s, shown, left, width), x_of(end, shown, left, width));
        let bar = egui::Rect::from_min_max(
            egui::pos2(x0, camera_top + 1.0),
            egui::pos2(x1.max(x0 + 3.0), camera_top + LANE_HEIGHT - 3.0),
        );
        let tint = if i % 2 == 0 { 1.0 } else { 0.7 };
        painter.rect_filled(bar, 3.0, CAMERA_COLOR.gamma_multiply(tint));
        painter.with_clip_rect(bar).text(
            bar.left_center() + egui::vec2(3.0, 0.0),
            egui::Align2::LEFT_CENTER,
            shot_name(shot.id),
            egui::FontId::proportional(11.0),
            egui::Color32::BLACK,
        );
    }

    // Graduation sous les pistes.
    let ruler_y = rect.top() + LANE_HEIGHT * rows as f32 + 2.0;
    let step = tick_step_s(shown, width);
    let mut t = 0.0;
    while t <= shown + 1e-9 {
        let x = x_of(t, shown, left, width);
        painter.line_segment(
            [egui::pos2(x, ruler_y), egui::pos2(x, ruler_y + 4.0)],
            egui::Stroke::new(1.0, faint),
        );
        painter.text(
            egui::pos2(x + 2.0, ruler_y + 4.0),
            egui::Align2::LEFT_TOP,
            format!("{t:.0}s"),
            egui::FontId::monospace(9.0),
            text_color.gamma_multiply(0.7),
        );
        t += step;
    }

    // La tête de lecture.
    let x = x_of(playhead, shown, left, width);
    painter.line_segment(
        [egui::pos2(x, rect.top()), egui::pos2(x, ruler_y + 2.0)],
        egui::Stroke::new(1.5, egui::Color32::WHITE),
    );

    // Survol : le nom, le début et la fin du tronçon sous le pointeur.
    if let Some(pointer) = response.hover_pos() {
        let lane = ((pointer.y - rect.top()) / LANE_HEIGHT) as usize;
        let t = f64::from(pointer.x - left) / f64::from(width) * shown;
        if lane == lanes.len() {
            if let Some(shot) = shot_at(timeline, t, playhead) {
                let end = shot.end_s.map_or_else(|| "à l'antenne".to_owned(), clock_text);
                response.on_hover_ui_at_pointer(|ui| {
                    ui.strong(shot_title(shot.id));
                    ui.label(format!("{} → {end}", clock_text(shot.start_s)));
                    ui.weak(shot.id);
                });
            }
        } else if let Some(lane) = lanes.get(lane)
            && let Some(span) = span_at(timeline.spans(), *lane, t, playhead)
        {
            let end = span.end_s.map_or_else(|| "en cours".to_owned(), clock_text);
            response.on_hover_ui_at_pointer(|ui| {
                ui.strong(&span.label);
                ui.label(format!("{} → {end}", clock_text(span.start_s)));
                ui.weak(format!("{} · {:?}", span.event_id, span.origin));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_render::EventOrigin;

    fn span(category: EventCategory, start_s: f64, end_s: Option<f64>) -> TimelineSpan {
        TimelineSpan {
            event_id: "x",
            label: "X".to_owned(),
            category,
            origin: EventOrigin::Manual,
            start_s,
            end_s,
        }
    }

    #[test]
    fn a_short_session_does_not_fill_the_whole_width() {
        assert!((shown_length_s(2.0) - MIN_SPAN_S).abs() < 1e-9);
        assert!((shown_length_s(45.0) - 45.0).abs() < 1e-9);
    }

    #[test]
    fn positions_are_proportional_and_clamped() {
        assert!((x_of(0.0, 20.0, 100.0, 400.0) - 100.0).abs() < 1e-3);
        assert!((x_of(10.0, 20.0, 100.0, 400.0) - 300.0).abs() < 1e-3);
        assert!(
            (x_of(99.0, 20.0, 100.0, 400.0) - 500.0).abs() < 1e-3,
            "jamais au-delà de la piste"
        );
        assert!((x_of(-5.0, 20.0, 100.0, 400.0) - 100.0).abs() < 1e-3);
    }

    #[test]
    fn ticks_get_sparser_as_the_session_gets_longer() {
        assert!(tick_step_s(10.0, 460.0) <= 2.0);
        assert!(tick_step_s(300.0, 460.0) >= 30.0);
        assert!(
            tick_step_s(60.0, 40.0) >= 60.0,
            "piste très étroite : pas le plus large"
        );
    }

    #[test]
    fn the_span_under_the_pointer_is_found_in_its_own_lane_only() {
        let spans = [
            span(EventCategory::Car, 1.0, Some(4.0)),
            span(EventCategory::Building, 2.0, Some(8.0)),
            span(EventCategory::Car, 6.0, None),
        ];
        let car = |t| span_at(&spans, EventCategory::Car, t, 9.0).map(|s| s.start_s);
        assert_eq!(car(2.0), Some(1.0));
        assert_eq!(car(5.0), None, "entre deux événements");
        assert_eq!(car(8.0), Some(6.0), "celui qui tourne court jusqu'à la tête de lecture");
        assert_eq!(car(9.5), None, "au-delà de la tête de lecture");
        assert!(span_at(&spans, EventCategory::Sign, 3.0, 9.0).is_none());
    }

    #[test]
    fn the_full_report_lists_refusals_with_their_reason() {
        use cf_render::RefusalReason;
        let mut timeline = DebugTimeline::default();
        timeline.request(1.0, "tower");
        let mut journal = EventJournal::default();
        journal.request(1.0, "tower");
        journal.refused(1.0, "tower", RefusalReason::StructureBusy);
        let report = full_report(&timeline, Some(&journal), 5.0);
        assert!(report.contains("Demandes refusées"), "{report}");
        assert!(
            report.contains("tower — refusé : une structure occupe déjà la route"),
            "{report}"
        );
        assert!(!full_report(&timeline, None, 5.0).contains("refusées"));
    }

    #[test]
    fn the_shot_under_the_pointer_is_found_on_the_camera_lane() {
        use cf_render::ActiveEvents;
        let mut timeline = DebugTimeline::default();
        timeline.track_shot(0.0, Some("chase"));
        timeline.track_shot(5.0, Some("cockpit"));
        let mut active = ActiveEvents::default();
        active.begin(cf_render::ActiveEvent {
            id: cf_render::ActiveId::UNSET,
            event_id: "x",
            label: "X".to_owned(),
            category: EventCategory::Car,
            origin: EventOrigin::Manual,
            started_s: 2.0,
            duration_s: None,
        });
        timeline.track(2.0, &active);
        // Le zéro de la timeline est à 2 s de simulation : « chase » a fini à 3 s, « cockpit » est à l'antenne.
        assert_eq!(shot_at(&timeline, 1.0, 9.0).map(|s| s.id), Some("chase"));
        assert_eq!(shot_at(&timeline, 4.0, 9.0).map(|s| s.id), Some("cockpit"));
        assert_eq!(shot_at(&timeline, 12.0, 9.0), None, "au-delà de la tête de lecture");
    }

    #[test]
    fn the_clock_reads_minutes_seconds_and_tenths() {
        assert_eq!(clock_text(0.0), "0:00.0");
        assert_eq!(clock_text(75.34), "1:15.3");
        assert_eq!(clock_text(-3.0), "0:00.0");
    }
}
