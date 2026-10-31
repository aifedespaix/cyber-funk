//! La session de préparation : l'événement en cours d'édition, son historique, son état « modifié ».
//!
//! L'édition travaille sur une copie de l'événement (`EventDocument`), jamais sur le show en train de jouer : un
//! show ne se recharge qu'à l'ouverture (règle 8). Les fichiers enregistrés se rouvrent avec `--show`.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use cf_show::load::LookLibrary;
use cf_show::model::{Event, LookId};
use cf_show::persist::EventDocument;
use cf_show::validate::{Issue, validate_event};

use super::edit;
use super::history::History;

/// Où chercher les looks et quel fichier ouvrir au démarrage. Posé par le binaire avant `OperatorPlugin` ; sans lui,
/// le dossier `assets` du répertoire courant et un événement neuf.
#[derive(Resource, Debug, Clone)]
pub struct PrepConfig {
    /// Dossier des assets (ses looks sont sous `looks/`).
    pub assets_dir: PathBuf,
    /// Événement à ouvrir au démarrage (le `--show` de la ligne de commande).
    pub show_path: Option<PathBuf>,
}

impl Default for PrepConfig {
    fn default() -> Self {
        Self {
            assets_dir: PathBuf::from("assets"),
            show_path: None,
        }
    }
}

/// Ce qui est sélectionné dans les éditeurs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Selection {
    /// Rang du créneau.
    pub slot: usize,
    /// Rang du cue ; `None` : le créneau lui-même.
    pub cue: Option<usize>,
    /// Les réglages de l'événement sont affichés à la place du créneau.
    pub event: bool,
}

/// Le dernier résultat d'une action de fichier, affiché dans la barre d'état.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Status {
    /// Rien à dire.
    Quiet,
    /// Réussite.
    Info(String),
    /// Échec, avec la raison.
    Error(String),
}

/// L'événement en cours d'édition et tout ce qui l'accompagne.
#[derive(Resource)]
pub(crate) struct PrepSession {
    doc: EventDocument,
    /// L'événement tel qu'ouvert ou enregistré : « modifié » = différent de lui.
    baseline: Event,
    history: History<Event>,
    library: LookLibrary,
    issues: Vec<Issue>,
    pub selection: Selection,
    pub status: Status,
    /// Champ « chemin du fichier » de la barre.
    pub path_input: String,
}

impl PrepSession {
    /// Session de départ : le fichier de la configuration s'il y en a un, sinon un événement neuf.
    pub(crate) fn start(config: &PrepConfig) -> Option<Self> {
        let (library, library_error) = match LookLibrary::load_dir(&config.assets_dir.join("looks")) {
            Ok(library) => (library, None),
            Err(e) => (LookLibrary::new(), Some(format!("looks illisibles : {e}"))),
        };
        let mut session = Self::blank(library)?;
        if let Some(path) = &config.show_path {
            session.open(path);
        }
        if let Some(message) = library_error {
            session.status = Status::Error(message);
        }
        Some(session)
    }

    fn blank(library: LookLibrary) -> Option<Self> {
        let safe_look = match library.iter().next() {
            Some((id, _)) => id.clone(),
            None => LookId::new("calme").ok()?,
        };
        let event = edit::new_event(safe_look)?;
        let mut session = Self {
            baseline: event.clone(),
            doc: EventDocument::new(event),
            history: History::default(),
            library,
            issues: Vec::new(),
            selection: Selection::default(),
            status: Status::Quiet,
            path_input: String::new(),
        };
        session.revalidate();
        Some(session)
    }

    /// L'événement édité.
    pub(crate) fn event(&self) -> &Event {
        self.doc.event()
    }

    /// Les problèmes de l'événement tel qu'il est maintenant (vide : valide).
    pub(crate) fn issues(&self) -> &[Issue] {
        &self.issues
    }

    /// L'événement diffère-t-il de la version ouverte ou enregistrée en dernier ?
    pub(crate) fn is_dirty(&self) -> bool {
        *self.doc.event() != self.baseline
    }

