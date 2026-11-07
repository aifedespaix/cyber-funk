//! Les quatre cartes du milieu : énergie audio sur 60 s, bandes du spectre, événements par famille, temps d'antenne
//! par DJ. Tracées directement au `Painter` d'egui (pas de `egui_plot` dans le workspace). Sans données, la carte le
//! dit (« Pas de données ») au lieu d'afficher des chiffres inventés.

use bevy_egui::egui::{self, Color32, Pos2, Rect, Stroke, StrokeKind, vec2};
use cf_render::audio::AudioFeatures;
use cf_render::{EventCategory, ShowSheet};

use super::history::{Airtime, ENERGY_SAMPLES, EnergyHistory, EventTally};

/// Magenta de Sillage (courbe d'énergie, barres).
const MAGENTA: Color32 = Color32::from_rgb(0xff, 0x3d, 0xf0);
/// Cyan de Sillage (bandes, barres).
const CYAN: Color32 = Color32::from_rgb(0x00, 0xe6, 0xf7);
/// Fond des zones tracées.
const PLOT_BG: Color32 = Color32::from_rgb(0x0a, 0x0a, 0x14);
/// Largeur des libellés des barres horizontales (points egui).
const BAR_LABEL_W: f32 = 84.0;
/// Hauteur d'une barre horizontale.
const BAR_H: f32 = 8.0;
/// Écrit quand une carte n'a rien à montrer.
const NO_DATA: &str = "Pas de données";

/// Ce que les cartes affichent.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ChartData<'a> {
    pub energy: Option<&'a EnergyHistory>,
    pub features: Option<&'a AudioFeatures>,
    pub tally: Option<&'a EventTally>,
    pub airtime: Option<(&'a Airtime, &'a ShowSheet)>,
}

/// Les quatre cartes, en colonnes égales, de `height` points de haut.
pub(crate) fn draw(ui: &mut egui::Ui, data: &ChartData<'_>, height: f32) {
    ui.columns(4, |cols| {
        card(&mut cols[0], "Énergie audio (60 s)", height, |ui| {
            energy(ui, data.energy);
        });
        card(&mut cols[1], "Spectre (3 bandes)", height, |ui| {
            bands(ui, data.features);
        });
        card(&mut cols[2], "Événements par type", height, |ui| {
            events(ui, data.tally);
        });
        card(&mut cols[3], "Temps d'antenne par DJ", height, |ui| {
            airtime(ui, data.airtime);
        });
    });
}

fn card(ui: &mut egui::Ui, title: &str, height: f32, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        let inner = (height - 2.0 * ui.style().spacing.window_margin.topf()).max(0.0);
        ui.set_width(ui.available_width());
        ui.set_height(inner);
        ui.strong(title);
        add(ui);
    });
}

/// Zone de tracé : toute la place restante de la carte.
fn plot_area(ui: &mut egui::Ui) -> (Rect, egui::Painter) {
    let size = ui.available_size().max(vec2(1.0, 1.0));
    let (response, painter) = ui.allocate_painter(size, egui::Sense::hover());
    painter.rect_filled(response.rect, 3.0, PLOT_BG);
    (response.rect, painter)
}

fn energy(ui: &mut egui::Ui, history: Option<&EnergyHistory>) {
    let Some(history) = history.filter(|h| h.len() > 1) else {
        ui.weak(NO_DATA);
        return;
    };
    let (rect, painter) = plot_area(ui);
    // L'axe des x couvre toujours 60 s : une courbe qui démarre remplit la droite, comme un défilement.
    let step = rect.width() / (ENERGY_SAMPLES - 1) as f32;
    let first = ENERGY_SAMPLES - history.len();
    let points: Vec<Pos2> = history
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = rect.left() + (first + i) as f32 * step;
            let y = rect.bottom() - v.clamp(0.0, 1.0) * rect.height() * 0.9;
            Pos2::new(x, y)
        })
        .collect();
    painter.line(points, Stroke::new(2.0, MAGENTA));
}

