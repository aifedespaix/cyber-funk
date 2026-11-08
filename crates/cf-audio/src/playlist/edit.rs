//! Édition de la playlist pendant la lecture : réordonner, ajouter, retirer, et les durées affichées à l'opérateur.
//!
//! Toutes les opérations gardent le titre courant sur la même piste (retrouvée par son chemin) ; si on le retire,
//! c'est le titre qui prend sa place dans la liste qui devient courant. L'historique de « précédent » est ré-indexé.

use std::collections::VecDeque;
use std::sync::Arc;

use super::{Playlist, Track};

impl Playlist {
    /// Déplace la piste `from` à la position `to` (borné à la liste). Sans effet si l'un des deux est hors liste.
    pub fn move_track(&mut self, from: usize, to: usize) {
        let count = self.tracks.len();
        if from >= count || count == 0 {
            return;
        }
        let to = to.min(count - 1);
        if from == to {
            return;
        }
        let mut list = self.tracks.to_vec();
        let moved = list.remove(from);
        list.insert(to, moved);
        self.rebuild(list, |i| {
            Some(if i == from {
                to
            } else if from < to && i > from && i <= to {
                i - 1
            } else if from > to && i >= to && i < from {
                i + 1
            } else {
                i
            })
        });
    }

    /// Ajoute une piste en fin de liste. Si la liste était vide, elle devient le titre courant.
    pub fn add(&mut self, track: Track) {
        let mut list = self.tracks.to_vec();
        list.push(track);
        self.rebuild(list, Some);
    }

    /// Retire la piste `index` et la rend. Si c'était le titre courant, le titre suivant de la liste prend sa place
    /// (le dernier, s'il n'y en a plus après). L'appelant relance alors la lecture si elle était en cours.
    pub fn remove(&mut self, index: usize) -> Option<Track> {
        let removed = self.tracks.get(index)?.clone();
        let mut list = self.tracks.to_vec();
        list.remove(index);
        let was_current = index == self.index;
        self.rebuild(list, |i| match i.cmp(&index) {
            std::cmp::Ordering::Less => Some(i),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => Some(i - 1),
        });
        if was_current {
            self.index = index.min(self.tracks.len().saturating_sub(1));
        }
        Some(removed)
    }

    /// Remplace toute la liste (édition faite par l'opérateur) en gardant le titre courant s'il y est encore, retrouvé
    /// par son chemin. L'historique est vidé : ses indices ne veulent plus rien dire.
    pub fn replace_tracks(&mut self, tracks: Vec<Track>) {
        let current = self.current().map(|t| t.path.clone());
        self.tracks = tracks.into();
        self.history = VecDeque::with_capacity(super::HISTORY);
        self.index = current
            .and_then(|path| self.tracks.iter().position(|t| t.path == path))
            .unwrap_or_else(|| self.index.min(self.tracks.len().saturating_sub(1)));
    }

    /// Durée totale (s) des pistes dont la durée est connue.
    #[must_use]
    pub fn total_s(&self) -> f64 {
        self.tracks.iter().filter_map(|t| t.duration_s).sum()
    }

    /// Durée restante (s) à partir de `position_s` dans le titre courant : le reste de ce titre puis tous les titres
    /// qui le suivent dans la liste (l'ordre réel dépend du mode de lecture ; c'est une estimation).
    #[must_use]
    pub fn remaining_s(&self, position_s: f64) -> f64 {
        let Some(current) = self.tracks.get(self.index) else {
            return 0.0;
        };
        let rest_of_current = (current.duration_s.unwrap_or(0.0) - position_s).max(0.0);
        let after: f64 = self.tracks[self.index + 1..].iter().filter_map(|t| t.duration_s).sum();
        rest_of_current + after
    }

