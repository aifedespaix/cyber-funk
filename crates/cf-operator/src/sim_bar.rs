//! Barre de pilotage de la simulation, flottante en haut de la fenêtre opérateur : horloge à la milliseconde,
//! pause / reprise et reset. Toujours visible, y compris en vue Live (c'est là qu'on en a besoin quand tout se fige).
//!
//! Le reset est volontairement en deux temps (clic, puis confirmation dans la barre) : en live, un clic involontaire
//! ne doit pas relancer le show.

use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use bevy_egui::{EguiContext, PrimaryEguiContext, egui};
use cf_render::{SimClock, SimControl};

/// Secondes de la journée UTC → décalage local : la bibliothèque standard ne donne pas le fuseau, on affiche donc
/// l'heure UTC, étiquetée comme telle.
const DAY_S: u64 = 86_400;

/// Étape de confirmation du reset.
#[derive(Debug, Default)]
pub(crate) struct BarState {
    confirm_reset: bool,
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

/// La barre. Sans `SimControl` (le noyau n'est pas monté), rien n'est affiché.
pub(crate) fn sim_bar(
    mut context: Single<&mut EguiContext, With<PrimaryEguiContext>>,
    control: Option<ResMut<SimControl>>,
    clock: Option<Res<SimClock>>,
    mut state: Local<BarState>,
) {
    let (Some(mut control), Some(clock)) = (control, clock) else {
        return;
    };
    let ctx = context.get_mut();
    egui::Window::new("sim_bar")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 6.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let paused = control.paused();
                let sim = egui::RichText::new(clock_text(clock.sim_ms())).monospace().size(20.0);
                ui.label(if paused { sim.color(egui::Color32::YELLOW) } else { sim });
                ui.weak(format!("sim · image {}", clock.frame()));
                ui.separator();
                ui.label(egui::RichText::new(wall_text()).monospace());
                ui.weak("UTC");
                ui.separator();
                let label = if paused { "▶ Reprendre" } else { "⏸ Pause" };
                if ui
                    .button(label)
                    .on_hover_text(
                        "Fige le chef du spectacle, le monde et la caméra, et met la musique de démo en pause.",
                    )
                    .clicked()
                {
                    control.toggle_pause();
                }
                if state.confirm_reset {
                    if ui.button("Confirmer le reset").clicked() {
                        control.request_reset();
                        state.confirm_reset = false;
                    }
                    if ui.button("Annuler").clicked() {
                        state.confirm_reset = false;
                    }
                } else if ui
                    .button("⟲ Reset")
                    .on_hover_text("Relance la simulation au premier cue et la musique au début de la piste.")
                    .clicked()
                {
                    state.confirm_reset = true;
                }
                if paused {
                    ui.colored_label(egui::Color32::YELLOW, "EN PAUSE");
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_horloge_s_ecrit_a_la_milliseconde() {
        assert_eq!(clock_text(0), "00:00:00.000");
        assert_eq!(clock_text(3_723_045), "01:02:03.045");
        assert_eq!(clock_text(100 * 3_600_000), "100:00:00.000");
    }
}
