//! Les listes de créneaux et de cues : sélection au clic, réordonnancement par glisser-déposer.

use bevy_egui::egui;

/// Ce que l'utilisateur a fait d'une liste pendant l'image.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct ListAction {
    /// Rang cliqué.
    pub select: Option<usize>,
    /// Glissé-déposé : (rang d'origine, rang d'arrivée).
    pub moved: Option<(usize, usize)>,
}

/// Une ligne de liste.
pub(super) struct ListRow {
    pub label: String,
    pub selected: bool,
    /// Le contenu pose un problème de validation (affiché en rouge).
    pub flagged: bool,
}

/// Affiche une liste réordonnable. `salt` distingue les listes entre elles (identifiants egui des lignes).
pub(super) fn reorderable(ui: &mut egui::Ui, salt: &str, rows: &[ListRow]) -> ListAction {
    let mut action = ListAction::default();
    let mut rects = Vec::with_capacity(rows.len());
    let (_, dropped) = ui.dnd_drop_zone::<usize, ()>(egui::Frame::default().inner_margin(4.0), |ui| {
        ui.set_min_width(ui.available_width());
        for (i, row) in rows.iter().enumerate() {
            let id = egui::Id::new((salt, i));
            let inner = ui.dnd_drag_source(id, i, |ui| {
                let mut text = egui::RichText::new(&row.label);
                if row.flagged {
                    text = text.color(egui::Color32::LIGHT_RED);
                }
                ui.add_sized(
                    [ui.available_width(), 24.0],
                    egui::Button::selectable(row.selected, text),
                )
            });
            if inner.inner.clicked() {
                action.select = Some(i);
            }
            rects.push(inner.response.rect);
        }
    });
    if let Some(from) = dropped {
        let pointer_y = ui.ctx().pointer_latest_pos().map(|p| p.y);
        action.moved = pointer_y.map(|y| (*from, drop_index(&rects, y)));
    }
    action
}

/// Rang d'arrivée d'un dépôt à l'ordonnée `y` : le rang de la ligne survolée (la dernière si on dépose dessous).
fn drop_index(rects: &[egui::Rect], y: f32) -> usize {
    rects
        .iter()
        .position(|r| y <= r.bottom())
        .unwrap_or_else(|| rects.len().saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rects() -> Vec<egui::Rect> {
        (0..3u8)
            .map(|i| {
                let top = f32::from(i) * 30.0;
                egui::Rect::from_min_max(egui::pos2(0.0, top), egui::pos2(100.0, top + 28.0))
            })
            .collect()
    }

    #[test]
    fn le_depot_vise_la_ligne_survolee() {
        let r = rects();
        assert_eq!(drop_index(&r, 5.0), 0);
        assert_eq!(drop_index(&r, 35.0), 1);
        assert_eq!(drop_index(&r, 70.0), 2);
    }

    #[test]
    fn deposer_sous_la_derniere_ligne_vise_la_derniere() {
        assert_eq!(drop_index(&rects(), 500.0), 2);
        assert_eq!(drop_index(&[], 5.0), 0);
    }
}
