//! Session de mesure : une ligne par image tant qu'elle enregistre, exportable en CSV (pour J9).
//!
//! Tout est testable sans fenêtre : `write_csv` écrit dans n'importe quel `Write`.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use thiserror::Error;

/// Une image de la session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SessionRow {
    /// Secondes depuis le début de la session (rempli par [`MeasurementSession::record`]).
    pub t_s: f64,
    /// Durée de l'image, en ms.
    pub frame_ms: f32,
    /// Temps GPU de l'image (somme des passes de premier niveau), absent sans timestamps GPU.
    pub gpu_ms: Option<f64>,
    /// Entités du monde Bevy, tous types confondus.
    pub world_entities: u32,
    /// Entités qui peuvent être dessinées (portent une `ViewVisibility`).
    pub renderable_entities: u32,
    /// Parmi elles, celles que le culling garde visibles.
    pub visible_entities: u32,
    /// Maillages distincts parmi les entités visibles (borne basse du nombre de lots).
    pub visible_mesh_kinds: u32,
    /// Tronçons vivants du streaming.
    pub chunks_live: u32,
    /// Pièces affichées par les tronçons vivants.
    pub pieces_used: u32,
    /// Échelle de rendu courante.
    pub render_scale: f32,
}

/// En-tête du CSV, dans l'ordre d'écriture de [`SessionRow`] (le `fps` est dérivé de `frame_ms`).
pub const CSV_HEADER: &str = "t_s,frame_ms,fps,gpu_ms,world_entities,renderable_entities,visible_entities,\
visible_mesh_kinds,chunks_live,pieces_used,render_scale";

/// Échec de l'export d'une session.
#[derive(Debug, Error)]
pub enum ExportError {
    /// Le fichier n'a pas pu être créé ou écrit.
    #[error("écriture de {path} impossible : {source}")]
    Io {
        /// Fichier visé.
        path: PathBuf,
        /// Cause.
        source: io::Error,
    },
}

/// La session de mesure en cours ou terminée.
///
/// Les lignes sont préallouées (`max_rows`) : enregistrer ne sème aucune allocation. Au-delà, l'enregistrement
/// s'arrête et [`MeasurementSession::truncated`] le dit, plutôt que de grossir sans fin pendant un set entier.
#[derive(Resource, Debug)]
pub struct MeasurementSession {
    recording: bool,
    truncated: bool,
    started_at_s: f64,
    max_rows: usize,
    rows: Vec<SessionRow>,
}

impl MeasurementSession {
    /// 10 minutes à 60 i/s.
    pub const DEFAULT_MAX_ROWS: usize = 36_000;

    /// Session vide qui accepte jusqu'à `max_rows` images.
    #[must_use]
    pub fn new(max_rows: usize) -> Self {
        Self {
            recording: false,
            truncated: false,
            started_at_s: 0.0,
            max_rows,
            rows: Vec::with_capacity(max_rows),
        }
    }

    /// Démarre une nouvelle session à l'instant `now_s` (horloge réelle) ; efface la précédente.
    pub fn start(&mut self, now_s: f64) {
        self.rows.clear();
        self.truncated = false;
        self.started_at_s = now_s;
        self.recording = true;
    }

    /// Arrête l'enregistrement ; les lignes restent exportables.
    pub fn stop(&mut self) {
        self.recording = false;
    }

    /// Vrai pendant l'enregistrement.
    #[must_use]
    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// Vrai si la session a atteint `max_rows` et s'est arrêtée d'elle-même.
    #[must_use]
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// Les images enregistrées.
    #[must_use]
    pub fn rows(&self) -> &[SessionRow] {
        &self.rows
    }

    /// Ajoute une image (sans effet hors enregistrement). `now_s` est lu sur l'horloge réelle.
    pub fn record(&mut self, now_s: f64, mut row: SessionRow) {
        if !self.recording {
            return;
        }
        if self.rows.len() >= self.max_rows {
            self.recording = false;
            self.truncated = true;
            return;
        }
        row.t_s = now_s - self.started_at_s;
        self.rows.push(row);
    }

