//! Historique annuler / rétablir par instantanés.
//!
//! Un événement pèse quelques dizaines de Ko et ne s'édite qu'en Préparation, hors de toute boucle chaude : copier
//! l'état complet à chaque édition est plus sûr (aucune opération inverse à écrire, donc aucune à oublier) que de
//! journaliser des différences.

/// Nombre maximal d'étapes d'annulation conservées.
pub(crate) const MAX_STEPS: usize = 200;

/// Pile d'annulation et de rétablissement d'un état `T`.
#[derive(Debug, Clone)]
pub(crate) struct History<T> {
    undo: Vec<T>,
    redo: Vec<T>,
    /// Clé de la dernière édition enregistrée : une suite d'éditions de la même clé (frappe dans un champ, glissé
    /// d'un curseur) ne compte que pour une étape.
    last_key: Option<u64>,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            last_key: None,
        }
    }
}

impl<T> History<T> {
    /// Enregistre l'état `before`, celui d'avant l'édition. `key` regroupe les éditions successives d'un même champ ;
    /// `None` ouvre toujours une étape. Toute édition vide la pile de rétablissement.
    pub(crate) fn record(&mut self, before: T, key: Option<u64>) {
        self.redo.clear();
        if key.is_some() && key == self.last_key {
            return;
        }
        self.last_key = key;
        if self.undo.len() == MAX_STEPS {
            self.undo.remove(0);
        }
        self.undo.push(before);
    }

    /// Referme le regroupement en cours : la prochaine édition ouvrira une nouvelle étape.
    pub(crate) fn break_group(&mut self) {
        self.last_key = None;
    }

    /// Annule : rend l'état précédent et garde `current` pour le rétablir.
    pub(crate) fn undo(&mut self, current: T) -> Option<T> {
        self.last_key = None;
        let previous = self.undo.pop()?;
        self.redo.push(current);
        Some(previous)
    }

    /// Rétablit : rend l'état annulé et garde `current` pour annuler à nouveau.
    pub(crate) fn redo(&mut self, current: T) -> Option<T> {
        self.last_key = None;
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }

    /// Y a-t-il quelque chose à annuler ?
    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Y a-t-il quelque chose à rétablir ?
    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Oublie tout (ouverture d'un autre fichier).
    pub(crate) fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.last_key = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annuler_puis_retablir_restitue_les_etats() {
        let mut h = History::default();
        h.record(1, None);
        h.record(2, None);
        assert_eq!(h.undo(3), Some(2));
        assert_eq!(h.undo(2), Some(1));
        assert_eq!(h.undo(1), None);
        assert_eq!(h.redo(1), Some(2));
        assert_eq!(h.redo(2), Some(3));
        assert_eq!(h.redo(3), None);
    }

    #[test]
    fn une_edition_vide_le_retablissement() {
        let mut h = History::default();
        h.record(1, None);
        assert_eq!(h.undo(2), Some(1));
        assert!(h.can_redo());
        h.record(1, None);
        assert!(!h.can_redo());
    }

    #[test]
    fn les_editions_de_meme_cle_ne_font_qu_une_etape() {
        let mut h = History::default();
        h.record("a", Some(7));
        h.record("ab", Some(7));
        h.record("abc", Some(7));
        assert_eq!(h.undo("abcd"), Some("a"));
        assert!(!h.can_undo());
    }

    #[test]
    fn fermer_le_groupe_ouvre_une_nouvelle_etape() {
        let mut h = History::default();
        h.record(1, Some(7));
        h.break_group();
        h.record(2, Some(7));
        assert_eq!(h.undo(3), Some(2));
        assert_eq!(h.undo(2), Some(1));
    }

    #[test]
    fn la_pile_est_bornee() {
        let mut h = History::default();
        for i in 0..MAX_STEPS + 10 {
            h.record(i, None);
        }
        assert_eq!(h.undo.len(), MAX_STEPS);
    }
}
