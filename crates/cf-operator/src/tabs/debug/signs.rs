//! Les panneaux de l'onglet Debug : choisir un type de panneau, sa variation, son texte, et le poser devant la voiture.
//!
//! ```text
//! Enseigne DJ                         (l'enseigne du DJ en cours, touche T)
//! SUR LA ROUTE   Portique · Potence · Portique-caméra
//! SUR LES CÔTÉS  Hologramme · Néon · Projection
//! SUR LES IMMEUBLES  Panneau de toit · Écran géant
//! Variation      Visuel [Auto ▾]   Texte [Auto ▾]   ▶ Relancer
//! ▸ Tous les textes (N)              un clic pose ce texte sur un panneau qui sait l'afficher
//! ```
//!
//! Un clic sur un type le pose tout de suite avec la variation choisie (« Auto » : ce que la route aurait tiré). Les
//! listes viennent du monde (`SignCatalog`) : l'interface ne connaît aucun panneau par son nom.

use bevy_egui::egui;
use cf_control::Action;
use cf_render::{SignCatalog, SignGroup, SignOrder};

use super::chip::push_order;
use super::columns::column_color;
use super::rows::{Row, draw_row};
use super::{DebugState, Gesture};
use cf_render::EventCategory;

/// Teinte de la colonne des panneaux.
const SIGN_COLOR: egui::Color32 = column_color(EventCategory::Sign);
/// Longueur maximale (caractères) d'un texte dans une liste déroulante.
const COMBO_TEXT_CHARS: usize = 34;
/// Hauteur de la liste de tous les textes (px) : elle défile seule.
const ALL_TEXTS_HEIGHT: f32 = 260.0;

/// Le choix de l'opérateur. Vit dans `DebugState`, jamais dans les réglages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct SignPick {
    /// Rang du dernier format posé dans `SignCatalog::formats`.
    pub format: Option<usize>,
    /// Visuel choisi ; `None` : automatique.
    pub ad: Option<u8>,
    /// Texte choisi ; `None` : automatique.
    pub slogan: Option<u8>,
}

impl SignPick {
    /// Choisit un format ; le visuel et le texte retombent en automatique s'il ne sait pas les afficher.
    pub(crate) fn choose_format(&mut self, catalog: &SignCatalog, format: usize) {
        self.format = Some(format);
        let knows = |ad: u8| catalog.formats.get(format).is_some_and(|f| f.ads.contains(&ad));
        if self.ad.is_some_and(|ad| !knows(ad)) {
            self.ad = None;
            self.slogan = None;
        }
    }

    /// Choisit un visuel (le texte retombe en automatique).
    pub(crate) fn choose_ad(&mut self, ad: Option<u8>) {
        if self.ad != ad {
            self.ad = ad;
            self.slogan = None;
        }
    }

    /// L'ordre à envoyer pour le format choisi, `None` si aucun n'est choisi ou s'il n'existe plus.
    pub(crate) fn order(&self, catalog: &SignCatalog) -> Option<SignOrder> {
        let format = self.format.filter(|&f| f < catalog.formats.len())?;
        Some(SignOrder {
            format: u8::try_from(format).ok()?,
            ad: self.ad,
            slogan: self.slogan,
        })
    }

    /// Le choix « ce visuel, ce texte » : le format qui le montre est repris s'il le sait, sinon le premier qui le sait.
    pub(crate) fn choose_text(&mut self, catalog: &SignCatalog, ad: u8, slogan: u8) -> bool {
        let Some(format) = catalog.format_for_ad(ad, self.format) else {
            return false;
        };
        self.format = Some(format);
        self.ad = Some(ad);
        self.slogan = Some(slogan);
        true
    }
}

/// Un texte raccourci pour une liste déroulante.
pub(crate) fn short(text: &str) -> String {
    if text.chars().count() <= COMBO_TEXT_CHARS {
        return text.to_owned();
    }
    let cut: String = text.chars().take(COMBO_TEXT_CHARS - 1).collect();
    format!("{cut}…")
}

