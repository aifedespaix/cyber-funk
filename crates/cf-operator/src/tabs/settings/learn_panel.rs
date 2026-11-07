//! MIDI learn et profil de l'opérateur (J8-04) : la liste des actions, leurs bindings, « apprendre », import / export.
//!
//! Lecture : `ControlsStatus` (écoute en cours, profil, remarque du dernier apprentissage). Écriture : messages
//! [`LearnCommand`], exécutés par `cf-render`. Deux clics et un appui : « Apprendre » en face de GO, puis le pad.
//!
//! Un profil exporté dans `assets/controllers/<id>.ron` devient un profil livré : l'événement et les créneaux
//! peuvent le désigner (éditeur de la Préparation), et il est relu au prochain lancement.

use std::path::PathBuf;

use bevy_egui::egui;
use cf_control::{ActionTemplate, ControlProfile, InputSource, Layer, MACRO_COUNT, MacroIndex, Mode};
use cf_render::{ControlsStatus, LearnCommand};

/// Une action que l'on peut apprendre, avec son libellé.
struct Entry {
    template: ActionTemplate,
    label: String,
}

/// Les actions proposées, groupées par famille (l'ordre est celui de l'écran).
fn catalog() -> Vec<(&'static str, Vec<Entry>)> {
    let entry = |template: ActionTemplate, label: &str| Entry {
        template,
        label: label.to_owned(),
    };
    let macros = (0..MACRO_COUNT)
        .map(|i| Entry {
            template: ActionTemplate::Macro(MacroIndex(i)),
            label: format!("Macro {}", i + 1),
        })
        .collect();
    vec![
        (
            "Conduite",
            vec![
                entry(ActionTemplate::Go, "GO"),
                entry(ActionTemplate::Back, "BACK"),
                entry(ActionTemplate::Hold, "HOLD"),
                entry(ActionTemplate::NextDj, "DJ suivant"),
                entry(ActionTemplate::PrevDj, "DJ précédent"),
            ],
        ),
        (
            "Pads",
            vec![
                entry(ActionTemplate::ForceDrop, "Drop forcé"),
                entry(ActionTemplate::SkipShot, "Plan suivant"),
                entry(ActionTemplate::ShowDj, "Enseigne du DJ"),
                entry(ActionTemplate::RandomEvent, "Événement au hasard"),
                entry(ActionTemplate::Ascension, "Ascension"),
            ],
        ),
        ("Macros", macros),
        (
            "Niveaux",
            vec![
                entry(ActionTemplate::Master, "Master"),
                entry(ActionTemplate::LayerFade(Layer::Background), "Fondu : arrière-plan"),
                entry(ActionTemplate::LayerFade(Layer::World), "Fondu : monde"),
                entry(ActionTemplate::LayerFade(Layer::Foreground), "Fondu : avant-plan"),
                entry(ActionTemplate::LayerFade(Layer::Overlay), "Fondu : surcouche"),
            ],
        ),
        (
            "Sécurité",
            vec![
                entry(ActionTemplate::Blackout, "Blackout"),
                entry(ActionTemplate::SafeLook, "Look calme"),
                entry(ActionTemplate::FlashKill, "Flashs coupés"),
                entry(ActionTemplate::Breathing, "Respiration"),
            ],
        ),
        (
            "Tempo",
            vec![
                entry(ActionTemplate::TapTempo, "Tap tempo"),
                entry(ActionTemplate::Resync, "Resynchroniser"),
                entry(ActionTemplate::NudgePhase { beats: 0.25 }, "Phase +¼ temps"),
                entry(ActionTemplate::NudgePhase { beats: -0.25 }, "Phase −¼ temps"),
            ],
        ),
        (
            "Application",
            vec![entry(ActionTemplate::Playback, "Lecture de la démo")],
        ),
    ]
}

/// Le geste d'une source, lisible (« note 36, canal 1 »).
pub(crate) fn source_text(source: &InputSource) -> String {
    let port = |port: &Option<String>| port.as_deref().map_or_else(String::new, |p| format!(" · {p}"));
    match source {
        InputSource::MidiNote { port: p, channel, note } => format!("note {note}, canal {}{}", channel + 1, port(p)),
        InputSource::MidiCc {
            port: p, channel, cc, ..
        } => format!("CC {cc}, canal {}{}", channel + 1, port(p)),
        InputSource::MidiCc14 { port: p, channel, cc } => {
            format!("CC 14 bits {cc}, canal {}{}", channel + 1, port(p))
        }
        InputSource::MidiProgram {
            port: p,
            channel,
            program,
        } => format!("programme {program}, canal {}{}", channel + 1, port(p)),
        other => format!("{other:?}"),
    }
}

