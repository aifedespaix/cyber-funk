//! Playlist des pistes de démo et modes de lecture (portage de la partie playlist de `engine/audio.ts`).
//!
//! Logique pure, sans audio ni horloge : le lecteur ([`crate::player`]) lui demande quel titre charger, elle répond.
//! Le hasard passe par un RNG à graine injectée (tests déterministes).

mod scan;
mod track_name;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

pub use scan::{LOCAL_DIR, ScanError, is_audio_file, scan_music_dir, track_from_path};
pub use track_name::parse_track_name;

/// Taille de l'historique de « précédent » (Sillage : `HISTORY`).
const HISTORY: usize = 24;

/// Au-delà de cette position (s), « précédent » relance le titre au lieu de revenir au précédent, comme un vrai
/// lecteur.
pub const PREV_RESTART_AFTER_S: f64 = 3.0;

/// Une piste de démo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    /// Titre nettoyé, tiré du nom de fichier.
    pub title: String,
    /// Artiste nettoyé ; vide si le nom ne suit pas « Artiste - Titre ».
    pub artist: String,
    /// Chemin du fichier.
    pub path: PathBuf,
    /// Piste personnelle (`assets/music/local/`) : jamais versionnée ni redistribuée.
    pub local: bool,
}

/// Que faire à la fin d'un titre : enchaîner au hasard, dans l'ordre, ou le rejouer (Sillage : `PLAY_MODES`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayMode {
    /// Titre suivant tiré au hasard, jamais le même deux fois de suite. Mode par défaut de Sillage.
    #[default]
    Random,
    /// Titres dans l'ordre de la liste, en boucle.
    Sequence,
    /// Le titre courant est rejoué à la fin ; « suivant » avance dans l'ordre.
    Loop,
}

/// Ce que le lecteur doit faire après une commande de navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Charger le titre d'indice donné, depuis le début.
    Load(usize),
    /// Relancer le titre courant depuis le début.
    Restart,
}

/// Liste de pistes, titre courant, mode de lecture et historique de « précédent ».
#[derive(Debug, Clone)]
pub struct Playlist {
    tracks: Arc<[Track]>,
    index: usize,
    mode: PlayMode,
    history: VecDeque<usize>,
    rng: Pcg32,
}

impl Playlist {
    /// Playlist sur `tracks`, placée sur `start` (borné à la liste) ou sur un titre tiré au hasard, comme
    /// `setTracks` de Sillage. `seed` fixe le hasard (tirage du départ et mode [`PlayMode::Random`]).
    #[must_use]
    pub fn new(tracks: Vec<Track>, start: Option<usize>, seed: u64) -> Self {
        let mut rng = Pcg32::seed_from_u64(seed);
        let count = tracks.len();
        let index = match start {
            _ if count == 0 => 0,
            Some(start) => start.min(count - 1),
            None => random_below(&mut rng, count),
        };
        Self {
            tracks: tracks.into(),
            index,
            mode: PlayMode::default(),
            history: VecDeque::with_capacity(HISTORY),
            rng,
        }
    }

    /// Toutes les pistes, dans l'ordre de la liste.
    #[must_use]
    pub fn tracks(&self) -> &Arc<[Track]> {
        &self.tracks
    }

    /// Indice du titre courant (`None` si la liste est vide).
    #[must_use]
    pub fn index(&self) -> Option<usize> {
        (!self.tracks.is_empty()).then_some(self.index)
    }

    /// Titre courant.
    #[must_use]
    pub fn current(&self) -> Option<&Track> {
        self.tracks.get(self.index)
    }

    /// Mode de lecture courant.
    #[must_use]
    pub fn mode(&self) -> PlayMode {
        self.mode
    }

    /// Change le mode de lecture ; prend effet à la prochaine fin de titre ou au prochain « suivant ».
    pub fn set_mode(&mut self, mode: PlayMode) {
        self.mode = mode;
    }

    /// Fin naturelle du titre courant : rejouer (mode [`PlayMode::Loop`]) ou enchaîner selon le mode.
    pub fn on_track_end(&mut self) -> Option<Step> {
        if self.tracks.is_empty() {
            return None;
        }
        if self.mode == PlayMode::Loop {
            return Some(Step::Restart);
        }
        let next = self.next_index();
        Some(self.go(next, true))
    }

    /// « Suivant » demandé par l'opérateur.
    pub fn next_track(&mut self) -> Option<Step> {
        if self.tracks.is_empty() {
            return None;
        }
        let next = self.next_index();
        Some(self.go(next, true))
    }

    /// « Précédent » : relance le titre si on en a écouté plus de [`PREV_RESTART_AFTER_S`], sinon revient au titre
    /// précédent de l'historique, ou à défaut au titre d'avant dans la liste.
    pub fn prev_track(&mut self, position_s: f64) -> Option<Step> {
        if self.tracks.is_empty() {
            return None;
        }
        if position_s > PREV_RESTART_AFTER_S {
            return Some(Step::Restart);
        }
        match self.history.pop_back() {
            Some(previous) => Some(self.go(previous, false)),
            None => self.select(self.index.cast_signed() - 1),
        }
    }

    /// Choisit un titre par son indice ; les indices hors liste font le tour (`-1` = dernier titre).
    pub fn select(&mut self, index: isize) -> Option<Step> {
        let count = self.tracks.len().cast_signed();
        if count == 0 {
            return None;
        }
        Some(self.go(index.rem_euclid(count).cast_unsigned(), true))
    }

