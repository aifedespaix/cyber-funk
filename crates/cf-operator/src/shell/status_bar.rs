//! La barre de statut permanente, dessinée dans tous les onglets et en Live : source audio, i/s, tempo, DJ en cours et
//! temps restant, automatisme du monde, événements en cours, horloge et pilotage de la simulation (repris de l'ancienne
//! `sim_bar`), « Passer au suivant » et le bouton Noir.
//!
//! Deux gestes sont en deux temps (clic, puis confirmation dans la barre, voir `confirm`) : le reset et « Passer au
//! suivant ». En live, un
//! clic involontaire ne doit ni relancer le show ni couper un DJ. Le Noir, lui, est toujours actif et immédiat : c'est
//! le geste de sécurité. Il passe par `Action::Blackout`, donc par le chef du spectacle (règle 7 inchangée).
//!
//! `draw` ne touche pas à Bevy : il rend des [`StatusAction`] et se teste sans écran.

use std::time::{SystemTime, UNIX_EPOCH};

use bevy_egui::egui::{self, Color32, RichText};

use super::ShellState;
use super::confirm::{CONFIRM_DELAY_S, Confirm};
use super::shortcuts::{self, KEY_BLACKOUT};

/// Secondes de la journée : la bibliothèque standard ne donne pas le fuseau, l'heure murale est donc affichée en UTC,
/// étiquetée comme telle.
const DAY_S: u64 = 86_400;
/// Minutes d'une journée (horaires prévus qui passent minuit).
const DAY_MIN: u32 = 1440;
/// Texte d'une valeur indisponible : jamais un zéro inventé.
const NONE_TEXT: &str = "—";
/// Fond du bouton Noir, visible dans le noir de la salle.
const NOIR_FILL: Color32 = Color32::from_rgb(150, 20, 30);

/// Un geste de l'opérateur dans la barre, appliqué par la coque.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusAction {
    /// Basculer le Noir.
    Blackout,
    /// Passer au DJ suivant (confirmé).
    NextDj,
    /// Pause / reprise de la simulation.
    TogglePause,
    /// Basculer l'automatisme du monde (manuel / auto).
    ToggleManual,
    /// Reset de la simulation (confirmé).
    ConfirmReset,
}

/// Les valeurs lues pour la barre. `None` : la ressource n'existe pas (plugin non monté), la barre affiche « — ».
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct StatusData<'a> {
    /// Source audio (nom court) et l'analyse tourne.
    pub audio: Option<(&'static str, bool)>,
    /// Images par seconde.
    pub fps: Option<f32>,
    /// Tempo (BPM).
    pub bpm: Option<f32>,
    /// DJ en cours : nom (peut être vide) et rang du créneau.
    pub dj: Option<(&'a str, usize)>,
    /// Secondes restantes du créneau d'après son horaire prévu.
    pub remaining_s: Option<f64>,
    /// Nombre d'événements du monde en cours.
    pub active_events: Option<usize>,
    /// Le monde est en mode manuel.
    pub manual: Option<bool>,
    /// La simulation est en pause.
    pub paused: Option<bool>,
    /// Horloge de simulation : millisecondes et image.
    pub clock: Option<(u64, u64)>,
    /// Un show est chargé (« Passer au suivant » a un sens).
    pub has_show: bool,
    /// Le Noir est allumé.
    pub blackout_on: bool,
    /// Secondes réelles de l'application (délais des confirmations).
    pub now_s: f64,
    /// Rang de chargement du créneau en cours (`ShowState::slot_seq`, 0 sans show) : un autre DJ désarme « Passer au
    /// suivant ».
    pub slot_seq: u32,
}

/// Quels boutons sont actifs pour ces valeurs. Quatre drapeaux indépendants, un par bouton : une machine à états
/// n'aurait pas de sens ici.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Enabled {
    pub blackout: bool,
    pub next_dj: bool,
    pub pause: bool,
    pub manual: bool,
}

/// Les boutons actifs. Le Noir l'est toujours, quoi qu'il manque.
pub(crate) fn enabled(data: &StatusData<'_>) -> Enabled {
    Enabled {
        blackout: true,
        next_dj: data.has_show,
        pause: data.paused.is_some(),
        manual: data.manual.is_some(),
    }
}

/// `HH:MM:SS.mmm` pour un nombre de millisecondes (les heures dépassent 24 sans retour à zéro).
pub(crate) fn clock_text(ms: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

/// Heure UTC de la journée en `HH:MM:SS.mmm`.
fn wall_text() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0));
    clock_text(ms % (DAY_S * 1000))
}

/// Secondes restantes d'un créneau : durée prévue (fin − début, minuit compris) moins le temps écoulé depuis son
/// chargement. `None` sans horaire. Négatif quand le DJ déborde.
pub(crate) fn remaining_s(schedule: Option<(u16, u16)>, elapsed_s: f64) -> Option<f64> {
    let (start, end) = schedule?;
    let length_min = (u32::from(end) + DAY_MIN - u32::from(start)) % DAY_MIN;
    Some(f64::from(length_min) * 60.0 - elapsed_s)
}

/// `mm:ss` (ou `h:mm:ss`), précédé d'un signe moins quand le temps est dépassé.
fn remaining_text(seconds: f64) -> String {
    let sign = if seconds < 0.0 { "−" } else { "" };
    format!("{sign}{}", crate::live::clock(seconds.abs()))
}

fn or_none<T>(value: Option<T>, show: impl FnOnce(T) -> String) -> String {
    value.map_or_else(|| NONE_TEXT.to_owned(), show)
}