/// La zone « Panneaux » en tête de la colonne du même nom.
pub(crate) fn draw(ui: &mut egui::Ui, catalog: Option<&SignCatalog>, state: &mut DebugState, orders: &mut Vec<Action>) {
    ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
    if ui
        .button("Enseigne DJ")
        .on_hover_text("Le réalisateur montre l'enseigne du DJ en cours (comme la touche T).")
        .clicked()
    {
        push_order(orders, Action::ShowDj);
    }
    let Some(catalog) = catalog.filter(|c| !c.is_empty()) else {
        ui.weak("Panneaux absents (monde absent).");
        return;
    };
    let DebugState { sign, gestures, .. } = state;
    for group in SignGroup::ALL {
        ui.small(group.label().to_uppercase());
        for (index, format) in catalog.formats.iter().enumerate().filter(|(_, f)| f.group == group) {
            let row = Row {
                selected: sign.format == Some(index),
                ..Row::new(format.label, SIGN_COLOR)
            };
            if draw_row(ui, &row).on_hover_text(format.hint).clicked() {
                sign.choose_format(catalog, index);
                if let Some(order) = sign.order(catalog) {
                    gestures.push(Gesture::Sign(order));
                }
            }
        }
    }
    draw_variation(ui, catalog, sign, gestures);
    draw_all_texts(ui, catalog, sign, gestures);
}

/// Les deux listes déroulantes de variation et « Relancer ».
fn draw_variation(ui: &mut egui::Ui, catalog: &SignCatalog, sign: &mut SignPick, gestures: &mut Vec<Gesture>) {
    let Some(format) = sign.format.and_then(|i| catalog.formats.get(i)) else {
        ui.weak("Choisis un type pour le poser et régler sa variation.");
        return;
    };
    ui.small(format!("VARIATION · {}", format.label.to_uppercase()));
    let ad_text = sign
        .ad
        .and_then(|ad| catalog.ads.get(usize::from(ad)))
        .map_or("Auto", |ad| ad.label.as_str());
    let mut ad = sign.ad;
    egui::ComboBox::from_id_salt("debug-sign-ad")
        .selected_text(format!("Visuel : {ad_text}"))
        .width(ui.available_width() - 8.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut ad, None, "Auto");
            for &index in &format.ads {
                if let Some(info) = catalog.ads.get(usize::from(index)) {
                    ui.selectable_value(&mut ad, Some(index), &info.label);
                }
            }
        });
    sign.choose_ad(ad);
    let slogans = sign
        .ad
        .and_then(|ad| catalog.ads.get(usize::from(ad)))
        .map(|ad| ad.slogans.as_slice());
    let slogan_text = match (sign.slogan, slogans) {
        (Some(i), Some(list)) => list.get(usize::from(i)).map_or("Auto".to_owned(), |t| short(t)),
        _ => "Auto".to_owned(),
    };
    ui.add_enabled_ui(slogans.is_some(), |ui| {
        let mut slogan = sign.slogan;
        egui::ComboBox::from_id_salt("debug-sign-slogan")
            .selected_text(format!("Texte : {slogan_text}"))
            .width(ui.available_width() - 8.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut slogan, None, "Auto");
                for (i, text) in slogans.unwrap_or_default().iter().enumerate() {
                    if let Ok(i) = u8::try_from(i) {
                        ui.selectable_value(&mut slogan, Some(i), short(text));
                    }
                }
            });
        sign.slogan = slogan;
    })
    .response
    .on_disabled_hover_text("Choisis d'abord un visuel.");
    if ui
        .button("▶ Relancer")
        .on_hover_text("Pose un nouveau panneau de ce type avec cette variation.")
        .clicked()
        && let Some(order) = sign.order(catalog)
    {
        gestures.push(Gesture::Sign(order));
    }
}