    /// Écrit la session en CSV (virgule, point décimal, champ vide pour une mesure indisponible).
    ///
    /// # Errors
    /// Toute erreur d'écriture du `writer`.
    pub fn write_csv<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writeln!(writer, "{CSV_HEADER}")?;
        for row in &self.rows {
            let fps = if row.frame_ms > 0.0 { 1000.0 / row.frame_ms } else { 0.0 };
            write!(writer, "{:.4},{:.4},{fps:.2},", row.t_s, row.frame_ms)?;
            if let Some(gpu) = row.gpu_ms {
                write!(writer, "{gpu:.4}")?;
            }
            writeln!(
                writer,
                ",{},{},{},{},{},{},{:.3}",
                row.world_entities,
                row.renderable_entities,
                row.visible_entities,
                row.visible_mesh_kinds,
                row.chunks_live,
                row.pieces_used,
                row.render_scale
            )?;
        }
        Ok(())
    }

    /// Écrit la session dans `path` (le dossier parent est créé au besoin). Renvoie le nombre de lignes écrites.
    ///
    /// # Errors
    /// [`ExportError::Io`] si le dossier ou le fichier ne peut pas être créé ou écrit.
    pub fn save_csv(&self, path: &Path) -> Result<usize, ExportError> {
        let io_error = |source| ExportError::Io {
            path: path.to_path_buf(),
            source,
        };
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(io_error)?;
        }
        let file = std::fs::File::create(path).map_err(io_error)?;
        let mut writer = io::BufWriter::new(file);
        self.write_csv(&mut writer).map_err(io_error)?;
        writer.flush().map_err(io_error)?;
        Ok(self.rows.len())
    }
}

impl Default for MeasurementSession {
    fn default() -> Self {
        Self::new(Self::DEFAULT_MAX_ROWS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(frame_ms: f32, gpu_ms: Option<f64>) -> SessionRow {
        SessionRow {
            t_s: 0.0,
            frame_ms,
            gpu_ms,
            world_entities: 12,
            renderable_entities: 10,
            visible_entities: 4,
            visible_mesh_kinds: 2,
            chunks_live: 3,
            pieces_used: 8,
            render_scale: 1.0,
        }
    }

    #[test]
    fn hors_enregistrement_rien_n_est_garde() {
        let mut session = MeasurementSession::new(10);
        session.record(1.0, row(16.0, None));
        assert_eq!(session.rows(), []);
    }

    #[test]
    fn le_csv_a_un_en_tete_et_un_champ_vide_sans_gpu() {
        let mut session = MeasurementSession::new(10);
        session.start(5.0);
        session.record(5.5, row(20.0, None));
        session.record(5.52, row(10.0, Some(2.5)));
        let mut out = Vec::new();
        assert!(session.write_csv(&mut out).is_ok());
        let text = String::from_utf8_lossy(&out);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], CSV_HEADER);
        assert_eq!(lines[1], "0.5000,20.0000,50.00,,12,10,4,2,3,8,1.000");
        assert!(lines[2].starts_with("0.5200,10.0000,100.00,2.5000,"), "{}", lines[2]);
        let columns = CSV_HEADER.split(',').count();
        assert!(lines.iter().all(|l| l.split(',').count() == columns));
    }

    #[test]
    fn la_session_s_arrete_quand_elle_est_pleine() {
        let mut session = MeasurementSession::new(2);
        session.start(0.0);
        for i in 0..5 {
            session.record(f64::from(i), row(16.0, None));
        }
        assert_eq!(session.rows().len(), 2);
        assert!(session.truncated());
        assert!(!session.is_recording());
    }

    #[test]
    fn redemarrer_efface_la_session_precedente() {
        let mut session = MeasurementSession::new(4);
        session.start(0.0);
        session.record(1.0, row(16.0, None));
        session.stop();
        session.start(10.0);
        assert_eq!(session.rows(), []);
        assert!(session.is_recording());
    }

    #[test]
    fn l_export_cree_le_dossier_et_le_fichier() {
        let Ok(tmp) = tempfile::tempdir() else {
            return;
        };
        let path = tmp.path().join("sous-dossier").join("mesure.csv");
        let mut session = MeasurementSession::new(4);
        session.start(0.0);
        session.record(0.1, row(16.0, Some(1.0)));
        assert_eq!(session.save_csv(&path).ok(), Some(1));
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(text.starts_with(CSV_HEADER));
        let impossible = path.join("x.csv");
        assert!(
            session.save_csv(&impossible).is_err(),
            "un fichier ne peut pas servir de dossier"
        );
    }
}
