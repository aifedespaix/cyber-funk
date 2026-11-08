//! Le lecteur (titre en cours, précédent / pause / suivant, progression) et la playlist gérée : défilante,
//! réordonnable au glisser-déposer, avec la durée de chaque titre et la durée totale.
//!
//! Toute modification de la liste part en une seule commande, `AudioCommand::SetPlaylist`, avec la liste entière :
//! le moteur retrouve le titre entendu par son chemin et continue de le jouer.

use bevy_egui::egui;
use cf_audio::playlist::{PlayMode, Track, remaining_s, total_s};
use cf_render::audio::{AudioCommand, AudioStatus};

use crate::format;
use crate::prepare::lists::{ListRow, reorderable};

/// Ce qu'on sait du titre entendu.
#[derive(Debug, Default)]
pub(crate) struct PlaylistUi {
    /// Ligne choisie dans la liste (pas forcément celle qui joue).
    pub selected: Option<usize>,
}

/// Ce que l'opérateur a fait pendant l'image.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct PlaylistOut {
    pub commands: Vec<AudioCommand>,
    /// Clic sur « + Ajouter… » : ouvrir le sélecteur de fichiers.
    pub add_files: bool,
}

/// La liste après avoir déplacé la piste `from` en position `to`.
pub(crate) fn moved(tracks: &[Track], from: usize, to: usize) -> Vec<Track> {
    let mut list = tracks.to_vec();
    if from < list.len() && from != to {
        let track = list.remove(from);
        list.insert(to.min(list.len()), track);
    }
    list
}

/// La liste sans la piste `index`.
pub(crate) fn without(tracks: &[Track], index: usize) -> Vec<Track> {
    let mut list = tracks.to_vec();
    if index < list.len() {
        list.remove(index);
    }
    list
}

/// La liste complétée de `added`, sans doublon (un fichier déjà présent n'est pas ajouté deux fois).
pub(crate) fn with_added(tracks: &[Track], added: Vec<Track>) -> Vec<Track> {
    let mut list = tracks.to_vec();
    for track in added {
        if !list.iter().any(|known| known.path == track.path) {
            list.push(track);
        }
    }
    list
}

/// Texte d'une ligne de la playlist.
pub(crate) fn row_label(index: usize, track: &Track, playing: bool) -> String {
    let marker = if track.error.is_some() {
        "⚠"
    } else if playing {
        "▶"
    } else {
        " "
    };
    let name = if track.artist.is_empty() {
        track.title.clone()
    } else {
        format!("{} — {}", track.artist, track.title)
    };
    let local = if track.local { "  (perso)" } else { "" };
    format!(
        "{marker} {:>2}.  {name}{local}    {}",
        index + 1,
        format::duration(track.duration_s)
    )
}

/// Pied de la playlist : nombre de titres, durée totale, durée restante estimée.
pub(crate) fn footer_text(tracks: &[Track], current: Option<usize>, position_s: f64) -> String {
    let count = tracks.len();
    let plural = if count > 1 { "s" } else { "" };
    format!(
        "{count} titre{plural} · durée totale {} · restant ≈ {}",
        format::duration(Some(total_s(tracks))),
        format::duration(Some(remaining_s(tracks, current, position_s)))
    )
}

/// Le lecteur : titre en cours et commandes. Rend les ordres de l'image.
pub(crate) fn player(ui: &mut egui::Ui, status: &AudioStatus) -> Vec<AudioCommand> {
    let mut commands = Vec::new();
    let demo = status.demo.as_ref();
    let current = demo.and_then(|d| d.track).and_then(|i| status.tracks.get(i));
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.weak("EN LECTURE");
        if let Some(track) = current {
            ui.label(egui::RichText::new(&track.title).size(19.0).strong());
            if !track.artist.is_empty() {
                ui.weak(&track.artist);
            }
        } else {
            ui.label(egui::RichText::new("Aucune piste").size(19.0).strong());
            ui.weak("Ajoutez des fichiers à la playlist.");
        }
        let position = demo.map_or(0.0, |d| d.position_s);
        let length = demo.and_then(|d| d.duration_s);
        let fraction = length
            .filter(|l| *l > 0.0)
            .map_or(0.0, |l| (position / l).clamp(0.0, 1.0));
        ui.add(egui::ProgressBar::new(fraction as f32).text(format!(
            "{} / {}",
            format::duration(Some(position)),
            format::duration(length)
        )));
        let playing = demo.is_some_and(|d| d.playing);
        let has_tracks = !status.tracks.is_empty();
        ui.horizontal(|ui| {
            ui.add_enabled_ui(has_tracks, |ui| {
                if ui.button("⏮").on_hover_text("Précédent (ou début du titre)").clicked() {
                    commands.push(AudioCommand::DemoPrev);
                }
                let (symbol, hint) = if playing { ("⏸", "Pause") } else { ("▶", "Lecture") };
                if ui.button(symbol).on_hover_text(hint).clicked() {
                    commands.push(AudioCommand::DemoToggle);
                }
                if ui.button("⏭").on_hover_text("Suivant").clicked() {
                    commands.push(AudioCommand::DemoNext);
                }
            });
            ui.separator();
            let mode = demo.map_or(PlayMode::Random, |d| d.mode);
            for (target, label, hint) in [
                (PlayMode::Random, "🔀", "Aléatoire"),
                (PlayMode::Sequence, "➡", "Dans l'ordre"),
                (PlayMode::Loop, "🔁", "Répéter le titre"),
            ] {
                if ui
                    .add(egui::Button::selectable(mode == target, label))
                    .on_hover_text(hint)
                    .clicked()
                {
                    commands.push(AudioCommand::DemoMode(target));
                }
            }
        });
        let mut volume = demo.map_or(0.8, |d| d.volume);
        if ui
            .add(
                egui::Slider::new(&mut volume, 0.0..=1.0)
                    .text("Volume")
                    .show_value(false),
            )
            .changed()
        {
            commands.push(AudioCommand::DemoVolume(volume));
        }
    });
    commands
}