    /// Les looks que l'on peut choisir : bibliothèque et looks embarqués dans l'événement.
    pub(crate) fn look_ids(&self) -> Vec<LookId> {
        let mut ids: Vec<LookId> = self.library.iter().map(|(id, _)| id.clone()).collect();
        for look in &self.doc.event().looks {
            if !ids.contains(&look.id) {
                ids.push(look.id.clone());
            }
        }
        ids.sort();
        ids
    }

    /// Remplace l'événement par `edited` s'il diffère, en gardant l'ancien pour annuler. `key` regroupe les éditions
    /// successives d'un même champ en une seule étape.
    pub(crate) fn commit(&mut self, edited: Event, key: Option<u64>) {
        if edited == *self.doc.event() {
            return;
        }
        self.history.record(self.doc.event().clone(), key);
        *self.doc.event_mut() = edited;
        self.after_change();
    }

    /// Referme le regroupement d'éditions en cours (le champ n'est plus manipulé).
    pub(crate) fn break_group(&mut self) {
        self.history.break_group();
    }

    /// Annule la dernière édition. Renvoie `false` s'il n'y a rien à annuler.
    pub(crate) fn undo(&mut self) -> bool {
        let Some(previous) = self.history.undo(self.doc.event().clone()) else {
            return false;
        };
        *self.doc.event_mut() = previous;
        self.after_change();
        true
    }

    /// Rétablit la dernière édition annulée.
    pub(crate) fn redo(&mut self) -> bool {
        let Some(next) = self.history.redo(self.doc.event().clone()) else {
            return false;
        };
        *self.doc.event_mut() = next;
        self.after_change();
        true
    }

