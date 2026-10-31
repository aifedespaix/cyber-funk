//! Formulaire d'un cue : look, entrée, avancée, déclencheurs.

use bevy_egui::egui;
use cf_show::model::cue::{MAX_ADVANCE_BARS, MAX_ADVANCE_SECONDS, MAX_CUE_NAME, MAX_FADE_BARS, MAX_TITLE};
use cf_show::model::{ClockTime, Cue, CueAdvance, CueEntry, EventName, LookId, Trigger};

use super::forms::look_choice;
use super::widgets::{Touch, choice, row, text_field};

/// Le genre d'une entrée, sans ses paramètres (pour la liste déroulante).
#[derive(Clone, Copy, PartialEq)]
enum EntryKind {
    Cut,
    Fade,
    OnDrop,
    OnPhrase,
}

/// Le genre d'une avancée, sans ses paramètres.
#[derive(Clone, Copy, PartialEq)]
enum AdvanceKind {
    Manual,
    Bars,
    OnDrop,
    At,
    After,
    OnMusic,
}

/// Les genres de déclencheurs que l'on peut ajouter.
#[derive(Clone, Copy, PartialEq)]
enum TriggerKind {
    Title,
    ShowDj,
    DjLogo,
    Ascension,
    Event,
    ForceDrop,
}

const TRIGGER_KINDS: [(TriggerKind, &str); 6] = [
    (TriggerKind::Title, "Carton-titre"),
    (TriggerKind::ShowDj, "Enseigne du DJ"),
    (TriggerKind::DjLogo, "Logo du DJ"),
    (TriggerKind::Ascension, "Ascension"),
    (TriggerKind::Event, "Événement du trafic"),
    (TriggerKind::ForceDrop, "Drop forcé"),
];

/// Édite un cue.
pub(super) fn cue_form(ui: &mut egui::Ui, cue: &mut Cue, looks: &[LookId], touch: &mut Touch) {
    row(ui, "Identifiant", |ui| {
        ui.label(cue.id.as_str());
    });
    row(ui, "Nom", |ui| {
        text_field(ui, touch, &mut cue.name, MAX_CUE_NAME);
    });
    row(ui, "Look", |ui| {
        look_choice(ui, touch, "cue_look", &mut cue.look, looks);
    });
    row(ui, "Entrée", |ui| {
        entry_field(ui, touch, &mut cue.entry);
    });
    row(ui, "Avancée", |ui| {
        advance_field(ui, touch, &mut cue.advance);
    });
    ui.separator();
    ui.strong("Déclencheurs à l'entrée");
    triggers_field(ui, touch, &mut cue.triggers);
}

fn entry_field(ui: &mut egui::Ui, touch: &mut Touch, entry: &mut CueEntry) {
    let mut kind = match entry {
        CueEntry::Cut => EntryKind::Cut,
        CueEntry::Fade { .. } => EntryKind::Fade,
        CueEntry::OnDrop => EntryKind::OnDrop,
        CueEntry::OnPhrase => EntryKind::OnPhrase,
    };
    let before = kind;
    choice(
        ui,
        touch,
        "cue_entry",
        &mut kind,
        &[
            (EntryKind::Cut, "Coupe sur le temps fort"),
            (EntryKind::Fade, "Fondu"),
            (EntryKind::OnDrop, "Au prochain drop"),
            (EntryKind::OnPhrase, "À la prochaine phrase"),
        ],
    );
    if kind != before {
        *entry = match kind {
            EntryKind::Cut => CueEntry::Cut,
            EntryKind::Fade => CueEntry::Fade { bars: 4 },
            EntryKind::OnDrop => CueEntry::OnDrop,
            EntryKind::OnPhrase => CueEntry::OnPhrase,
        };
    }
    if let CueEntry::Fade { bars } = entry {
        let response = ui.add(egui::DragValue::new(bars).range(1..=MAX_FADE_BARS).suffix(" mesures"));
        touch.continuous(&response);
    }
}