/// « Tous les textes » : chaque publicité et chacun de ses textes ; un clic pose ce texte.
fn draw_all_texts(ui: &mut egui::Ui, catalog: &SignCatalog, sign: &mut SignPick, gestures: &mut Vec<Gesture>) {
    let total: usize = catalog.ads.iter().map(|ad| ad.slogans.len()).sum();
    egui::CollapsingHeader::new(format!("Tous les textes ({total})"))
        .id_salt("debug-sign-texts")
        .default_open(false)
        .show(ui, |ui| {
            ui.weak("Un clic pose ce texte sur un panneau qui sait l'afficher (celui choisi, sinon le premier).");
            egui::ScrollArea::vertical()
                .id_salt("debug-sign-texts-list")
                .max_height(ALL_TEXTS_HEIGHT)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (ad_index, ad) in catalog.ads.iter().enumerate() {
                        let Ok(ad_rank) = u8::try_from(ad_index) else { continue };
                        ui.small(ad.label.to_uppercase());
                        for (text_index, text) in ad.slogans.iter().enumerate() {
                            let Ok(text_rank) = u8::try_from(text_index) else {
                                continue;
                            };
                            let row = Row {
                                selected: sign.ad == Some(ad_rank) && sign.slogan == Some(text_rank),
                                ..Row::new(text, SIGN_COLOR)
                            };
                            if draw_row(ui, &row).on_hover_text(*text).clicked()
                                && sign.choose_text(catalog, ad_rank, text_rank)
                                && let Some(order) = sign.order(catalog)
                            {
                                gestures.push(Gesture::Sign(order));
                            }
                        }
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_render::{SignAdInfo, SignFormatInfo};

    fn catalog() -> SignCatalog {
        let format = |key, group, ads: &[u8]| SignFormatInfo {
            key,
            label: key,
            hint: "",
            group,
            ads: ads.to_vec(),
        };
        let ad = |label: &str, slogans: &[&'static str]| SignAdInfo {
            label: label.to_owned(),
            slogans: slogans.to_vec(),
        };
        SignCatalog {
            formats: vec![
                format("gantry", SignGroup::Road, &[0, 1]),
                format("neon", SignGroup::Side, &[2]),
            ],
            ads: vec![ad("A", &["a1", "a2"]), ad("B", &["b1"]), ad("N", &["n1", "n2"])],
        }
    }

    #[test]
    fn changer_de_format_garde_la_variation_seulement_si_le_format_la_connait() {
        let catalog = catalog();
        let mut pick = SignPick {
            format: Some(0),
            ad: Some(1),
            slogan: Some(0),
        };
        pick.choose_format(&catalog, 1);
        assert_eq!(
            (pick.format, pick.ad, pick.slogan),
            (Some(1), None, None),
            "le néon ne connaît pas B"
        );
        pick.ad = Some(2);
        pick.slogan = Some(1);
        pick.choose_format(&catalog, 1);
        assert_eq!((pick.ad, pick.slogan), (Some(2), Some(1)));
    }

    #[test]
    fn changer_de_visuel_remet_le_texte_en_automatique() {
        let mut pick = SignPick {
            ad: Some(0),
            slogan: Some(1),
            ..SignPick::default()
        };
        pick.choose_ad(Some(0));
        assert_eq!(pick.slogan, Some(1), "même visuel : le texte reste");
        pick.choose_ad(Some(1));
        assert_eq!(pick.slogan, None);
    }

    #[test]
    fn un_texte_choisi_prend_le_format_qui_sait_l_afficher() {
        let catalog = catalog();
        let mut pick = SignPick {
            format: Some(0),
            ..SignPick::default()
        };
        assert!(pick.choose_text(&catalog, 0, 1));
        assert_eq!(pick.format, Some(0), "le format choisi le sait : on le garde");
        assert!(pick.choose_text(&catalog, 2, 0));
        assert_eq!(pick.format, Some(1), "seul le néon sait afficher N");
        assert_eq!(
            pick.order(&catalog),
            Some(SignOrder {
                format: 1,
                ad: Some(2),
                slogan: Some(0)
            })
        );
        assert!(!pick.choose_text(&catalog, 9, 0), "visuel inconnu");
    }

    #[test]
    fn sans_format_choisi_ou_apres_un_catalogue_plus_court_il_n_y_a_pas_d_ordre() {
        let catalog = catalog();
        assert_eq!(SignPick::default().order(&catalog), None);
        let stale = SignPick {
            format: Some(7),
            ..SignPick::default()
        };
        assert_eq!(stale.order(&catalog), None);
    }

    #[test]
    fn un_long_texte_est_raccourci_sans_couper_un_caractere_accentue() {
        let long = "ÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉÉ";
        let cut = short(long);
        assert_eq!(cut.chars().count(), COMBO_TEXT_CHARS);
        assert!(cut.ends_with('…'));
        assert_eq!(short("court"), "court");
    }
}
