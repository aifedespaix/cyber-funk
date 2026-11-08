//! Le sélecteur de fichiers du système, pour ajouter des musiques à la playlist.
//!
//! La boîte de dialogue bloque son thread jusqu'à la réponse de l'opérateur : elle tourne donc sur un thread à part,
//! jamais sur celui de Bevy (le projecteur continuerait sinon de se figer pendant tout le temps du choix). Les pistes
//! choisies reviennent par un canal que l'interface relit à chaque image.

use std::sync::Mutex;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use bevy::prelude::*;
use cf_audio::playlist::{AUDIO_EXTENSIONS, Track, track_from_path};

/// Le choix de fichiers en cours, s'il y en a un.
#[derive(Resource, Default)]
pub(crate) struct FilePicker {
    pending: Mutex<Option<Receiver<Vec<Track>>>>,
}

impl std::fmt::Debug for FilePicker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilePicker").field("en_cours", &self.is_open()).finish()
    }
}

impl FilePicker {
    /// Une boîte de dialogue est-elle ouverte ?
    pub(crate) fn is_open(&self) -> bool {
        self.pending.lock().is_ok_and(|pending| pending.is_some())
    }

    /// Ouvre le sélecteur, sauf s'il l'est déjà. Les pistes sont marquées « perso » : ce sont des fichiers de
    /// l'opérateur, jamais copiés dans `assets/` ni redistribués.
    pub(crate) fn open(&self) {
        let Ok(mut pending) = self.pending.lock() else {
            return;
        };
        if pending.is_some() {
            return;
        }
        let (sender, receiver) = channel();
        let spawned = std::thread::Builder::new()
            .name("cf-operator-file-dialog".into())
            .spawn(move || {
                let picked = rfd::FileDialog::new()
                    .set_title("Ajouter des musiques")
                    .add_filter("Audio", &AUDIO_EXTENSIONS)
                    .pick_files()
                    .unwrap_or_default();
                // Ouvre chaque fichier pour lire sa durée : ici, hors du thread de rendu.
                let tracks = picked.into_iter().map(|path| track_from_path(path, true)).collect();
                let _ = sender.send(tracks);
            });
        match spawned {
            Ok(_) => *pending = Some(receiver),
            Err(error) => tracing::warn!(%error, "sélecteur de fichiers : thread impossible à créer"),
        }
    }

    /// Les pistes choisies depuis la dernière image, une seule fois ; `None` si rien n'est arrivé.
    pub(crate) fn poll(&self) -> Option<Vec<Track>> {
        let mut pending = self.pending.lock().ok()?;
        let receiver = pending.as_ref()?;
        match receiver.try_recv() {
            Ok(tracks) => {
                *pending = None;
                Some(tracks)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                *pending = None;
                None
            }
        }
    }

    #[cfg(test)]
    fn listen(&self, receiver: Receiver<Vec<Track>>) {
        if let Ok(mut pending) = self.pending.lock() {
            *pending = Some(receiver);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn track() -> Track {
        track_from_path(PathBuf::from("absent/Artiste - Titre.mp3"), true)
    }

    #[test]
    fn sans_dialogue_ouvert_rien_n_arrive() {
        let picker = FilePicker::default();
        assert!(!picker.is_open());
        assert_eq!(picker.poll(), None);
    }

    #[test]
    fn les_pistes_choisies_arrivent_une_seule_fois() {
        let picker = FilePicker::default();
        let (sender, receiver) = channel();
        picker.listen(receiver);
        assert!(picker.is_open());
        assert_eq!(picker.poll(), None, "pas encore de réponse");
        sender.send(vec![track()]).expect("envoi");
        assert_eq!(picker.poll().map(|t| t.len()), Some(1));
        assert!(!picker.is_open());
        assert_eq!(picker.poll(), None);
    }

    #[test]
    fn un_dialogue_abandonne_se_referme_sans_pistes() {
        let picker = FilePicker::default();
        let (sender, receiver) = channel::<Vec<Track>>();
        picker.listen(receiver);
        drop(sender);
        assert_eq!(picker.poll(), None);
        assert!(!picker.is_open());
    }

    #[test]
    fn une_piste_inexistante_est_marquee_illisible_au_lieu_de_planter() {
        let t = track();
        assert!(t.error.is_some());
        assert!(t.local);
    }
}