fn advance_field(ui: &mut egui::Ui, touch: &mut Touch, advance: &mut CueAdvance) {
    let mut kind = match advance {
        CueAdvance::Manual => AdvanceKind::Manual,
        CueAdvance::Bars { .. } => AdvanceKind::Bars,
        CueAdvance::OnDrop => AdvanceKind::OnDrop,
        CueAdvance::At { .. } => AdvanceKind::At,
        CueAdvance::After { .. } => AdvanceKind::After,
        CueAdvance::OnMusic => AdvanceKind::OnMusic,
    };
    let before = kind;
    choice(
        ui,
        touch,
        "cue_advance",
        &mut kind,
        &[
            (AdvanceKind::Manual, "Sur GO seulement"),
            (AdvanceKind::Bars, "Après N mesures"),
            (AdvanceKind::OnDrop, "Au prochain drop"),
            (AdvanceKind::At, "À une heure"),
            (AdvanceKind::After, "Après une durée"),
            (AdvanceKind::OnMusic, "Quand la musique est là"),
        ],
    );
    if kind != before {
        *advance = match kind {
            AdvanceKind::Manual => CueAdvance::Manual,
            AdvanceKind::Bars => CueAdvance::Bars { bars: 32 },
            AdvanceKind::OnDrop => CueAdvance::OnDrop,
            AdvanceKind::At => ClockTime::from_hm(23, 0).map_or(CueAdvance::Manual, |time| CueAdvance::At { time }),
            AdvanceKind::After => CueAdvance::After { seconds: 300.0 },
            AdvanceKind::OnMusic => CueAdvance::OnMusic,
        };
    }
    match advance {
        CueAdvance::Bars { bars } => {
            let response = ui.add(
                egui::DragValue::new(bars)
                    .range(1..=MAX_ADVANCE_BARS)
                    .suffix(" mesures"),
            );
            touch.continuous(&response);
        }
        CueAdvance::After { seconds } => {
            let response = ui.add(
                egui::DragValue::new(seconds)
                    .range(1.0..=MAX_ADVANCE_SECONDS)
                    .suffix(" s"),
            );
            touch.continuous(&response);
        }
        CueAdvance::At { time } => {
            let minutes = time.minutes_since_midnight();
            let (mut h, mut m) = (
                u8::try_from(minutes / 60).unwrap_or(0),
                u8::try_from(minutes % 60).unwrap_or(0),
            );
            let hours = ui.add(egui::DragValue::new(&mut h).range(0..=23));
            ui.label(":");
            let mins = ui.add(egui::DragValue::new(&mut m).range(0..=59));
            if (hours.changed() || mins.changed())
                && let Some(new) = ClockTime::from_hm(h, m)
            {
                *time = new;
                touch.continuous(if hours.changed() { &hours } else { &mins });
            }
        }
        CueAdvance::Manual | CueAdvance::OnDrop | CueAdvance::OnMusic => {}
    }
}

fn triggers_field(ui: &mut egui::Ui, touch: &mut Touch, triggers: &mut Vec<Trigger>) {
    let mut remove = None;
    for (i, trigger) in triggers.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            match trigger {
                Trigger::Title(text) => {
                    ui.label("Carton-titre");
                    text_field(ui, touch, text, MAX_TITLE);
                }
                Trigger::Event(name) => {
                    ui.label("Événement du trafic");
                    let mut text = name.as_str().to_owned();
                    let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(160.0));
                    if response.changed()
                        && let Ok(new) = EventName::new(text)
                    {
                        *name = new;
                        touch.continuous(&response);
                    }
                }
                Trigger::ShowDj => {
                    ui.label("Enseigne du DJ");
                }
                Trigger::DjLogo => {
                    ui.label("Logo du DJ");
                }
                Trigger::Ascension => {
                    ui.label("Ascension");
                }
                Trigger::ForceDrop => {
                    ui.label("Drop forcé");
                }
            }
            if ui.small_button("✕").on_hover_text("Retirer").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        triggers.remove(i);
        touch.structural();
    }
    ui.menu_button("+ Ajouter un déclencheur", |ui| {
        for (kind, label) in TRIGGER_KINDS {
            if ui.button(label).clicked() {
                let new = match kind {
                    TriggerKind::Title => Some(Trigger::Title("Titre".to_owned())),
                    TriggerKind::ShowDj => Some(Trigger::ShowDj),
                    TriggerKind::DjLogo => Some(Trigger::DjLogo),
                    TriggerKind::Ascension => Some(Trigger::Ascension),
                    TriggerKind::Event => EventName::new("storm").ok().map(Trigger::Event),
                    TriggerKind::ForceDrop => Some(Trigger::ForceDrop),
                };
                if let Some(new) = new {
                    triggers.push(new);
                    touch.structural();
                }
                ui.close();
            }
        }
    });
}
