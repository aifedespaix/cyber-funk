//! Formulaires de l'événement et d'un créneau.

use bevy_egui::egui;
use cf_show::drop::TitleLanguage;
use cf_show::model::event::{MAX_DJ_NAME, MAX_EVENT_NAME, MAX_LATENCY_OFFSET_MS};
use cf_show::model::{
    AssetPath, ControlProfileId, DJ_PALETTES, DjSlot, Event, FlashPolicy, LookId, MonitorSelect, PresetTheme, Theme,
};

use super::widgets::{Touch, choice, number, row, text_field, toggle};

const PRESETS: [(PresetTheme, &str); 6] = [
    (PresetTheme::Velvet, "Velours"),
    (PresetTheme::Synthwave, "Synthwave"),
    (PresetTheme::Terminal, "Terminal"),
    (PresetTheme::Aurora, "Aurore"),
    (PresetTheme::Gold, "Or"),
    (PresetTheme::Frost, "Givre"),
];

/// Réglages de l'événement : nom, flashs, latence, écran, look de secours, enseignes, profils, langue.
pub(crate) fn event_form(
    ui: &mut egui::Ui,
    event: &mut Event,
    looks: &[LookId],
    library_ids: &[String],
    touch: &mut Touch,
) {
    let s = &mut event.settings;
    row(ui, "Nom", |ui| {
        text_field(ui, touch, &mut s.name, MAX_EVENT_NAME);
    });
    row(ui, "Thème", |ui| {
        theme_choice(ui, touch, "event_theme", &mut s.theme);
    });
    row(ui, "Flashs", |ui| {
        choice(
            ui,
            touch,
            "flashes",
            &mut s.flashes,
            &[
                (FlashPolicy::Limited, "Permis (limiteur actif)"),
                (FlashPolicy::Off, "Aucun"),
            ],
        );
    });
    row(ui, "Latence image", |ui| {
        number(
            ui,
            touch,
            &mut s.latency_offset_ms,
            -MAX_LATENCY_OFFSET_MS..=MAX_LATENCY_OFFSET_MS,
            " ms",
        );
    });
    row(ui, "Écran projecteur", |ui| {
        monitor_choice(ui, touch, &mut s.projector);
    });
    row(ui, "Look de secours", |ui| {
        look_choice(ui, touch, "safe_look", &mut s.safe_look, looks);
    });
    row(ui, "Enseigne géante", |ui| {
        toggle(ui, touch, &mut s.dj_sign, "Nom du DJ en grand dans le monde");
    });
    row(ui, "Publicités", |ui| {
        toggle(ui, touch, &mut s.dj_on_ads, "Nom du DJ sur les publicités");
    });
    row(ui, "Langue des titres", |ui| {
        choice(
            ui,
            touch,
            "title_language",
            &mut s.title_language,
            &[(TitleLanguage::Fr, "Français"), (TitleLanguage::En, "English")],
        );
    });
    row(ui, "Profils de contrôle", |ui| {
        profile_list(ui, touch, "event_controls", library_ids, &mut s.controls);
    });
}

/// Fiche d'un DJ : identifiant, nom, logo et thème. Le reste de son créneau (conduite, mode, profil audio) est
/// automatique : l'opérateur n'a rien à y régler, et le fichier de la soirée le garde tel quel.
pub(crate) fn slot_form(ui: &mut egui::Ui, slot: &mut DjSlot, touch: &mut Touch) {
    row(ui, "Identifiant", |ui| {
        ui.label(slot.id.as_str());
    });
    row(ui, "Nom du DJ", |ui| {
        text_field(ui, touch, &mut slot.dj.name, MAX_DJ_NAME);
    });
    row(ui, "Logo", |ui| {
        logo_field(ui, touch, &mut slot.dj.logo);
    });
    row(ui, "Thème du DJ", |ui| {
        let mut own = slot.dj.theme.is_some();
        let response = ui.checkbox(&mut own, "Impose le sien");
        if response.changed() {
            slot.dj.theme = own.then_some(Theme::Preset(PresetTheme::Velvet));
            touch.discrete(&response);
        }
        if let Some(theme) = &mut slot.dj.theme {
            theme_choice(ui, touch, "dj_theme", theme);
        }
    });
}

/// Choix d'un look parmi ceux qui existent (un identifiant absent de la liste reste affiché, signalé par la
/// validation).
pub(crate) fn look_choice(ui: &mut egui::Ui, touch: &mut Touch, salt: &str, value: &mut LookId, looks: &[LookId]) {
    let mut options: Vec<(LookId, String)> = looks.iter().map(|id| (id.clone(), id.to_string())).collect();
    if !looks.contains(value) {
        options.insert(0, (value.clone(), format!("{value} (introuvable)")));
    }
    let borrowed: Vec<(LookId, &str)> = options.iter().map(|(id, label)| (id.clone(), label.as_str())).collect();
    choice(ui, touch, salt, value, &borrowed);
}