fn bands(ui: &mut egui::Ui, features: Option<&AudioFeatures>) {
    let Some(f) = features else {
        ui.weak(NO_DATA);
        return;
    };
    let (rect, painter) = plot_area(ui);
    let values = [("basses", f.bass), ("médiums", f.mid), ("aigus", f.high)];
    let w = rect.width() / values.len() as f32;
    let font = egui::TextStyle::Small.resolve(ui.style());
    let label_h = ui.text_style_height(&egui::TextStyle::Small);
    for (i, (name, value)) in values.into_iter().enumerate() {
        let left = rect.left() + i as f32 * w;
        let top = rect.bottom() - label_h - value.clamp(0.0, 1.0) * (rect.height() - label_h);
        let bar = Rect::from_min_max(
            Pos2::new(left + 4.0, top),
            Pos2::new(left + w - 4.0, rect.bottom() - label_h),
        );
        painter.rect_filled(bar, 2.0, CYAN);
        painter.text(
            Pos2::new(left + w / 2.0, rect.bottom()),
            egui::Align2::CENTER_BOTTOM,
            name,
            font.clone(),
            ui.visuals().weak_text_color(),
        );
    }
}

/// Une barre horizontale : libellé, jauge (part du maximum), valeur.
fn bar(ui: &mut egui::Ui, label: &str, fraction: f32, value: &str, color: Color32) {
    ui.horizontal(|ui| {
        ui.add_sized([BAR_LABEL_W, BAR_H * 2.0], egui::Label::new(label).truncate());
        let width = (ui.available_width() - 48.0).max(8.0);
        let (rect, _) = ui.allocate_exact_size(vec2(width, BAR_H), egui::Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, BAR_H / 2.0, PLOT_BG);
        let filled = Rect::from_min_size(rect.min, vec2(rect.width() * fraction.clamp(0.0, 1.0), BAR_H));
        painter.rect_filled(filled, BAR_H / 2.0, color);
        painter.rect_stroke(rect, BAR_H / 2.0, Stroke::new(1.0, PLOT_BG), StrokeKind::Inside);
        ui.label(value);
    });
}

fn category_label(category: EventCategory) -> &'static str {
    match category {
        EventCategory::Sign => "Enseignes",
        EventCategory::Building => "Immeubles",
        EventCategory::Car => "Voitures",
        EventCategory::District => "Quartiers",
    }
}

fn events(ui: &mut egui::Ui, tally: Option<&EventTally>) {
    let Some(tally) = tally.filter(|t| t.total() > 0) else {
        ui.weak(NO_DATA);
        return;
    };
    let max = EventCategory::ALL
        .iter()
        .map(|c| tally.count(*c))
        .max()
        .unwrap_or(1)
        .max(1);
    for category in EventCategory::ALL {
        let n = tally.count(category);
        bar(
            ui,
            category_label(category),
            n as f32 / max as f32,
            &n.to_string(),
            MAGENTA,
        );
    }
}

fn airtime(ui: &mut egui::Ui, data: Option<(&Airtime, &ShowSheet)>) {
    let Some((airtime, sheet)) = data.filter(|(a, _)| a.secs.iter().any(|s| *s > 0.0)) else {
        ui.weak(NO_DATA);
        return;
    };
    let max = airtime.secs.iter().copied().fold(0.0_f64, f64::max).max(1.0);
    egui::ScrollArea::vertical().id_salt("airtime_bars").show(ui, |ui| {
        for (slot, secs) in sheet.slots.iter().zip(&airtime.secs) {
            let name = if slot.dj.is_empty() {
                "(sans nom)"
            } else {
                slot.dj.as_str()
            };
            bar(ui, name, (secs / max) as f32, &minutes_text(*secs), CYAN);
        }
    });
}

/// `12′` sous l'heure, `1 h 05′` au-delà.
fn minutes_text(secs: f64) -> String {
    let minutes = (secs / 60.0).floor() as u64;
    if minutes < 60 {
        format!("{minutes}′")
    } else {
        format!("{} h {:02}′", minutes / 60, minutes % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::minutes_text;

    #[test]
    fn airtime_reads_in_minutes() {
        assert_eq!(minutes_text(59.0), "0′");
        assert_eq!(minutes_text(12.5 * 60.0), "12′");
        assert_eq!(minutes_text(65.0 * 60.0), "1 h 05′");
    }
}
