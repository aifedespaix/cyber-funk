//! Le profil de l'opérateur (J8-04) : les bindings appris au MIDI learn, empilés au-dessus de tous les autres.
//!
//! ```text
//! interface ──LearnCommand──▶ handle_commands ──▶ ControlBridge.user (profil) ──▶ rebuild ──▶ Mapper
//!                                                       │                              └─▶ fichier RON
//! flux MIDI ──Dispatch::Learned──▶ record_learned ──────┘
//! ```
//!
//! Le profil vit dans un fichier (`ControlsConfig::user_profile_path`) réécrit atomiquement à chaque changement :
//! il survit donc à un redémarrage. Il est empilé en dernier : à geste égal, ce que l'opérateur a appris gagne sur le
//! clavier par défaut, l'événement et le créneau ; ce que ça masque est dit dans [`ControlsStatus::learn_notice`].

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use cf_control::{ActionTemplate, Binding, ControlProfile, Override};
use cf_show::persist::write_atomic;

use super::bridge::{ControlBridge, Emitter};
use super::{ControlAction, ControlsStatus};
use crate::show::ShowControls;

/// Nom du profil de l'opérateur dans l'interface et dans le fichier.
pub const USER_PROFILE_NAME: &str = "Mes contrôles";

/// Ce que l'interface demande aux contrôles.
#[derive(Message, Debug, Clone, PartialEq)]
pub enum LearnCommand {
    /// Écouter le prochain geste MIDI pour cette action (remplace une écoute en cours).
    Start(ActionTemplate),
    /// Abandonner l'écoute.
    Cancel,
    /// Retirer la ligne de ce rang du profil de l'opérateur.
    Remove(usize),
    /// Remplacer le profil de l'opérateur par celui-ci (import). Refusé s'il est invalide.
    Import(ControlProfile),
}

/// Le profil de l'opérateur et son fichier.
#[derive(Debug)]
pub(super) struct UserControls {
    pub profile: ControlProfile,
    path: Option<PathBuf>,
    /// La table du mapper n'est plus à jour : à reconstruire (et à enregistrer).
    pub dirty: bool,
}

impl UserControls {
    /// Lit le profil du fichier s'il existe. Une remarque en clair si le fichier est illisible : le profil reste alors
    /// vide et le fichier n'est pas touché tant que l'opérateur ne change rien.
    pub fn load(path: Option<&Path>) -> (Self, Option<String>) {
        let mut note = None;
        let profile = match path.map(|p| (p, std::fs::read_to_string(p))) {
            Some((p, Ok(text))) => ControlProfile::from_ron(&text).unwrap_or_else(|error| {
                note = Some(format!("profil de l'opérateur illisible ({}) : {error}", p.display()));
                ControlProfile::new(USER_PROFILE_NAME)
            }),
            _ => ControlProfile::new(USER_PROFILE_NAME),
        };
        let this = Self {
            profile,
            path: path.map(Path::to_path_buf),
            dirty: false,
        };
        (this, note)
    }

    /// Enregistre le profil (atomique). Sans chemin configuré, ne fait rien.
    fn save(&self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("dossier {} : {e}", dir.display()))?;
        }
        let text = self.profile.to_ron().map_err(|e| e.to_string())?;
        write_atomic(path, text.as_bytes()).map_err(|e| e.to_string())
    }

    /// Pose un binding appris. Rend les lignes de l'opérateur qu'il remplace ; si le profil devenait invalide, il est
    /// laissé tel quel et l'erreur est rendue.
    pub fn learn(&mut self, binding: Binding) -> Result<Vec<Binding>, String> {
        let before = self.profile.clone();
        let replaced = self.profile.set_binding(binding);
        if let Err(error) = self.profile.validate() {
            self.profile = before;
            return Err(error.to_string());
        }
        self.dirty = true;
        Ok(replaced)
    }
}

/// Texte court d'un binding, pour les remarques.
fn describe(binding: &Binding) -> String {
    format!("{:?} → {:?}", binding.source, binding.action)
}

/// Les remarques qui concernent le binding appris : lignes de l'opérateur remplacées, bindings d'autres profils masqués.
fn notice(learned: &Binding, replaced: &[Binding], overrides: &[Override]) -> String {
    let mut text = format!("Appris : {}", describe(learned));
    for old in replaced {
        let _ = write!(text, " · remplace {:?}", old.action);
    }
    for o in overrides
        .iter()
        .filter(|o| o.by == learned.action && o.source.overlaps(&learned.source))
    {
        let _ = write!(text, " · masque {:?} du profil « {} »", o.replaced, o.replaced_profile);
    }
    text
}