    /// Replace le titre courant sans toucher à l'historique : le lecteur s'en sert quand le décodeur, parti en avance
    /// sur le titre suivant, doit revenir au titre que l'on entend encore.
    pub(crate) fn resync(&mut self, index: usize) {
        if index < self.tracks.len() && index != self.index {
            // L'avance avait empilé ce titre dans l'historique : on l'en retire, il redevient le titre courant.
            if self.history.back() == Some(&index) {
                self.history.pop_back();
            }
            self.index = index;
        }
    }

    /// Titre suivant selon le mode : au hasard (jamais le même deux fois de suite) ou dans l'ordre.
    fn next_index(&mut self) -> usize {
        let count = self.tracks.len();
        if count <= 1 {
            return 0;
        }
        match self.mode {
            PlayMode::Sequence | PlayMode::Loop => (self.index + 1) % count,
            PlayMode::Random => {
                let mut pick = random_below(&mut self.rng, count - 1);
                if pick >= self.index {
                    pick += 1;
                }
                pick
            }
        }
    }

    fn go(&mut self, index: usize, remember: bool) -> Step {
        if remember && index != self.index {
            if self.history.len() == HISTORY {
                self.history.pop_front();
            }
            self.history.push_back(self.index);
        }
        self.index = index;
        Step::Load(index)
    }
}

/// Entier uniforme dans `0..n` (`n > 0`). Le léger biais du modulo sur 64 bits est négligeable pour une playlist.
fn random_below(rng: &mut Pcg32, n: usize) -> usize {
    (rng.next_u64() % n as u64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracks(n: usize) -> Vec<Track> {
        (0..n)
            .map(|i| Track {
                title: format!("T{i}"),
                artist: String::new(),
                path: PathBuf::from(format!("{i}.mp3")),
                local: false,
            })
            .collect()
    }

    #[test]
    fn empty_playlist_does_nothing() {
        let mut list = Playlist::new(Vec::new(), None, 1);
        assert_eq!(list.index(), None);
        assert_eq!(list.next_track(), None);
        assert_eq!(list.prev_track(0.0), None);
        assert_eq!(list.on_track_end(), None);
        assert_eq!(list.select(3), None);
    }

    #[test]
    fn start_is_clamped_or_random_but_seeded() {
        assert_eq!(Playlist::new(tracks(4), Some(9), 1).index(), Some(3));
        let a = Playlist::new(tracks(10), None, 42).index();
        let b = Playlist::new(tracks(10), None, 42).index();
        assert_eq!(a, b);
    }

    #[test]
    fn sequence_wraps_around() {
        let mut list = Playlist::new(tracks(3), Some(1), 1);
        list.set_mode(PlayMode::Sequence);
        assert_eq!(list.on_track_end(), Some(Step::Load(2)));
        assert_eq!(list.on_track_end(), Some(Step::Load(0)));
        assert_eq!(list.next_track(), Some(Step::Load(1)));
    }

    #[test]
    fn loop_replays_at_end_but_next_advances() {
        let mut list = Playlist::new(tracks(3), Some(0), 1);
        list.set_mode(PlayMode::Loop);
        assert_eq!(list.on_track_end(), Some(Step::Restart));
        assert_eq!(list.index(), Some(0));
        assert_eq!(list.next_track(), Some(Step::Load(1)));
    }

    #[test]
    fn random_never_repeats_and_covers_the_list() {
        let mut list = Playlist::new(tracks(4), Some(0), 7);
        let mut seen = [false; 4];
        let mut last = 0;
        for _ in 0..200 {
            let loaded = match list.on_track_end() {
                Some(Step::Load(i)) => Some(i),
                _ => None,
            };
            let i = loaded.expect("le mode hasard charge toujours un titre");
            assert_ne!(i, last, "jamais le même titre deux fois de suite");
            seen[i] = true;
            last = i;
        }
        assert!(seen.iter().all(|s| *s));
    }

    #[test]
    fn single_track_random_stays_on_it() {
        let mut list = Playlist::new(tracks(1), None, 3);
        assert_eq!(list.next_track(), Some(Step::Load(0)));
    }

    #[test]
    fn prev_restarts_after_three_seconds_then_walks_history() {
        let mut list = Playlist::new(tracks(5), Some(0), 1);
        list.set_mode(PlayMode::Sequence);
        list.select(3);
        list.next_track(); // 4
        assert_eq!(list.prev_track(10.0), Some(Step::Restart));
        assert_eq!(list.prev_track(1.0), Some(Step::Load(3)));
        assert_eq!(list.prev_track(1.0), Some(Step::Load(0)));
        // Historique vide : on recule dans la liste, en faisant le tour.
        assert_eq!(list.prev_track(1.0), Some(Step::Load(4)));
    }

    #[test]
    fn history_is_bounded() {
        let mut list = Playlist::new(tracks(2), Some(0), 1);
        list.set_mode(PlayMode::Sequence);
        for _ in 0..100 {
            list.next_track();
        }
        assert_eq!(list.history.len(), HISTORY);
    }

    #[test]
    fn select_wraps_negative_indices() {
        let mut list = Playlist::new(tracks(3), Some(0), 1);
        assert_eq!(list.select(-1), Some(Step::Load(2)));
        assert_eq!(list.select(4), Some(Step::Load(1)));
    }
}