    /// Reconstruit la liste et ré-indexe le titre courant et l'historique avec `map` (ancien → nouvel indice, `None`
    /// si la piste a disparu).
    fn rebuild(&mut self, list: Vec<Track>, map: impl Fn(usize) -> Option<usize>) {
        self.index = map(self.index).unwrap_or(self.index);
        self.history = self.history.iter().filter_map(|&i| map(i)).collect();
        self.tracks = Arc::from(list);
        self.index = self.index.min(self.tracks.len().saturating_sub(1));
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use proptest::prelude::*;

    use super::super::{PlayMode, Playlist, Step, Track};

    fn track(i: usize, duration: Option<f64>) -> Track {
        Track {
            title: format!("T{i}"),
            artist: String::new(),
            path: PathBuf::from(format!("{i}.mp3")),
            local: false,
            duration_s: duration,
            error: None,
        }
    }

    fn list(n: usize) -> Playlist {
        Playlist::new((0..n).map(|i| track(i, Some(10.0))).collect(), Some(0), 1)
    }

    fn titles(list: &Playlist) -> Vec<String> {
        list.tracks().iter().map(|t| t.title.clone()).collect()
    }

    #[test]
    fn deplacer_conserve_les_pistes_et_suit_le_titre_courant() {
        let mut p = list(4);
        p.select(1);
        p.move_track(1, 3);
        assert_eq!(titles(&p), ["T0", "T2", "T3", "T1"]);
        assert_eq!(p.current().map(|t| t.title.as_str()), Some("T1"));
        assert_eq!(p.index(), Some(3));
        p.move_track(3, 0);
        assert_eq!(titles(&p), ["T1", "T0", "T2", "T3"]);
        assert_eq!(p.current().map(|t| t.title.as_str()), Some("T1"));
    }

    #[test]
    fn deplacer_une_autre_piste_ne_change_pas_le_titre_courant() {
        let mut p = list(4);
        p.select(2);
        p.move_track(0, 3);
        assert_eq!(p.current().map(|t| t.title.as_str()), Some("T2"));
    }

    #[test]
    fn retirer_le_titre_courant_charge_le_suivant() {
        let mut p = list(3);
        p.select(1);
        let removed = p.remove(1);
        assert_eq!(removed.map(|t| t.title), Some("T1".into()));
        assert_eq!(p.current().map(|t| t.title.as_str()), Some("T2"));
        let last = p.remove(1);
        assert!(last.is_some());
        assert_eq!(p.current().map(|t| t.title.as_str()), Some("T0"));
    }

    #[test]
    fn retirer_un_titre_avant_le_courant_decale_l_indice() {
        let mut p = list(3);
        p.select(2);
        p.remove(0);
        assert_eq!(p.index(), Some(1));
        assert_eq!(p.current().map(|t| t.title.as_str()), Some("T2"));
    }

    #[test]
    fn retirer_le_dernier_titre_laisse_une_liste_vide_sans_panique() {
        let mut p = list(1);
        assert!(p.remove(0).is_some());
        assert_eq!(p.index(), None);
        assert_eq!(p.remove(0), None);
        p.move_track(0, 1);
        assert_eq!(p.next_track(), None);
    }

    #[test]
    fn ajouter_a_une_liste_vide_donne_un_titre_courant() {
        let mut p = Playlist::new(Vec::new(), None, 1);
        p.add(track(7, None));
        assert_eq!(p.index(), Some(0));
        assert_eq!(p.select(0), Some(Step::Load(0)));
    }

    #[test]
    fn total_est_la_somme_des_durees_connues() {
        let mut p = Playlist::new(
            vec![track(0, Some(10.0)), track(1, None), track(2, Some(5.5))],
            Some(0),
            1,
        );
        assert!((p.total_s() - 15.5).abs() < 1e-9);
        p.add(track(3, Some(4.5)));
        assert!((p.total_s() - 20.0).abs() < 1e-9);
    }

    #[test]
    fn restant_compte_le_reste_du_titre_courant() {
        let mut p = list(3);
        p.set_mode(PlayMode::Sequence);
        assert!((p.remaining_s(4.0) - 26.0).abs() < 1e-9);
        p.select(2);
        assert!((p.remaining_s(4.0) - 6.0).abs() < 1e-9);
        assert!((p.remaining_s(99.0)).abs() < 1e-9, "jamais négatif");
    }

    #[test]
    fn un_fichier_illisible_reste_dans_la_liste_avec_sa_raison() {
        let mut bad = track(0, None);
        bad.error = Some("format non géré".into());
        let p = Playlist::new(vec![bad, track(1, Some(3.0))], Some(0), 1);
        assert_eq!(p.tracks()[0].error.as_deref(), Some("format non géré"));
        assert!((p.total_s() - 3.0).abs() < 1e-9);
    }

    #[test]
    fn remplacer_garde_le_titre_courant_par_son_chemin() {
        let mut p = list(4);
        p.select(2);
        p.replace_tracks(vec![track(3, None), track(2, None), track(9, None)]);
        assert_eq!(p.current().map(|t| t.title.as_str()), Some("T2"));
        p.replace_tracks(vec![track(5, None)]);
        assert_eq!(p.index(), Some(0));
    }

    proptest! {
        #[test]
        fn deplacer_ne_perd_ni_ne_duplique_aucune_piste(
            n in 1_usize..12,
            moves in prop::collection::vec((0_usize..14, 0_usize..14), 0..30),
            start in 0_usize..12,
        ) {
            let mut p = list(n);
            p.select(start.min(n - 1).cast_signed());
            let current = p.current().map(|t| t.title.clone());
            for (from, to) in moves {
                p.move_track(from, to);
            }
            let mut sorted = titles(&p);
            sorted.sort();
            let mut expected: Vec<String> = (0..n).map(|i| format!("T{i}")).collect();
            expected.sort();
            prop_assert_eq!(sorted, expected);
            prop_assert_eq!(p.current().map(|t| t.title.clone()), current);
        }
    }
}