fn theme_choice(ui: &mut egui::Ui, touch: &mut Touch, salt: &str, theme: &mut Theme) {
    // Les six thèmes de Sillage, puis les dix palettes de présélection des DJs.
    let mut options: Vec<(Theme, &str)> = PRESETS.iter().map(|(p, label)| (Theme::Preset(*p), *label)).collect();
    options.extend(DJ_PALETTES.iter().filter_map(|p| Some((p.theme()?, p.label))));
    // Cinq couleurs écrites à la main dans le fichier : elles restent affichées et choisies.
    if !options.iter().any(|(t, _)| t == theme) {
        options.insert(0, (theme.clone(), "Personnalisé (fichier)"));
    }
    choice(ui, touch, salt, theme, &options);
}

fn monitor_choice(ui: &mut egui::Ui, touch: &mut Touch, monitor: &mut MonitorSelect) {
    #[derive(Clone, Copy, PartialEq)]
    enum Kind {
        Auto,
        Index,
        Name,
    }
    let mut kind = match monitor {
        MonitorSelect::Auto => Kind::Auto,
        MonitorSelect::Index(_) => Kind::Index,
        MonitorSelect::Name(_) => Kind::Name,
    };
    let before = kind;
    choice(
        ui,
        touch,
        "monitor_kind",
        &mut kind,
        &[
            (Kind::Auto, "Automatique"),
            (Kind::Index, "Par rang"),
            (Kind::Name, "Par nom"),
        ],
    );
    if kind != before {
        *monitor = match kind {
            Kind::Auto => MonitorSelect::Auto,
            Kind::Index => MonitorSelect::Index(1),
            Kind::Name => MonitorSelect::Name("Écran".to_owned()),
        };
    }
    match monitor {
        MonitorSelect::Auto => {}
        MonitorSelect::Index(rank) => {
            let response = ui.add(egui::DragValue::new(rank).range(0..=15));
            touch.continuous(&response);
        }
        MonitorSelect::Name(name) => text_field(ui, touch, name, cf_show::model::event::MAX_MONITOR_NAME),
    }
}

fn logo_field(ui: &mut egui::Ui, touch: &mut Touch, logo: &mut Option<AssetPath>) {
    let mut text = logo.as_ref().map(|p| p.as_str().to_owned()).unwrap_or_default();
    let response = ui.add(
        egui::TextEdit::singleline(&mut text)
            .hint_text("logos/dj.png (sous assets/)")
            .desired_width(260.0),
    );
    if response.changed() {
        // Un chemin invalide n'est pas pris : on garde l'ancien tant que la saisie n'est pas valide.
        if text.is_empty() {
            *logo = None;
        } else if let Ok(path) = AssetPath::new(text) {
            *logo = Some(path);
        }
        touch.continuous(&response);
    }
}

/// Liste de profils de contrôle : retirer d'un clic, ajouter par identifiant.
fn profile_list(
    ui: &mut egui::Ui,
    touch: &mut Touch,
    salt: &str,
    library_ids: &[String],
    list: &mut Vec<ControlProfileId>,
) {
    ui.vertical(|ui| {
        let mut remove = None;
        for (i, id) in list.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(id.as_str());
                if ui.small_button("✕").on_hover_text("Retirer").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            list.remove(i);
            touch.structural();
        }
        // Les profils livrés (`assets/controllers`) se choisissent d'un clic ; la saisie reste pour un profil à venir.
        let missing: Vec<ControlProfileId> = library_ids
            .iter()
            .filter_map(|id| ControlProfileId::new(id.clone()).ok())
            .filter(|id| !list.contains(id))
            .collect();
        if !missing.is_empty() {
            ui.menu_button("+ Profil livré", |ui| {
                for id in missing {
                    if ui.button(id.as_str()).clicked() {
                        list.push(id);
                        touch.structural();
                        ui.close();
                    }
                }
            });
        }
        let buffer_id = ui.id().with(salt);
        let mut text: String = ui.data_mut(|d| d.get_temp(buffer_id).unwrap_or_default());
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .hint_text("identifiant")
                    .desired_width(160.0),
            );
            let valid = ControlProfileId::new(text.clone()).ok().filter(|id| !list.contains(id));
            if ui.add_enabled(valid.is_some(), egui::Button::new("Ajouter")).clicked()
                && let Some(id) = valid
            {
                list.push(id);
                text.clear();
                touch.structural();
            }
            let _ = field;
        });
        ui.data_mut(|d| d.insert_temp(buffer_id, text));
    });
}
