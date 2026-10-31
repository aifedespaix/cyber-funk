//! Petits composants egui communs aux formulaires de préparation.

use bevy_egui::egui;

/// Ce qu'une image d'édition a changé : sert à regrouper la frappe dans un champ en une seule étape d'annulation.
#[derive(Debug, Default)]
pub(super) struct Touch {
    /// Au moins un champ a changé pendant cette image.
    changed: bool,
    /// Identité du champ modifié, seulement s'il est unique et regroupable (texte, curseur) ; `None` sinon.
    key: Option<u64>,
    /// Plusieurs champs différents ont changé : plus de regroupement.
    mixed: bool,
}

impl Touch {
    /// Note le changement d'un champ discret (liste, case, bouton) : une étape d'annulation à lui seul.
    pub(super) fn discrete(&mut self, response: &egui::Response) {
        if response.changed() {
            self.changed = true;
            self.mixed = true;
        }
    }

    /// Note le changement d'un champ continu (texte, valeur glissée) : les changements successifs du même champ
    /// ne font qu'une étape.
    pub(super) fn continuous(&mut self, response: &egui::Response) {
        if !response.changed() {
            return;
        }
        let id = response.id.value();
        match self.key {
            Some(current) if current != id => self.mixed = true,
            _ => self.key = Some(id),
        }
        self.changed = true;
    }

    /// Note un changement quelconque (modification structurelle) : jamais regroupé.
    pub(super) fn structural(&mut self) {
        self.changed = true;
        self.mixed = true;
    }

    /// La clé de regroupement à donner à l'historique.
    pub(super) fn key(&self) -> Option<u64> {
        if self.changed && !self.mixed { self.key } else { None }
    }
}

/// Une ligne « libellé : contrôle » dont le libellé a une largeur fixe pour aligner les champs.
pub(super) fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized([150.0, 20.0], egui::Label::new(label));
        add(ui);
    });
}

/// Champ texte d'une ligne, borné à `max` caractères.
pub(super) fn text_field(ui: &mut egui::Ui, touch: &mut Touch, value: &mut String, max: usize) {
    let response = ui.add(egui::TextEdit::singleline(value).desired_width(260.0).char_limit(max));
    touch.continuous(&response);
}

/// Liste déroulante sur un ensemble fini de valeurs `(valeur, libellé)`.
pub(super) fn choice<T: PartialEq + Clone>(
    ui: &mut egui::Ui,
    touch: &mut Touch,
    salt: &str,
    value: &mut T,
    options: &[(T, &str)],
) {
    let current = options
        .iter()
        .find(|(v, _)| v == value)
        .map_or("?", |(_, label)| *label);
    let mut changed = false;
    let response = egui::ComboBox::from_id_salt(salt)
        .selected_text(current)
        .show_ui(ui, |ui| {
            for (option, label) in options {
                if ui.selectable_label(option == value, *label).clicked() && option != value {
                    *value = option.clone();
                    changed = true;
                }
            }
        })
        .response;
    if changed {
        // `ComboBox` ne signale pas `changed()` : on le marque nous-mêmes sur la réponse du bouton.
        let mut marked = response;
        marked.mark_changed();
        touch.discrete(&marked);
    }
}

/// Interrupteur avec son libellé à droite.
pub(super) fn toggle(ui: &mut egui::Ui, touch: &mut Touch, value: &mut bool, label: &str) {
    let response = ui.checkbox(value, label);
    touch.discrete(&response);
}

/// Un nombre à virgule glissable, borné.
pub(super) fn number(
    ui: &mut egui::Ui,
    touch: &mut Touch,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
) {
    let response = ui.add(egui::DragValue::new(value).range(range).suffix(suffix).speed(0.5));
    touch.continuous(&response);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_seul_champ_continu_donne_une_cle() {
        let touch = Touch {
            changed: true,
            key: Some(4),
            ..Touch::default()
        };
        assert_eq!(touch.key(), Some(4));
    }

    #[test]
    fn deux_champs_ou_un_changement_discret_cassent_le_regroupement() {
        let mut touch = Touch {
            changed: true,
            key: Some(4),
            ..Touch::default()
        };
        touch.structural();
        assert_eq!(touch.key(), None);
    }

    #[test]
    fn sans_changement_pas_de_cle() {
        assert_eq!(Touch::default().key(), None);
    }
}