fn mode_text(mode: Mode) -> &'static str {
    match mode {
        Mode::Momentary => "momentané",
        Mode::Toggle => "bascule",
        Mode::Absolute => "position",
        Mode::Relative => "relatif",
    }
}

/// Où s'écrit un export quand l'opérateur ne précise rien.
fn default_export_path() -> String {
    "assets/controllers/mes-controles.ron".to_owned()
}

/// Dessine la section. Ne touche pas à Bevy : se teste avec un `egui::Context` seul.
pub(crate) fn draw(ui: &mut egui::Ui, status: &ControlsStatus, orders: &mut Vec<LearnCommand>) {
    ui.strong("MIDI learn · profil « Mes contrôles »");
    ui.weak("Choisis « Apprendre » en face d'une action, puis touche le pad ou le potard. Enregistré à chaque geste.");
    if let Some(template) = &status.learning {
        ui.horizontal(|ui| {
            ui.colored_label(
                egui::Color32::YELLOW,
                format!("● En écoute pour {} : touche un pad, un potard…", label_of(template)),
            );
            if ui.button("Annuler").clicked() {
                orders.push(LearnCommand::Cancel);
            }
        });
    }
    if let Some(notice) = &status.learn_notice {
        ui.label(notice);
    }
    for (group, entries) in catalog() {
        egui::CollapsingHeader::new(group)
            .default_open(group == "Conduite")
            .show(ui, |ui| {
                for entry in entries {
                    row(ui, &entry, status, orders);
                }
            });
    }
    ui.separator();
    transfer(ui, status, orders);
}

fn label_of(template: &ActionTemplate) -> String {
    catalog()
        .into_iter()
        .flat_map(|(_, entries)| entries)
        .find(|e| e.template == *template)
        .map_or_else(|| format!("{template:?}"), |e| e.label)
}

fn row(ui: &mut egui::Ui, entry: &Entry, status: &ControlsStatus, orders: &mut Vec<LearnCommand>) {
    ui.horizontal(|ui| {
        ui.add_sized([170.0, 22.0], egui::Label::new(&entry.label));
        let learning_this = status.learning.as_ref() == Some(&entry.template);
        let button = egui::Button::new(if learning_this { "…" } else { "Apprendre" });
        if ui.add_enabled(!learning_this, button).clicked() {
            orders.push(LearnCommand::Start(entry.template.clone()));
        }
        let bindings = status
            .user_profile
            .bindings
            .iter()
            .enumerate()
            .filter(|(_, b)| b.action == entry.template);
        for (index, binding) in bindings {
            ui.label(format!(
                "{} ({})",
                source_text(&binding.source),
                mode_text(binding.mode)
            ));
            if ui.small_button("✕").on_hover_text("Retirer ce binding").clicked() {
                orders.push(LearnCommand::Remove(index));
            }
        }
    });
}

/// Import et export du profil de l'opérateur (fichier RON).
fn transfer(ui: &mut egui::Ui, status: &ControlsStatus, orders: &mut Vec<LearnCommand>) {
    let id = ui.id().with("learn_path");
    let mut path: String = ui.data_mut(|d| d.get_temp(id)).unwrap_or_else(default_export_path);
    let message_id = id.with("message");
    let mut message: String = ui.data_mut(|d| d.get_temp(message_id)).unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        ui.label("Fichier");
        ui.add(egui::TextEdit::singleline(&mut path).desired_width(320.0));
        if ui.button("Exporter").clicked() {
            message = export(&status.user_profile, &path);
        }
        if ui.button("Importer").clicked() {
            match import(&path) {
                Ok(profile) => {
                    orders.push(LearnCommand::Import(profile));
                    message.clear();
                }
                Err(error) => message = error,
            }
        }
    });
    if !message.is_empty() {
        ui.colored_label(egui::Color32::LIGHT_RED, &message);
    }
    ui.data_mut(|d| {
        d.insert_temp(id, path);
        d.insert_temp(message_id, message);
    });
}