    pub(crate) fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub(crate) fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Ouvre un fichier d'événement : l'édition en cours est remplacée (l'appelant a déjà confirmé si elle était
    /// modifiée). Une erreur laisse l'édition intacte.
    pub(crate) fn open(&mut self, path: &Path) {
        match EventDocument::open(path, &self.library) {
            Ok((doc, _)) => {
                self.baseline = doc.event().clone();
                self.doc = doc;
                self.history.clear();
                self.path_input = path.display().to_string();
                self.after_change();
                self.status = Status::Info(format!("{} ouvert", path.display()));
            }
            Err(e) => self.status = Status::Error(format!("ouverture impossible : {e}")),
        }
    }

    /// Enregistre dans le fichier associé ; sans fichier, ne fait rien et renvoie `false` (voir `save_as`).
    pub(crate) fn save(&mut self) -> bool {
        if self.doc.path().is_none() {
            return false;
        }
        let result = self.doc.save();
        self.finish_save(result);
        true
    }

    /// Enregistre sous `path`, qui devient le fichier associé.
    pub(crate) fn save_as(&mut self, path: &Path) {
        let result = self.doc.save_as(path);
        self.path_input = path.display().to_string();
        self.finish_save(result);
    }

    fn finish_save(&mut self, result: Result<(), cf_show::persist::PersistError>) {
        match result {
            Ok(()) => {
                self.baseline = self.doc.event().clone();
                let name = self.doc.path().map(|p| p.display().to_string()).unwrap_or_default();
                self.status = Status::Info(format!("{name} enregistré"));
            }
            Err(e) => self.status = Status::Error(format!("enregistrement impossible : {e}")),
        }
    }

    /// Valide l'événement tel qu'il est maintenant et recadre la sélection dans ses bornes.
    fn after_change(&mut self) {
        self.revalidate();
        let event = self.doc.event();
        self.selection.slot = self.selection.slot.min(event.slots.len().saturating_sub(1));
        let cues = event.slots.get(self.selection.slot).map_or(0, |s| s.cues.len());
        if self.selection.cue.is_some_and(|c| c >= cues) {
            self.selection.cue = cues.checked_sub(1);
        }
    }

    fn revalidate(&mut self) {
        let event = self.doc.event();
        let exists = |id: &LookId| self.library.contains(id) || event.looks.iter().any(|look| look.id == *id);
        self.issues = validate_event(event, &exists);
    }

    /// Le show chargé que produirait cet événement, pour vérifier qu'il se charge vraiment (utilisé par les tests).
    #[cfg(test)]
    pub(crate) fn as_loaded(&self) -> Result<cf_show::load::LoadedShow, cf_show::load::LoadError> {
        cf_show::load::LoadedShow::new(self.doc.event().clone(), &self.library, "test")
    }
}

/// Crée la session au démarrage (lecture de fichiers : permise, le show n'est pas encore en cours).
pub(crate) fn init_session(config: Res<PrepConfig>, mut commands: Commands) {
    if let Some(session) = PrepSession::start(&config) {
        commands.insert_resource(session);
    } else {
        tracing::error!("préparation indisponible : impossible de créer un événement neuf");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets"))
    }

    fn demo_session() -> PrepSession {
        let config = PrepConfig {
            assets_dir: assets(),
            show_path: Some(assets().join("shows/demo.ron")),
        };
        PrepSession::start(&config).expect("session de démonstration")
    }

    #[test]
    fn le_show_de_demonstration_s_ouvre_sans_probleme() {
        let session = demo_session();
        assert_eq!(
            session.status,
            Status::Info(format!("{} ouvert", assets().join("shows/demo.ron").display()))
        );
        assert_eq!(session.issues(), []);
        assert!(!session.is_dirty());
        assert!(session.as_loaded().is_ok());
    }

    #[test]
    fn annuler_et_retablir_couvrent_les_editions() {
        let mut session = demo_session();
        let original = session.event().clone();

        let mut edited = original.clone();
        edited.settings.name = "Autre nom".to_owned();
        session.commit(edited.clone(), None);
        assert!(session.is_dirty());

        let mut structural = edited.clone();
        edit::add_slot(&mut structural);
        session.commit(structural.clone(), None);

        assert!(session.undo());
        assert_eq!(*session.event(), edited);
        assert!(session.undo());
        assert_eq!(*session.event(), original);
        assert!(!session.is_dirty());
        assert!(!session.undo());
        assert!(session.redo());
        assert!(session.redo());
        assert_eq!(*session.event(), structural);
        assert!(!session.redo());
    }

    #[test]
    fn le_fichier_enregistre_se_recharge_a_l_identique() {
        let dir = tempfile::tempdir().expect("dossier temporaire");
        let path = dir.path().join("soiree.cfshow");
        let mut session = demo_session();

        let mut edited = session.event().clone();
        edited.settings.name = "Soirée de test".to_owned();
        edit::add_slot(&mut edited);
        edit::add_cue(&mut edited, 0);
        edit::duplicate_cue(&mut edited, 0, 0);
        session.commit(edited, None);
        assert!(session.is_dirty());

        session.save_as(&path);
        assert!(matches!(session.status, Status::Info(_)), "{:?}", session.status);
        assert!(!session.is_dirty());
        assert!(session.save());

        let mut reopened =
            PrepSession::blank(LookLibrary::load_dir(&assets().join("looks")).expect("looks")).expect("session");
        reopened.open(&path);
        assert!(matches!(reopened.status, Status::Info(_)), "{:?}", reopened.status);
        assert_eq!(reopened.event(), session.event());
    }

    #[test]
    fn un_fichier_invalide_laisse_l_edition_intacte() {
        let mut session = demo_session();
        let before = session.event().clone();
        session.open(Path::new("n-existe-pas.cfshow"));
        assert!(matches!(session.status, Status::Error(_)));
        assert_eq!(*session.event(), before);
    }

    #[test]
    fn une_edition_invalide_est_signalee() {
        let mut session = demo_session();
        let mut edited = session.event().clone();
        edited.settings.latency_offset_ms = 9_999.0;
        session.commit(edited, None);
        assert!(session.issues().iter().any(|i| i.path.contains("latency")));
    }
}
