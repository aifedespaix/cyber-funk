//! Le journal de session en direct : barre d'outils (niveaux, filtre, capacité, pause, vider, copies) et lignes.
//!
//! Les lignes viennent du tampon de `cf-debuglog` (une copie par image, prise par l'onglet). En pause, l'affichage lit
//! une copie figée (`frozen`) pendant que le tampon continue de se remplir. Le filtre n'est recalculé que si les lignes
//! ou le filtre ont changé : en régime établi, rien n'est alloué pour filtrer.

use std::collections::BTreeSet;

use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, RichText};
use cf_debuglog::{DEFAULT_CAPACITY, Level, LogBuffer, LogFilter, LogLine};

use super::copy;

/// Capacités proposées (lignes gardées par le tampon).
pub(crate) const CAPACITY_CHOICES: [usize; 3] = [100, 200, 500];
/// Nombre de lignes copiées proposé.
pub(crate) const COPY_CHOICES: [usize; 4] = [20, 50, 100, 200];
/// Nombre de lignes copiées par défaut.
const DEFAULT_COPY_N: usize = 50;
/// Durée d'affichage de la confirmation « N lignes copiées », en secondes (temps d'egui, pas l'horloge du show).
const TOAST_S: f64 = 2.0;
/// Boutons de niveau, dans l'ordre de la barre. Debug est éteint par défaut (bavard, utile seulement en enquête).
const LEVEL_BUTTONS: [Level; 4] = [Level::Debug, Level::Info, Level::Warn, Level::Error];
/// Couleur des noms d'événement (magenta de Sillage) : cliquables, ils posent le filtre.
const EVENT_COLOR: Color32 = Color32::from_rgb(0xff, 0x3d, 0xf0);
/// Couleur du niveau info (cyan de Sillage).
const INFO_COLOR: Color32 = Color32::from_rgb(0x00, 0xe6, 0xf7);

/// Ce qui a servi à calculer la liste visible : si rien n'a bougé, elle est reprise telle quelle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct VisibleKey {
    first_seq: u64,
    last_seq: u64,
    len: usize,
    levels: BTreeSet<Level>,
    text: String,
}

/// État de la vue du journal. Jamais sauvegardé.
#[derive(Resource, Debug, Clone)]
pub(crate) struct LogViewState {
    /// Niveaux affichés ; jamais vide (un ensemble vide voudrait dire « tous » pour [`LogFilter`]).
    pub levels: BTreeSet<Level>,
    /// Texte cherché dans le nom d'événement et le détail (sans tenir compte de la casse).
    pub text: String,
    /// L'affichage est figé.
    pub paused: bool,
    /// Lignes gardées par le tampon.
    pub capacity: usize,
    /// Lignes copiées par « Copier pour un agent » et « Warn + erreurs ».
    pub copy_n: usize,
    /// Copie affichée pendant la pause.
    pub frozen: Option<Vec<LogLine>>,
    /// Confirmation affichée et l'instant (temps d'egui, s) où elle s'efface.
    toast: Option<(String, f64)>,
    /// Rangs des lignes visibles dans la liste affichée (tampon réutilisé).
    visible: Vec<usize>,
    visible_key: Option<VisibleKey>,
}

impl Default for LogViewState {
    fn default() -> Self {
        Self {
            levels: BTreeSet::from([Level::Info, Level::Warn, Level::Error]),
            text: String::new(),
            paused: false,
            capacity: DEFAULT_CAPACITY,
            copy_n: DEFAULT_COPY_N,
            frozen: None,
            toast: None,
            visible: Vec::new(),
            visible_key: None,
        }
    }
}

impl LogViewState {
    /// Le filtre de l'affichage, au format de `cf-debuglog`.
    pub(crate) fn filter(&self) -> LogFilter {
        LogFilter {
            levels: self.levels.clone(),
            text: self.text.clone(),
        }
    }

    /// Allume ou éteint un niveau. Le dernier niveau allumé reste allumé.
    pub(crate) fn toggle_level(&mut self, level: Level) {
        if self.levels.contains(&level) {
            if self.levels.len() > 1 {
                self.levels.remove(&level);
            }
        } else {
            self.levels.insert(level);
        }
    }

    /// Pause : fige une copie du tampon. Reprendre : rend le direct.
    pub(crate) fn toggle_pause(&mut self, buffer: &LogBuffer) {
        self.paused = !self.paused;
        self.frozen = self.paused.then(|| buffer.snapshot());
    }

