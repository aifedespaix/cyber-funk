//! Règles d'un événement : réglages, looks embarqués, créneaux, cues et références de looks.

use super::look::{check_look, check_overrides};
use super::{Issue, Report, index, join};
use crate::model::cue::{MAX_ADVANCE_BARS, MAX_ADVANCE_SECONDS, MAX_CUE_NAME, MAX_FADE_BARS, MAX_TITLE};
use crate::model::event::{
    BPM_LIMITS, MAX_CUES, MAX_DJ_NAME, MAX_EVENT_NAME, MAX_LATENCY_OFFSET_MS, MAX_MONITOR_NAME, MAX_SLOTS,
};
use crate::model::{
    AudioProfile, Cue, CueAdvance, CueEntry, DjSlot, Event, EventSettings, FORMAT_VERSION, LookId, MonitorSelect,
    Trigger,
};

/// Vérifie un événement ; `look_exists` dit si un identifiant de look est disponible (bibliothèque + looks
/// embarqués). Renvoie tous les problèmes, vide si l'événement est valide.
#[must_use]
pub fn validate_event(event: &Event, look_exists: &dyn Fn(&LookId) -> bool) -> Vec<Issue> {
    let mut r = Report::default();
    if event.version != FORMAT_VERSION {
        r.push(
            "version".to_owned(),
            format!("version {}, attendu {FORMAT_VERSION}", event.version),
        );
    }
    check_settings(&mut r, &event.settings, look_exists);
    r.unique("looks", event.looks.iter().map(|look| &look.id));
    for (i, look) in event.looks.iter().enumerate() {
        check_look(&mut r, &index("looks", i), look);
    }
    r.count("slots".to_owned(), event.slots.len(), 1, MAX_SLOTS);
    r.unique("slots", event.slots.iter().map(|slot| &slot.id));
    for (i, slot) in event.slots.iter().enumerate() {
        check_slot(&mut r, &index("slots", i), slot, look_exists);
    }
    r.into_issues()
}

fn check_settings(r: &mut Report, settings: &EventSettings, look_exists: &dyn Fn(&LookId) -> bool) {
    r.text("settings.name".to_owned(), &settings.name, 0, MAX_EVENT_NAME);
    r.f32_in(
        "settings.latency_offset_ms".to_owned(),
        settings.latency_offset_ms,
        -MAX_LATENCY_OFFSET_MS..=MAX_LATENCY_OFFSET_MS,
    );
    if let MonitorSelect::Name(name) = &settings.projector {
        r.text("settings.projector".to_owned(), name, 1, MAX_MONITOR_NAME);
    }
    check_look_ref(r, "settings.safe_look".to_owned(), &settings.safe_look, look_exists);
    r.unique("settings.controls", settings.controls.iter());
}

fn check_look_ref(r: &mut Report, path: String, id: &LookId, look_exists: &dyn Fn(&LookId) -> bool) {
    if !look_exists(id) {
        r.push(
            path,
            format!("look « {id} » introuvable (ni dans l'événement, ni dans la bibliothèque assets/looks/)"),
        );
    }
}

fn check_slot(r: &mut Report, p: &str, slot: &DjSlot, look_exists: &dyn Fn(&LookId) -> bool) {
    r.text(join(p, "dj.name"), &slot.dj.name, 0, MAX_DJ_NAME);
    if let Some(schedule) = slot.dj.schedule
        && schedule.start == schedule.end
    {
        r.push(join(p, "dj.schedule.end"), "la fin prévue doit différer du début");
    }
    check_audio(r, &join(p, "audio"), &slot.audio);
    r.unique(&join(p, "controls"), slot.controls.iter());
    let cp = join(p, "cues");
    r.count(cp.clone(), slot.cues.len(), 1, MAX_CUES);
    r.unique(&cp, slot.cues.iter().map(|cue| &cue.id));
    for (i, cue) in slot.cues.iter().enumerate() {
        check_cue(r, &index(&cp, i), cue, look_exists);
    }
}

fn check_audio(r: &mut Report, p: &str, audio: &AudioProfile) {
    let bpm = BPM_LIMITS.0..=BPM_LIMITS.1;
    r.f32_in(join(p, "bpm_min"), audio.bpm_min, bpm.clone());
    r.f32_in(join(p, "bpm_max"), audio.bpm_max, bpm);
    if audio.bpm_min >= audio.bpm_max {
        r.push(
            join(p, "bpm_max"),
            format!("{} doit dépasser bpm_min ({})", audio.bpm_max, audio.bpm_min),
        );
    }
    r.f32_in(join(p, "sensitivity"), audio.sensitivity, 0.0..=1.0);
}

fn check_cue(r: &mut Report, p: &str, cue: &Cue, look_exists: &dyn Fn(&LookId) -> bool) {
    r.text(join(p, "name"), &cue.name, 1, MAX_CUE_NAME);
    check_look_ref(r, join(p, "look"), &cue.look, look_exists);
    check_overrides(r, &join(p, "overrides"), &cue.overrides);
    if let CueEntry::Fade { bars } = cue.entry {
        r.int_in(join(p, "entry.bars"), bars, 1..=MAX_FADE_BARS);
    }
    for (i, trigger) in cue.triggers.iter().enumerate() {
        if let Trigger::Title(text) = trigger {
            r.text(index(&join(p, "triggers"), i), text, 1, MAX_TITLE);
        }
    }
    match cue.advance {
        CueAdvance::Bars { bars } => r.int_in(join(p, "advance.bars"), bars, 1..=MAX_ADVANCE_BARS),
        CueAdvance::After { seconds } => {
            if !seconds.is_finite() || seconds <= 0.0 || seconds > MAX_ADVANCE_SECONDS {
                r.push(
                    join(p, "advance.seconds"),
                    format!("{seconds} hors de la plage permise ]0, {MAX_ADVANCE_SECONDS}] (secondes)"),
                );
            }
        }
        CueAdvance::Manual | CueAdvance::OnDrop | CueAdvance::At { .. } | CueAdvance::OnMusic => {}
    }
}