/// Dessine la barre et rend les gestes de l'image (vide le plus souvent : aucune allocation).
pub(crate) fn draw(ui: &mut egui::Ui, data: &StatusData<'_>, state: &mut ShellState) -> Vec<StatusAction> {
    let mut actions = Vec::new();
    let on = enabled(data);
    ui.horizontal_wrapped(|ui| {
        let audio = or_none(data.audio, |(source, running)| {
            format!("{source} · {}", if running { "analyse ok" } else { "analyse arrêtée" })
        });
        ui.label(format!("Audio : {audio}"));
        ui.label(format!("{} i/s", or_none(data.fps, |f| format!("{f:.0}"))));
        ui.label(format!("{} BPM", or_none(data.bpm, |b| format!("{b:.1}"))));
        ui.separator();
        let dj = or_none(data.dj, |(name, slot)| crate::live::dj_name(name, slot));
        ui.label(format!("DJ : {dj}"));
        ui.label(format!("reste {}", or_none(data.remaining_s, remaining_text)))
            .on_hover_text("D'après l'horaire prévu du créneau.");
        next_dj(ui, data, on.next_dj, state, &mut actions);
        ui.separator();
        let manual = or_none(data.manual, |m| (if m { "Manuel" } else { "Auto" }).to_owned());
        if ui
            .add_enabled(on.manual, egui::Button::new(format!("Monde : {manual}")))
            .on_hover_text("Manuel : le monde ne lance plus d'événement de lui-même ; les clics passent toujours.")
            .clicked()
        {
            actions.push(StatusAction::ToggleManual);
        }
        ui.label(format!("{} en cours", or_none(data.active_events, |n| n.to_string())));
        ui.separator();
        simulation(ui, data, on.pause, state, &mut actions);
        ui.separator();
        let mut noir = RichText::new("■ Noir").strong();
        if data.blackout_on {
            noir = noir.color(Color32::YELLOW);
        }
        let tip = format!("{} [{KEY_BLACKOUT}]", shortcuts::describe(KEY_BLACKOUT));
        if ui
            .add_enabled(on.blackout, egui::Button::new(noir).fill(NOIR_FILL))
            .on_hover_text(tip)
            .clicked()
        {
            actions.push(StatusAction::Blackout);
        }
        if data.blackout_on {
            ui.colored_label(Color32::YELLOW, "NOIR");
        }
    });
    actions
}

/// Le bouton de confirmation d'un geste armé : grisé tant qu'il n'accepte pas de clic (un double-clic sur le bouton
/// d'origine ne le confirme donc pas). Rend `true` sur un clic accepté.
fn confirm_button(ui: &mut egui::Ui, confirm: Confirm, now_s: f64, label: &str) -> bool {
    let ready = confirm.ready(now_s);
    if !ready {
        // L'image où le bouton devient cliquable doit être dessinée même sans mouvement de souris.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(CONFIRM_DELAY_S));
    }
    ui.add_enabled(ready, egui::Button::new(label)).clicked()
}

/// « Passer au suivant », en deux temps, lié au créneau en cours.
fn next_dj(
    ui: &mut egui::Ui,
    data: &StatusData<'_>,
    enabled: bool,
    state: &mut ShellState,
    actions: &mut Vec<StatusAction>,
) {
    if state.confirm_next_dj.is_armed() && enabled {
        if confirm_button(ui, state.confirm_next_dj, data.now_s, "Confirmer : DJ suivant")
            && state.click_next_dj(data.now_s, data.slot_seq)
        {
            actions.push(StatusAction::NextDj);
        }
        if ui.button("Annuler").clicked() {
            state.confirm_next_dj.disarm();
        }
    } else if ui
        .add_enabled(enabled, egui::Button::new("Passer au suivant"))
        .on_hover_text("Lance la passation au DJ suivant (demande une confirmation).")
        .clicked()
    {
        state.click_next_dj(data.now_s, data.slot_seq);
    }
}

/// Horloge à la milliseconde, heure UTC, pause / reprise et reset en deux temps (l'ancienne `sim_bar`).
fn simulation(
    ui: &mut egui::Ui,
    data: &StatusData<'_>,
    enabled: bool,
    state: &mut ShellState,
    actions: &mut Vec<StatusAction>,
) {
    let paused = data.paused.unwrap_or(false);
    if let Some((ms, frame)) = data.clock {
        let sim = RichText::new(clock_text(ms)).monospace();
        ui.label(if paused { sim.color(Color32::YELLOW) } else { sim });
        ui.weak(format!("sim · image {frame}"));
    }
    ui.label(RichText::new(wall_text()).monospace());
    ui.weak("UTC");
    let label = if paused { "▶ Reprendre" } else { "⏸ Pause" };
    if ui
        .add_enabled(enabled, egui::Button::new(label))
        .on_hover_text("Fige le chef du spectacle, le monde et la caméra, et met la musique de démo en pause.")
        .clicked()
    {
        actions.push(StatusAction::TogglePause);
    }
    if state.confirm_reset.is_armed() && enabled {
        if confirm_button(ui, state.confirm_reset, data.now_s, "Confirmer le reset") && state.click_reset(data.now_s) {
            actions.push(StatusAction::ConfirmReset);
        }
        if ui.button("Annuler").clicked() {
            state.confirm_reset.disarm();
        }
    } else if ui
        .add_enabled(enabled, egui::Button::new("⟲ Reset"))
        .on_hover_text("Relance la simulation au premier cue et la musique au début de la piste.")
        .clicked()
    {
        state.click_reset(data.now_s);
    }
    if paused {
        ui.colored_label(Color32::YELLOW, "EN PAUSE");
    }
}