    /// Les lignes affichées : la copie figée en pause, le direct sinon. `draw` fait de même en empruntant la copie
    /// figée à part (la barre d'outils doit pouvoir modifier l'état pendant l'image).
    #[cfg(test)]
    pub(crate) fn displayed<'a>(&'a self, live: &'a [LogLine]) -> &'a [LogLine] {
        self.frozen.as_deref().unwrap_or(live)
    }

    /// Change la capacité du tampon ; s'il rétrécit, les plus anciennes lignes sont oubliées.
    pub(crate) fn set_capacity(&mut self, buffer: &LogBuffer, capacity: usize) {
        self.capacity = capacity;
        buffer.set_capacity(capacity);
    }

    /// Vide le tampon (et la copie figée, en pause).
    pub(crate) fn clear(&mut self, buffer: &LogBuffer) {
        buffer.clear();
        // Remplacée et non vidée : `draw` a pu emprunter l'ancienne copie pendant l'image (voir `draw`).
        self.frozen = self.paused.then(Vec::new);
    }

    /// Affiche `message` pendant [`TOAST_S`] à partir de `now_s` (temps d'egui).
    pub(crate) fn notify(&mut self, message: String, now_s: f64) {
        self.toast = Some((message, now_s + TOAST_S));
    }

    /// La confirmation encore affichée à `now_s`, s'il y en a une.
    pub(crate) fn toast(&self, now_s: f64) -> Option<&str> {
        self.toast
            .as_ref()
            .filter(|(_, until)| now_s < *until)
            .map(|(message, _)| message.as_str())
    }

    /// Recalcule les rangs visibles de `lines` si les lignes ou le filtre ont changé.
    fn refresh_visible(&mut self, lines: &[LogLine]) {
        let unchanged = self.visible_key.as_ref().is_some_and(|key| {
            key.len == lines.len()
                && key.first_seq == lines.first().map_or(0, |l| l.seq)
                && key.last_seq == lines.last().map_or(0, |l| l.seq)
                && key.levels == self.levels
                && key.text == self.text
        });
        if unchanged {
            return;
        }
        let filter = self.filter();
        self.visible.clear();
        self.visible.extend(
            lines
                .iter()
                .enumerate()
                .filter(|(_, l)| filter.matches(l))
                .map(|(i, _)| i),
        );
        self.visible_key = Some(VisibleKey {
            first_seq: lines.first().map_or(0, |l| l.seq),
            last_seq: lines.last().map_or(0, |l| l.seq),
            len: lines.len(),
            levels: filter.levels,
            text: filter.text,
        });
    }
}

/// Le journal : barre d'outils puis lignes, dans toute la place restante. `live` : copie du tampon de l'image.
/// Rend la hauteur de la zone des lignes.
pub(crate) fn draw(
    ui: &mut egui::Ui,
    state: &mut LogViewState,
    buffer: &LogBuffer,
    live: &[LogLine],
    session: &str,
) -> f32 {
    let now_s = ui.input(|i| i.time);
    // Le filtre est calculé sur les lignes affichées (figées en pause) ; la copie figée est prise à part pour que
    // `state` reste modifiable par la barre d'outils.
    let frozen = state.frozen.take();
    let lines = frozen.as_deref().unwrap_or(live);
    state.refresh_visible(lines);
    toolbar(ui, state, buffer, lines, session, now_s);
    let height = rows(ui, state, lines);
    // « Vider », « Pause » ou « Reprendre » ont pu remplacer la copie figée pendant la barre d'outils : on n'écrase
    // que l'absence laissée par le `take` ci-dessus.
    if state.paused && state.frozen.is_none() {
        state.frozen = frozen;
    }
    height
}

/// La zone du journal quand aucun tampon n'est branché (pas d'enregistreur de session).
pub(crate) fn draw_unavailable(ui: &mut egui::Ui) -> f32 {
    let size = ui.available_size().max(egui::vec2(0.0, ui.spacing().interact_size.y));
    ui.allocate_ui(size, |ui| {
        ui.weak("Journal indisponible : pas d'enregistreur de session (journal de session non ouvert).");
        ui.weak("« Marquer ce moment » et les copies reviennent avec le journal.");
    });
    size.y
}

fn level_color(ui: &egui::Ui, level: Level) -> Color32 {
    match level {
        Level::Debug => ui.visuals().weak_text_color(),
        Level::Info => INFO_COLOR,
        Level::Warn => ui.visuals().warn_fg_color,
        Level::Error => ui.visuals().error_fg_color,
    }
}

