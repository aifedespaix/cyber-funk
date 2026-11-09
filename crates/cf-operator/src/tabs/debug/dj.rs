//! La section « DJ » du dock : voir qui joue, lancer une passation (la nouvelle voiture double l'ancienne, le portique
//! annonce le nouveau DJ) vers le suivant, le précédent ou n'importe quel DJ de la feuille.
//!
//! Une passation exige un spectacle chargé avec plusieurs DJ et un show qui joue : sinon la section le dit en clair plutôt
//! que de laisser un bouton muet. Les gestes deviennent des `RunOrder::Handover`, le même ordre que la colonne des DJ du
//! cockpit.

use bevy_egui::egui;
use cf_render::{Phase, RunPhase, ShowSheet};

use super::Gesture;

/// Ce que la section lit.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DjView<'a> {
    /// La feuille de conduite (absente : aucun spectacle chargé).
    pub sheet: Option<&'a ShowSheet>,
    /// Rang du créneau en cours.
    pub slot: Option<usize>,
    /// DJ visé par la passation en cours.
    pub handover_to: Option<usize>,
    /// Où en est le show.
    pub run: Option<RunPhase>,
}

/// Un geste de la section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DjGesture {
    /// Passation vers ce DJ (`None` : le suivant dans la liste).
    Handover(Option<usize>),
}

/// Pourquoi une passation n'est pas possible maintenant, `None` si elle l'est.
pub(crate) fn blocked(view: &DjView<'_>) -> Option<&'static str> {
    let count = view.sheet.map_or(0, |s| s.slots.len());
    if count < 2 {
        return Some("Il faut un spectacle chargé avec au moins deux DJ (--show …).");
    }
    match view.run.map(|r| r.phase) {
        Some(Phase::Handover) => Some("Une passation est déjà en cours."),
        Some(Phase::Waiting) => Some("Lance le show avant une passation."),
        Some(Phase::Starting) => Some("Le show démarre : attends la fin de la séquence."),
        Some(Phase::Stopping) => Some("Le show s'arrête."),
        Some(Phase::Running) | None => None,
    }
}

/// Le rang du DJ précédent et du suivant.
pub(crate) fn neighbours(slot: usize, count: usize) -> (Option<usize>, Option<usize>) {
    (slot.checked_sub(1), (slot + 1 < count).then_some(slot + 1))
}

/// Le nom d'un DJ pour l'interface (« DJ 3 » quand la feuille ne le nomme pas).
pub(crate) fn dj_name(sheet: &ShowSheet, index: usize) -> String {
    match sheet.slots.get(index).map(|s| s.dj.trim()) {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ => format!("DJ {}", index + 1),
    }
}

/// La section.
pub(crate) fn draw(ui: &mut egui::Ui, view: &DjView<'_>, gestures: &mut Vec<Gesture>) {
    ui.strong("DJ");
    let Some(sheet) = view.sheet.filter(|s| !s.slots.is_empty()) else {
        ui.weak("Aucun spectacle chargé : pas de DJ à enchaîner.");
        return;
    };
    let current = view.slot.unwrap_or(0).min(sheet.slots.len() - 1);
    ui.label(format!("En cours : {}", dj_name(sheet, current)));
    if let (Some(run), Some(to)) = (view.run.filter(|r| r.phase == Phase::Handover), view.handover_to) {
        ui.label(format!("Passation vers {}", dj_name(sheet, to)));
        ui.add(egui::ProgressBar::new(run.progress).desired_height(10.0));
    }
    let reason = blocked(view);
    let (previous, next) = neighbours(current, sheet.slots.len());
    ui.horizontal_wrapped(|ui| {
        for (label, target) in [("◀ Précédent", previous), ("Suivant ▶", next)] {
            let response = ui.add_enabled(reason.is_none() && target.is_some(), egui::Button::new(label));
            if response.clicked() {
                gestures.push(Gesture::Dj(DjGesture::Handover(target)));
            }
            if let Some(reason) = reason {
                response.on_disabled_hover_text(reason);
            } else if target.is_none() {
                response.on_disabled_hover_text("Pas d'autre DJ de ce côté de la liste.");
            }
        }
    });
    if let Some(reason) = reason {
        ui.weak(reason);
    }
    ui.small("PASSER À");
    for index in 0..sheet.slots.len() {
        let selected = index == current;
        let enabled = reason.is_none() && !selected;
        let response = ui.add_enabled(enabled, egui::Button::selectable(selected, dj_name(sheet, index)));
        if response.clicked() {
            gestures.push(Gesture::Dj(DjGesture::Handover(Some(index))));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_render::SheetSlot;

    fn sheet(names: &[&str]) -> ShowSheet {
        ShowSheet {
            slots: names
                .iter()
                .map(|name| SheetSlot {
                    dj: (*name).to_owned(),
                    theme: None,
                    logo: None,
                    schedule: None,
                    cues: Vec::new(),
                })
                .collect(),
            ..ShowSheet::default()
        }
    }

    fn view(sheet: Option<&ShowSheet>, phase: Option<Phase>) -> DjView<'_> {
        DjView {
            sheet,
            slot: Some(0),
            handover_to: None,
            run: phase.map(|phase| RunPhase { phase, progress: 0.5 }),
        }
    }

    #[test]
    fn sans_spectacle_ou_avec_un_seul_dj_aucune_passation() {
        assert!(blocked(&view(None, None)).is_some());
        let solo = sheet(&["Seul"]);
        assert!(blocked(&view(Some(&solo), None)).is_some());
    }

    #[test]
    fn la_passation_n_est_permise_que_show_en_cours() {
        let duo = sheet(&["A", "B"]);
        assert_eq!(blocked(&view(Some(&duo), Some(Phase::Running))), None);
        assert_eq!(
            blocked(&view(Some(&duo), None)),
            None,
            "sans séquenceur, le show tourne"
        );
        for phase in [Phase::Waiting, Phase::Starting, Phase::Stopping, Phase::Handover] {
            assert!(blocked(&view(Some(&duo), Some(phase))).is_some(), "{phase:?}");
        }
    }

    #[test]
    fn les_voisins_s_arretent_aux_bords_de_la_liste() {
        assert_eq!(neighbours(0, 3), (None, Some(1)));
        assert_eq!(neighbours(1, 3), (Some(0), Some(2)));
        assert_eq!(neighbours(2, 3), (Some(1), None));
    }

    #[test]
    fn un_dj_sans_nom_est_numerote() {
        let s = sheet(&["Kira", "  "]);
        assert_eq!(dj_name(&s, 0), "Kira");
        assert_eq!(dj_name(&s, 1), "DJ 2");
        assert_eq!(dj_name(&s, 9), "DJ 10");
    }
}