/// La playlist : liste défilante, actions sur la ligne choisie, pied de durées.
pub(crate) fn playlist(ui: &mut egui::Ui, status: &AudioStatus, state: &mut PlaylistUi) -> PlaylistOut {
    let mut out = PlaylistOut::default();
    let tracks: &[Track] = &status.tracks;
    let demo = status.demo.as_ref();
    let playing_index = demo.and_then(|d| d.track);
    let position = demo.map_or(0.0, |d| d.position_s);
    if state.selected.is_some_and(|i| i >= tracks.len()) {
        state.selected = None;
    }

    ui.horizontal(|ui| {
        ui.strong("Playlist");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("+ Ajouter…")
                .on_hover_text("Choisir des fichiers audio sur le disque")
                .clicked()
            {
                out.add_files = true;
            }
        });
    });
    let selected = state.selected;
    ui.horizontal(|ui| {
        ui.add_enabled_ui(selected.is_some(), |ui| {
            if ui.button("▶ Lire").clicked()
                && let Some(i) = selected
            {
                out.commands.push(AudioCommand::DemoSelect(i));
            }
            if ui.button("✕ Retirer").clicked()
                && let Some(i) = selected
            {
                out.commands.push(AudioCommand::SetPlaylist(without(tracks, i)));
                state.selected = None;
            }
        });
        ui.weak("Glisser une ligne pour la déplacer · double-clic pour la lire");
    });

    let footer_height = 24.0;
    let list_height = (ui.available_height() - footer_height).max(60.0);
    egui::ScrollArea::vertical()
        .max_height(list_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let rows: Vec<ListRow> = tracks
                .iter()
                .enumerate()
                .map(|(i, track)| ListRow {
                    label: row_label(i, track, playing_index == Some(i)),
                    selected: state.selected == Some(i),
                    flagged: track.error.is_some(),
                })
                .collect();
            let action = reorderable(ui, "playlist_row", &rows, None);
            if let Some(i) = action.select {
                state.selected = Some(i);
            }
            if let Some(i) = action.activate {
                out.commands.push(AudioCommand::DemoSelect(i));
            }
            if let Some((from, to)) = action.moved.filter(|(from, to)| from != to) {
                out.commands.push(AudioCommand::SetPlaylist(moved(tracks, from, to)));
                state.selected = Some(to);
            }
            if tracks.is_empty() {
                ui.weak("La playlist est vide.");
            }
        });
    ui.weak(footer_text(tracks, playing_index, position));
    out
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn track(name: &str, duration: Option<f64>) -> Track {
        Track {
            title: name.to_owned(),
            artist: String::new(),
            path: PathBuf::from(format!("{name}.mp3")),
            local: false,
            duration_s: duration,
            error: None,
        }
    }

    fn titles(tracks: &[Track]) -> Vec<&str> {
        tracks.iter().map(|t| t.title.as_str()).collect()
    }

    #[test]
    fn deplacer_une_ligne_la_met_a_sa_place() {
        let list = [track("A", None), track("B", None), track("C", None)];
        assert_eq!(titles(&moved(&list, 0, 2)), ["B", "C", "A"]);
        assert_eq!(titles(&moved(&list, 2, 0)), ["C", "A", "B"]);
        assert_eq!(titles(&moved(&list, 1, 1)), ["A", "B", "C"]);
        assert_eq!(
            titles(&moved(&list, 9, 0)),
            ["A", "B", "C"],
            "rang inconnu : rien ne bouge"
        );
    }

    #[test]
    fn retirer_une_ligne_inconnue_ne_change_rien() {
        let list = [track("A", None), track("B", None)];
        assert_eq!(titles(&without(&list, 0)), ["B"]);
        assert_eq!(titles(&without(&list, 5)), ["A", "B"]);
    }

    #[test]
    fn ajouter_ne_duplique_pas_un_fichier_deja_present() {
        let list = [track("A", None)];
        let added = with_added(&list, vec![track("A", None), track("B", None)]);
        assert_eq!(titles(&added), ["A", "B"]);
    }

    #[test]
    fn le_pied_donne_total_et_restant() {
        let list = [track("A", Some(60.0)), track("B", Some(120.0)), track("C", Some(30.0))];
        assert_eq!(
            footer_text(&list, Some(0), 20.0),
            "3 titres · durée totale 3:30 · restant ≈ 3:10"
        );
        assert_eq!(
            footer_text(&[], None, 0.0),
            "0 titre · durée totale 0:00 · restant ≈ 0:00"
        );
    }

    #[test]
    fn une_ligne_marque_le_titre_en_cours_et_les_fichiers_illisibles() {
        let mut bad = track("Z", None);
        bad.error = Some("format non géré".into());
        assert!(row_label(0, &track("A", Some(242.0)), true).starts_with('▶'));
        assert!(row_label(0, &track("A", Some(242.0)), true).ends_with("4:02"));
        assert!(row_label(1, &bad, true).starts_with('⚠'));
        assert!(row_label(1, &track("A", None), false).ends_with('—'));
    }
}