fn toolbar(
    ui: &mut egui::Ui,
    state: &mut LogViewState,
    buffer: &LogBuffer,
    lines: &[LogLine],
    session: &str,
    now_s: f64,
) {
    ui.horizontal_wrapped(|ui| {
        ui.strong("Journal de session en direct");
        ui.weak(format!("{} / {} lignes", state.visible.len(), lines.len()));
        ui.weak(session);
        if let Some(message) = state.toast(now_s) {
            ui.colored_label(INFO_COLOR, message);
        }
    });
    ui.horizontal_wrapped(|ui| {
        for level in LEVEL_BUTTONS {
            let on = state.levels.contains(&level);
            let text = RichText::new(level.label().to_lowercase()).color(level_color(ui, level));
            if ui
                .add(egui::Button::selectable(on, text))
                .on_hover_text("Afficher ou masquer ce niveau")
                .clicked()
            {
                state.toggle_level(level);
            }
        }
        ui.add(
            egui::TextEdit::singleline(&mut state.text)
                .hint_text("Filtrer (ev, texte)…")
                .desired_width(160.0),
        );
        if !state.text.is_empty() && ui.small_button("✕").on_hover_text("Effacer le filtre").clicked() {
            state.text.clear();
        }
        ui.label("Garder");
        let mut capacity = state.capacity;
        egui::ComboBox::from_id_salt("log_capacity")
            .selected_text(capacity.to_string())
            .width(56.0)
            .show_ui(ui, |ui| {
                for choice in CAPACITY_CHOICES {
                    ui.selectable_value(&mut capacity, choice, choice.to_string());
                }
            });
        ui.label("lignes");
        if capacity != state.capacity {
            state.set_capacity(buffer, capacity);
        }
        let pause = if state.paused { "Reprendre" } else { "Pause" };
        if ui.button(pause).clicked() {
            state.toggle_pause(buffer);
        }
        if ui.button("Vider").clicked() {
            state.clear(buffer);
        }
        ui.separator();
        ui.label("Copier les");
        egui::ComboBox::from_id_salt("log_copy_n")
            .selected_text(state.copy_n.to_string())
            .width(56.0)
            .show_ui(ui, |ui| {
                for choice in COPY_CHOICES {
                    ui.selectable_value(&mut state.copy_n, choice, choice.to_string());
                }
            });
        ui.label("dernières");
        let agent = ui
            .button(RichText::new("Copier pour un agent").color(INFO_COLOR))
            .on_hover_text("Les dernières lignes visibles (niveaux et filtre appliqués), avec un en-tête de session");
        let errors = ui
            .button("Warn + erreurs")
            .on_hover_text("Les dernières lignes warn et error, quels que soient les niveaux et le filtre affichés");
        let text = if agent.clicked() {
            Some(copy::copy_text(state, lines, &copy::header(session)))
        } else if errors.clicked() {
            Some(copy::copy_warn_errors_text(state, lines, &copy::header(session)))
        } else {
            None
        };
        if let Some(text) = text {
            let count = copy::copied_lines(&text);
            ui.ctx().copy_text(text);
            state.notify(format!("{count} lignes copiées"), now_s);
        }
    });
}

/// Les lignes visibles, collées en bas tant que l'opérateur n'a pas remonté. Rend la hauteur de la zone.
fn rows(ui: &mut egui::Ui, state: &mut LogViewState, lines: &[LogLine]) -> f32 {
    let row_h = ui.text_style_height(&egui::TextStyle::Monospace);
    let mut picked: Option<&str> = None;
    let visible = &state.visible;
    let output = egui::ScrollArea::vertical()
        .id_salt("log_rows")
        .auto_shrink(false)
        .stick_to_bottom(true)
        .show_rows(ui, row_h, visible.len(), |ui, range| {
            for &i in &visible[range] {
                let Some(line) = lines.get(i) else {
                    continue;
                };
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.label(RichText::new(line.seq.to_string()).monospace().weak());
                    ui.label(RichText::new(uptime_text(line.uptime_ms)).monospace().weak());
                    ui.label(
                        RichText::new(format!("{:<5}", line.level.label()))
                            .monospace()
                            .color(level_color(ui, line.level)),
                    );
                    let ev = ui
                        .add(
                            egui::Label::new(RichText::new(&line.ev).monospace().color(EVENT_COLOR))
                                .sense(egui::Sense::click()),
                        )
                        .on_hover_text("Filtrer sur cet événement");
                    if ev.clicked() {
                        picked = Some(&line.ev);
                    }
                    ui.label(RichText::new(&line.label).monospace());
                });
            }
        });
    if let Some(ev) = picked {
        ev.clone_into(&mut state.text);
    }
    output.inner_rect.height()
}

/// `+12.34s` : secondes depuis le lancement, au centième.
fn uptime_text(uptime_ms: u64) -> String {
    format!("+{:.2}s", uptime_ms as f64 / 1000.0)
}
