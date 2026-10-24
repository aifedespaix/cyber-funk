//! Préchargement de fichiers (musiques, logos) dans une tâche de fond.
//!
//! On lit chaque fichier en entier : l'absence ou l'illisibilité est signalée à l'ouverture, et le système de fichiers
//! garde le contenu en cache pour le premier accès du show (règle 8 : rien ne se charge pendant un show).

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task, futures_lite::future};

use super::tracker::{LoadTracker, StepId};

/// Une lecture en cours : l'étape à clore et la tâche qui rend la liste des fichiers en échec.
struct FileJob {
    step: StepId,
    task: Task<Vec<String>>,
}

/// Les lectures de fichiers en cours.
#[derive(Resource, Default)]
pub struct FileJobs {
    jobs: Vec<FileJob>,
}

impl std::fmt::Debug for FileJobs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileJobs").field("en_cours", &self.jobs.len()).finish()
    }
}

impl FileJobs {
    /// Lance la lecture de `paths` sous une étape nommée `label`. Sans fichier, l'étape est close d'emblée.
    pub fn spawn(&mut self, tracker: &mut LoadTracker, label: impl Into<String>, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            tracker.register_done(label);
            return;
        }
        let step = tracker.register(label);
        let task = IoTaskPool::get().spawn(async move {
            paths
                .iter()
                .filter_map(|path| match std::fs::read(path) {
                    Ok(_) => None,
                    Err(error) => Some(format!("{} : {error}", path.display())),
                })
                .collect()
        });
        self.jobs.push(FileJob { step, task });
    }
}

/// Clôt les étapes dont la lecture est finie.
pub(super) fn poll_file_jobs(mut jobs: ResMut<FileJobs>, mut tracker: ResMut<LoadTracker>) {
    jobs.jobs.retain_mut(|job| {
        let Some(failures) = future::block_on(future::poll_once(&mut job.task)) else {
            return true;
        };
        if failures.is_empty() {
            tracker.finish(job.step);
        } else {
            tracker.fail(job.step, failures.join(" ; "));
        }
        false
    });
}