/// Exécute les demandes de l'interface et fait avancer l'écoute (la moitié d'un CC 14 bits se conclut avec le temps).
pub(super) fn handle_commands(
    mut commands: MessageReader<LearnCommand>,
    mut bridge: ResMut<ControlBridge>,
    mut status: ResMut<ControlsStatus>,
) {
    for command in commands.read() {
        match command {
            LearnCommand::Start(template) => {
                bridge.learner.start_learn(template.clone());
                status.learning = Some(template.clone());
                status.learn_notice = None;
            }
            LearnCommand::Cancel => {
                bridge.learner.cancel();
                status.learning = None;
            }
            LearnCommand::Remove(index) => {
                if *index < bridge.user.profile.bindings.len() {
                    bridge.user.profile.bindings.remove(*index);
                    bridge.user.dirty = true;
                    status.learn_notice = None;
                }
            }
            LearnCommand::Import(profile) => match profile.validate() {
                Ok(()) => {
                    bridge.user.profile = ControlProfile {
                        name: USER_PROFILE_NAME.to_owned(),
                        bindings: profile.bindings.clone(),
                    };
                    bridge.user.dirty = true;
                    status.learn_notice = Some(format!("Profil importé : {} ligne(s)", profile.bindings.len()));
                }
                Err(error) => status.learn_notice = Some(format!("Import refusé : {error}")),
            },
        }
    }
    let now = bridge.now_s();
    if let Some(binding) = bridge.learner.poll(now) {
        record_learned(&mut bridge, &mut status, binding);
    }
}

/// Range un binding appris dans le profil de l'opérateur.
pub(super) fn record_learned(bridge: &mut ControlBridge, status: &mut ControlsStatus, binding: Binding) {
    status.learning = None;
    match bridge.user.learn(binding.clone()) {
        Ok(replaced) => {
            // Le message complet (avec ce que ça masque) est complété par `rebuild`, qui connaît les remplacements.
            status.learn_notice = Some(notice(&binding, &replaced, &[]));
            bridge.last_learned = Some((binding, replaced));
        }
        Err(error) => status.learn_notice = Some(format!("Geste refusé : {error}")),
    }
}

/// Reconstruit la table du mapper et enregistre le profil quand il a changé.
pub(super) fn rebuild(
    mut bridge: ResMut<ControlBridge>,
    mut status: ResMut<ControlsStatus>,
    controls: Option<Res<ShowControls>>,
    mut out: MessageWriter<ControlAction>,
) {
    if !bridge.user.dirty {
        return;
    }
    bridge.user.dirty = false;
    let now = bridge.now_s();
    let slot = bridge.slot;
    let mut emitter = Emitter {
        bridge: &mut bridge,
        status: &mut status,
        out: &mut out,
    };
    let mut released = Vec::new();
    let result = super::profiles::reload(&mut emitter, controls.as_deref(), slot, now, &mut released);
    for action in released {
        emitter.emit(action);
    }
    match result {
        Ok(overrides) => {
            if let Some((learned, replaced)) = emitter.bridge.last_learned.take() {
                emitter.status.learn_notice = Some(notice(&learned, &replaced, &overrides));
            }
        }
        Err(note) => {
            tracing::warn!(%note, "contrôles : profil de l'opérateur refusé, table précédente conservée");
            emitter.status.learn_notice = Some(note);
        }
    }
    if let Err(error) = emitter.bridge.user.save() {
        tracing::error!(%error, "contrôles : profil de l'opérateur non enregistré");
        emitter.status.learn_notice = Some(format!("Enregistrement impossible : {error}"));
    }
    emitter.status.user_profile = emitter.bridge.user.profile.clone();
}

#[cfg(test)]
mod tests {
    use cf_control::{InputSource, Mode};

    use super::*;

    fn note(note: u8) -> InputSource {
        InputSource::MidiNote {
            port: None,
            channel: 0,
            note,
        }
    }

    #[test]
    fn un_profil_enregistre_se_relit() {
        let dir = tempfile::tempdir().expect("dossier");
        let path = dir.path().join("sous/dossier/user.ron");
        let (mut user, note_a) = UserControls::load(Some(&path));
        assert!(note_a.is_none());
        user.learn(Binding::new(note(36), ActionTemplate::Go))
            .expect("binding valide");
        user.save().expect("écriture");
        let (again, note_b) = UserControls::load(Some(&path));
        assert!(note_b.is_none());
        assert_eq!(again.profile, user.profile);
        assert_eq!(again.profile.bindings[0].mode, Mode::Momentary);
    }

    #[test]
    fn apprendre_le_meme_geste_remplace_la_ligne() {
        let (mut user, _) = UserControls::load(None);
        user.learn(Binding::new(note(36), ActionTemplate::Go)).expect("premier");
        let replaced = user
            .learn(Binding::new(note(36), ActionTemplate::Back))
            .expect("second");
        assert_eq!(replaced.len(), 1);
        assert_eq!(user.profile.bindings.len(), 1);
        assert_eq!(user.profile.bindings[0].action, ActionTemplate::Back);
    }

    #[test]
    fn un_binding_invalide_est_refuse_et_le_profil_garde_sa_forme() {
        let (mut user, _) = UserControls::load(None);
        user.learn(Binding::new(note(36), ActionTemplate::Go)).expect("premier");
        let bad = Binding {
            source: note(37),
            action: ActionTemplate::Go,
            mode: Mode::Toggle,
        };
        assert!(user.learn(bad).is_err());
        assert_eq!(user.profile.bindings.len(), 1);
    }

    #[test]
    fn un_fichier_illisible_est_signale_sans_etre_ecrase() {
        let dir = tempfile::tempdir().expect("dossier");
        let path = dir.path().join("user.ron");
        std::fs::write(&path, "pas du ron").expect("écriture");
        let (user, note) = UserControls::load(Some(&path));
        assert!(note.is_some());
        assert_eq!(user.profile.bindings, []);
        assert_eq!(std::fs::read_to_string(&path).expect("relecture"), "pas du ron");
    }
}