/// Écrit le profil dans `path` (sous un autre nom que « Mes contrôles » : le nom du fichier fait l'identifiant).
/// Rend le message d'erreur, ou un message vide.
fn export(profile: &ControlProfile, path: &str) -> String {
    let path = PathBuf::from(path.trim());
    let text = match profile.to_ron() {
        Ok(text) => text,
        Err(error) => return format!("Export impossible : {error}"),
    };
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty())
        && let Err(error) = std::fs::create_dir_all(dir)
    {
        return format!("Export impossible : {error}");
    }
    match cf_show::persist::write_atomic(&path, text.as_bytes()) {
        Ok(()) => String::new(),
        Err(error) => format!("Export impossible : {error}"),
    }
}

/// Lit et valide un profil.
fn import(path: &str) -> Result<ControlProfile, String> {
    let path = PathBuf::from(path.trim());
    let text = std::fs::read_to_string(&path).map_err(|e| format!("Import impossible ({}) : {e}", path.display()))?;
    ControlProfile::from_ron(&text).map_err(|e| format!("Import refusé : {e}"))
}

/// Les bindings d'un profil pour une action (utilisé par les tests).
#[cfg(test)]
fn bindings_for<'a>(profile: &'a ControlProfile, template: &ActionTemplate) -> Vec<&'a cf_control::Binding> {
    profile.bindings.iter().filter(|b| b.action == *template).collect()
}

#[cfg(test)]
mod tests {
    use cf_control::Binding;

    use super::*;

    fn run(status: &ControlsStatus) -> Vec<LearnCommand> {
        let ctx = egui::Context::default();
        let mut orders = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 900.0))),
            ..egui::RawInput::default()
        };
        // Première passe : les polices ne sont prêtes qu'après elle ; la seconde dessine pour de bon.
        for _ in 0..2 {
            orders.clear();
            ctx.run_ui(input.clone(), |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| draw(ui, status, &mut orders));
            })
            .drop_without_applying_deltas();
        }
        orders
    }

    #[test]
    fn le_catalogue_couvre_les_actions_de_la_vue_live() {
        let all: Vec<_> = catalog().into_iter().flat_map(|(_, e)| e).map(|e| e.template).collect();
        for needed in [
            ActionTemplate::Go,
            ActionTemplate::Back,
            ActionTemplate::Hold,
            ActionTemplate::Blackout,
            ActionTemplate::SafeLook,
            ActionTemplate::FlashKill,
            ActionTemplate::TapTempo,
        ] {
            assert!(all.contains(&needed), "{needed:?}");
        }
        assert!(all.contains(&ActionTemplate::Macro(MacroIndex(MACRO_COUNT - 1))));
    }

    #[test]
    fn chaque_action_proposee_accepte_son_mode_par_defaut() {
        for (_, entries) in catalog() {
            for entry in entries {
                let kind = entry.template.kind();
                assert!(kind.accepts(kind.default_mode()), "{}", entry.label);
            }
        }
    }

    #[test]
    fn les_sources_midi_se_lisent_en_clair() {
        let note = InputSource::MidiNote {
            port: Some("DDJ".to_owned()),
            channel: 0,
            note: 36,
        };
        assert_eq!(source_text(&note), "note 36, canal 1 · DDJ");
    }

    #[test]
    fn exporter_puis_importer_restitue_le_profil() {
        let dir = tempfile::tempdir().expect("dossier");
        let path = dir.path().join("sous/mes-controles.ron").display().to_string();
        let mut profile = ControlProfile::new("Mes contrôles");
        profile.bindings.push(Binding::new(
            InputSource::MidiNote {
                port: None,
                channel: 0,
                note: 36,
            },
            ActionTemplate::Go,
        ));
        assert_eq!(export(&profile, &path), "");
        let back = import(&path).expect("relecture");
        assert_eq!(bindings_for(&back, &ActionTemplate::Go).len(), 1);
        assert!(import("/n/existe/pas.ron").is_err());
    }

    #[test]
    fn sans_clic_aucun_ordre_meme_en_ecoute() {
        let status = ControlsStatus {
            learning: Some(ActionTemplate::Go),
            ..ControlsStatus::default()
        };
        assert_eq!(run(&status), [] as [LearnCommand; 0]);
    }
}
