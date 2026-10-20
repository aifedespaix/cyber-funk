//! Panneau de diagnostic de la fenêtre opérateur : i/s, temps d'image, passes de rendu, entités, tronçons, session de
//! mesure. Les chiffres viennent de `cf_render::DiagnosticsSnapshot` ; ce module ne mesure rien.
//!
//! Le panneau est une fenêtre egui de la fenêtre opérateur : il n'existe pas dans le projecteur. `F3` l'affiche ou le
//! cache.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use bevy_egui::{EguiContext, PrimaryEguiContext, egui};
use cf_render::{DiagnosticsSnapshot, MeasurementSession};

/// Dossier où les sessions de mesure sont écrites (relatif au dossier de lancement).
const EXPORT_DIR: &str = "measures";
/// Budget d'une image à 60 i/s, en ms : au-delà, le p99 est signalé.
const FRAME_BUDGET_MS: f32 = 1000.0 / 60.0;

/// État du panneau.
#[derive(Debug)]
pub(crate) struct PanelState {
    open: bool,
    /// Résultat du dernier export, affiché sous les boutons.
    last_export: Option<String>,
}

impl Default for PanelState {
    fn default() -> Self {
        Self {
            open: true,
            last_export: None,
        }
    }
}

/// Fenêtre « Diagnostic » de l'opérateur. Sans `DiagnosticsPlugin`, rien n'est affiché.
pub(crate) fn diagnostics_window(
    mut context: Single<&mut EguiContext, With<PrimaryEguiContext>>,
    snapshot: Option<Res<DiagnosticsSnapshot>>,
    session: Option<ResMut<MeasurementSession>>,
    time: Res<Time<Real>>,
    mut state: Local<PanelState>,
) {
    let (Some(snapshot), Some(mut session)) = (snapshot, session) else {
        return;
    };
    let ctx = context.get_mut();
    if ctx.input(|input| input.key_pressed(egui::Key::F3)) {
        state.open = !state.open;
    }
    if !state.open {
        return;
    }
    let now_s = time.elapsed_secs_f64();
    let mut open = true;
    egui::Window::new("Diagnostic (F3)")
        .open(&mut open)
        .default_pos(egui::pos2(16.0, 48.0))
        .resizable(false)
        .show(ctx, |ui| draw(ui, &snapshot, &mut session, &mut state, now_s));
    if !open {
        state.open = false;
    }
}

/// Contenu du panneau (séparé de la fenêtre pour être testé sans écran).
pub(crate) fn draw(
    ui: &mut egui::Ui,
    snapshot: &DiagnosticsSnapshot,
    session: &mut MeasurementSession,
    state: &mut PanelState,
    now_s: f64,
) {
    egui::Grid::new("diagnostics_grid")
        .num_columns(2)
        .spacing([16.0, 2.0])
        .show(ui, |ui| {
            line(ui, "Images / s", &format!("{:.1}", snapshot.fps));
            line(
                ui,
                "Temps d'image",
                &format!(
                    "{:.2} ms moyenne · {:.2} ms p99{}",
                    snapshot.frame_mean_ms,
                    snapshot.frame_p99_ms,
                    if snapshot.frame_p99_ms > FRAME_BUDGET_MS {
                        " (hors budget)"
                    } else {
                        ""
                    }
                ),
            );
            line(ui, "GPU (total)", &gpu_text(snapshot.gpu_total_ms));
            line(
                ui,
                "Draw calls",
                &format!(
                    "indisponible · {} maillages distincts visibles",
                    snapshot.visible_mesh_kinds
                ),
            );
            line(
                ui,
                "Entités visibles / dessinables",
                &format!("{} / {}", snapshot.visible_entities, snapshot.renderable_entities),
            );
            line(ui, "Entités du monde", &snapshot.world_entities.to_string());
            line(ui, "Tronçons actifs", &chunks_text(snapshot));
            line(ui, "Mémoire GPU (maillages)", &memory_text(snapshot.gpu_mesh_bytes));
            line(
                ui,
                "Échelle de rendu",
                &format!("{:.0} %", snapshot.render_scale * 100.0),
            );
        });

    ui.separator();
    ui.label("Passes de rendu (ms GPU · ms CPU)");
    if snapshot.passes.is_empty() {
        ui.weak("aucune mesure de passe");
    }
    egui::Grid::new("diagnostics_passes")
        .num_columns(3)
        .spacing([16.0, 2.0])
        .show(ui, |ui| {
            for pass in &snapshot.passes {
                ui.label(&pass.name);
                ui.label(gpu_text(pass.gpu_ms));
                ui.label(pass.cpu_ms.map_or_else(|| "n/d".to_owned(), |v| format!("{v:.3} ms")));
                ui.end_row();
            }
        });

    ui.separator();
    session_controls(ui, session, state, now_s);
}

fn line(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(label);
    ui.label(value);
    ui.end_row();
}

fn session_controls(ui: &mut egui::Ui, session: &mut MeasurementSession, state: &mut PanelState, now_s: f64) {
    ui.horizontal(|ui| {
        if session.is_recording() {
            if ui.button("Arrêter la session").clicked() {
                session.stop();
            }
            ui.label(format!("enregistrement : {} images", session.rows().len()));
        } else {
            if ui.button("Démarrer une session de mesure").clicked() {
                session.start(now_s);
                state.last_export = None;
            }
            let can_export = !session.rows().is_empty();
            if ui
                .add_enabled(can_export, egui::Button::new("Exporter en CSV"))
                .clicked()
            {
                state.last_export = Some(export_session(session, Path::new(EXPORT_DIR), unix_seconds()));
            }
        }
    });
    if session.truncated() {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            "Session pleine : l'enregistrement s'est arrêté tout seul.",
        );
    }
    if let Some(message) = &state.last_export {
        ui.label(message);
    }
}

/// Écrit la session dans `dir/cf-session-<horodatage>.csv` et renvoie le message à afficher à l'opérateur.
pub(crate) fn export_session(session: &MeasurementSession, dir: &Path, unix_seconds: u64) -> String {
    let path = export_path(dir, unix_seconds);
    match session.save_csv(&path) {
        Ok(rows) => format!("{rows} images écrites dans {}", path.display()),
        Err(error) => {
            warn!("{error}");
            format!("Export impossible : {error}")
        }
    }
}

/// Chemin d'une session exportée à la seconde `unix_seconds`.
pub(crate) fn export_path(dir: &Path, unix_seconds: u64) -> PathBuf {
    dir.join(format!("cf-session-{unix_seconds}.csv"))
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Un temps GPU, ou « indisponible » quand le matériel ou le pilote ne fournit pas de timestamps.
fn gpu_text(ms: Option<f64>) -> String {
    ms.map_or_else(|| "indisponible".to_owned(), |v| format!("{v:.3} ms"))
}

fn chunks_text(snapshot: &DiagnosticsSnapshot) -> String {
    snapshot.stream.map_or_else(
        || "aucun monde streamé".to_owned(),
        |s| {
            format!(
                "{} / {} (pièces {} / {})",
                s.chunks_live, s.chunks_capacity, s.pieces_used, s.pieces_pooled
            )
        },
    )
}

fn memory_text(bytes: Option<u64>) -> String {
    bytes.map_or_else(
        || "indisponible".to_owned(),
        |b| format!("{:.1} Mo", b as f64 / (1024.0 * 1024.0)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_render::{PassTiming, SessionRow};

    #[test]
    fn un_champ_indisponible_est_dit_et_non_affiche_a_zero() {
        assert_eq!(gpu_text(None), "indisponible");
        assert_eq!(gpu_text(Some(1.5)), "1.500 ms");
        assert_eq!(memory_text(None), "indisponible");
        assert_eq!(memory_text(Some(3 * 1024 * 1024)), "3.0 Mo");
        assert_eq!(chunks_text(&DiagnosticsSnapshot::default()), "aucun monde streamé");
    }

    fn draw_once(snapshot: &DiagnosticsSnapshot, session: &mut MeasurementSession, state: &mut PanelState) {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| draw(ui, snapshot, session, state, 0.0));
        output.drop_without_applying_deltas();
    }

    #[test]
    fn le_panneau_se_dessine_avec_et_sans_mesures() {
        let mut session = MeasurementSession::new(4);
        let mut state = PanelState::default();
        draw_once(&DiagnosticsSnapshot::default(), &mut session, &mut state);
        let snapshot = DiagnosticsSnapshot {
            fps: 60.0,
            passes: vec![PassTiming {
                name: "main_pass".into(),
                cpu_ms: Some(0.4),
                gpu_ms: None,
            }],
            ..default()
        };
        draw_once(&snapshot, &mut session, &mut state);
    }

    #[test]
    fn l_export_ecrit_le_fichier_et_dit_ou() {
        let Ok(tmp) = tempfile::tempdir() else {
            return;
        };
        let mut session = MeasurementSession::new(4);
        session.start(0.0);
        session.record(
            0.1,
            SessionRow {
                t_s: 0.0,
                frame_ms: 16.0,
                gpu_ms: None,
                world_entities: 1,
                renderable_entities: 1,
                visible_entities: 1,
                visible_mesh_kinds: 1,
                chunks_live: 0,
                pieces_used: 0,
                render_scale: 1.0,
            },
        );
        let message = export_session(&session, tmp.path(), 1_700_000_000);
        assert!(message.starts_with("1 images écrites dans"), "{message}");
        assert!(export_path(tmp.path(), 1_700_000_000).is_file());

        let blocked = tmp.path().join("occupe");
        assert!(std::fs::write(&blocked, b"x").is_ok());
        let message = export_session(&session, &blocked, 1);
        assert!(message.starts_with("Export impossible"), "{message}");
    }
}
