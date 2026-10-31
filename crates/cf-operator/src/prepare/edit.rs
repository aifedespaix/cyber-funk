//! Opérations structurelles sur l'événement (ajouter, dupliquer, déplacer, supprimer), pures et testables sans egui.
//!
//! Elles respectent les bornes du format (`MAX_SLOTS`, `MAX_CUES`, au moins un créneau, au moins un cue) et gardent
//! les identifiants uniques : le résultat passe `validate_event` sur ces points sans rien corriger après coup.

use cf_show::drop::TitleLanguage;
use cf_show::model::event::{MAX_CUES, MAX_SLOTS};
use cf_show::model::{
    AudioProfile, Cue, CueAdvance, CueEntry, CueId, DjIdentity, DjSlot, Event, EventSettings, FORMAT_VERSION,
    FlashPolicy, LookId, LookOverrides, MonitorSelect, PresetTheme, SlotId, SlotMode, TempoPreference, Theme,
};

/// Un événement neuf : un créneau, un cue, rien d'autre. `safe_look` est le look de secours (et celui du premier cue).
pub(crate) fn new_event(safe_look: LookId) -> Option<Event> {
    let mut event = Event {
        version: FORMAT_VERSION,
        settings: EventSettings {
            name: "Nouvelle soirée".to_owned(),
            theme: Theme::Preset(PresetTheme::Velvet),
            flashes: FlashPolicy::Limited,
            latency_offset_ms: 0.0,
            projector: MonitorSelect::Auto,
            safe_look,
            dj_sign: true,
            dj_on_ads: false,
            controls: Vec::new(),
            title_language: TitleLanguage::default(),
        },
        looks: Vec::new(),
        slots: Vec::new(),
    };
    add_slot(&mut event)?;
    Some(event)
}

/// Un identifiant `<base>-<n>` qui n'est pas déjà pris (`taken` dit si un candidat existe).
fn unique_slug(base: &str, taken: impl Fn(&str) -> bool) -> String {
    (2..=u32::MAX)
        .map(|n| format!("{base}-{n}"))
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| base.to_owned())
}

/// Nouvel identifiant de créneau, jamais présent dans l'événement.
fn new_slot_id(event: &Event, base: &str) -> Option<SlotId> {
    // Une base valide plus `-<n>` reste un slug valide, sauf si la longueur maximale est dépassée (alors `None`).
    SlotId::new(unique_slug(base, |c| event.slots.iter().any(|s| s.id.as_str() == c))).ok()
}

/// Nouvel identifiant de cue, jamais présent dans le créneau.
fn new_cue_id(slot: &DjSlot, base: &str) -> Option<CueId> {
    CueId::new(unique_slug(base, |c| slot.cues.iter().any(|q| q.id.as_str() == c))).ok()
}

/// Le cue de départ d'un créneau neuf : le look de secours de l'événement, coupe franche, avancée à la main.
fn starter_cue(event: &Event) -> Option<Cue> {
    Some(Cue {
        id: CueId::new("cue-1").ok()?,
        name: "Cue 1".to_owned(),
        look: event.settings.safe_look.clone(),
        overrides: LookOverrides::default(),
        entry: CueEntry::Cut,
        triggers: Vec::new(),
        advance: CueAdvance::Manual,
    })
}

/// Ajoute un créneau neuf à la fin. Renvoie son rang, ou `None` si l'événement est plein.
pub(crate) fn add_slot(event: &mut Event) -> Option<usize> {
    if event.slots.len() >= MAX_SLOTS {
        return None;
    }
    let slot = DjSlot {
        id: new_slot_id(event, "dj")?,
        dj: DjIdentity {
            name: format!("DJ {}", event.slots.len() + 1),
            logo: None,
            theme: None,
            schedule: None,
        },
        audio: AudioProfile {
            bpm_min: 110.0,
            bpm_max: 140.0,
            sensitivity: 0.5,
            tempo_source: TempoPreference::Auto,
        },
        mode: SlotMode::Semi,
        controls: Vec::new(),
        cues: vec![starter_cue(event)?],
    };
    event.slots.push(slot);
    Some(event.slots.len() - 1)
}

/// Duplique le créneau `index` juste après lui. Renvoie le rang de la copie.
pub(crate) fn duplicate_slot(event: &mut Event, index: usize) -> Option<usize> {
    if event.slots.len() >= MAX_SLOTS {
        return None;
    }
    let mut copy = event.slots.get(index)?.clone();
    copy.id = new_slot_id(event, copy.id.as_str())?;
    event.slots.insert(index + 1, copy);
    Some(index + 1)
}

/// Supprime le créneau `index` ; refuse de supprimer le dernier (un événement a au moins un créneau). Renvoie le
/// rang à sélectionner ensuite.
pub(crate) fn remove_slot(event: &mut Event, index: usize) -> Option<usize> {
    if event.slots.len() <= 1 || index >= event.slots.len() {
        return None;
    }
    event.slots.remove(index);
    Some(index.min(event.slots.len() - 1))
}

/// Déplace un élément de `from` vers le rang `to` (rang final dans la liste). Renvoie `to` ramené dans les bornes.
fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) -> Option<usize> {
    if from >= items.len() {
        return None;
    }
    let to = to.min(items.len() - 1);
    if from == to {
        return Some(to);
    }
    let item = items.remove(from);
    items.insert(to, item);
    Some(to)
}

/// Déplace le créneau `from` au rang `to`.
pub(crate) fn move_slot(event: &mut Event, from: usize, to: usize) -> Option<usize> {
    move_item(&mut event.slots, from, to)
}

/// Ajoute un cue à la fin du créneau `slot`, copie du dernier pour garder look et réglages cohérents.
pub(crate) fn add_cue(event: &mut Event, slot: usize) -> Option<usize> {
    let target = event.slots.get(slot)?;
    if target.cues.len() >= MAX_CUES {
        return None;
    }
    let mut cue = target.cues.last()?.clone();
    cue.id = new_cue_id(target, "cue")?;
    cue.name = format!("Cue {}", target.cues.len() + 1);
    cue.triggers.clear();
    let slot = event.slots.get_mut(slot)?;
    slot.cues.push(cue);
    Some(slot.cues.len() - 1)
}

/// Duplique le cue `index` juste après lui.
pub(crate) fn duplicate_cue(event: &mut Event, slot: usize, index: usize) -> Option<usize> {
    let target = event.slots.get(slot)?;
    if target.cues.len() >= MAX_CUES {
        return None;
    }
    let mut copy = target.cues.get(index)?.clone();
    copy.id = new_cue_id(target, copy.id.as_str())?;
    let slot = event.slots.get_mut(slot)?;
    slot.cues.insert(index + 1, copy);
    Some(index + 1)
}

/// Supprime le cue `index` ; refuse de supprimer le dernier. Renvoie le rang à sélectionner ensuite.
pub(crate) fn remove_cue(event: &mut Event, slot: usize, index: usize) -> Option<usize> {
    let slot = event.slots.get_mut(slot)?;
    if slot.cues.len() <= 1 || index >= slot.cues.len() {
        return None;
    }
    slot.cues.remove(index);
    Some(index.min(slot.cues.len() - 1))
}

/// Déplace le cue `from` au rang `to` dans son créneau.
pub(crate) fn move_cue(event: &mut Event, slot: usize, from: usize, to: usize) -> Option<usize> {
    move_item(&mut event.slots.get_mut(slot)?.cues, from, to)
}

#[cfg(test)]
mod tests {
    use cf_show::load::parse_event;
    use cf_show::validate::validate_event;

    use super::*;

    const EVENT: &str = r#"Event(
        version: 1,
        settings: (
            name: "Test",
            theme: Preset(Velvet),
            flashes: Limited,
            latency_offset_ms: 0.0,
            projector: Auto,
            safe_look: "calme",
            dj_sign: true,
            dj_on_ads: false,
        ),
        slots: [(
            id: "dj-1",
            dj: (name: "A"),
            audio: (bpm_min: 120.0, bpm_max: 130.0, sensitivity: 0.5, tempo_source: Auto),
            mode: Semi,
            cues: [(id: "cue-1", name: "Intro", look: "calme", entry: Cut, advance: Manual)],
        )],
    )"#;

    fn event() -> Event {
        parse_event(EVENT, "test").expect("événement de test")
    }

    fn valid(event: &Event) -> bool {
        validate_event(event, &|_| true).is_empty()
    }

    #[test]
    fn ajouter_un_creneau_donne_un_evenement_valide() {
        let mut e = event();
        assert_eq!(add_slot(&mut e), Some(1));
        assert_eq!(e.slots.len(), 2);
        assert!(valid(&e));
    }

    #[test]
    fn dupliquer_garde_des_identifiants_uniques() {
        let mut e = event();
        assert_eq!(duplicate_slot(&mut e, 0), Some(1));
        assert_eq!(duplicate_slot(&mut e, 0), Some(1));
        let ids: Vec<_> = e.slots.iter().map(|s| s.id.as_str().to_owned()).collect();
        assert_eq!(ids, ["dj-1", "dj-1-3", "dj-1-2"]);
        assert!(valid(&e));
        assert_eq!(duplicate_cue(&mut e, 0, 0), Some(1));
        assert_eq!(duplicate_cue(&mut e, 0, 0), Some(1));
        assert!(valid(&e));
    }

    #[test]
    fn le_dernier_creneau_et_le_dernier_cue_ne_se_suppriment_pas() {
        let mut e = event();
        assert_eq!(remove_slot(&mut e, 0), None);
        assert_eq!(remove_cue(&mut e, 0, 0), None);
        assert_eq!(e, event());
    }

    #[test]
    fn supprimer_renvoie_le_rang_a_selectionner() {
        let mut e = event();
        add_slot(&mut e);
        add_slot(&mut e);
        assert_eq!(remove_slot(&mut e, 2), Some(1));
        assert_eq!(remove_slot(&mut e, 0), Some(0));
        assert_eq!(e.slots.len(), 1);
    }

    #[test]
    fn deplacer_reordonne_sans_perdre_d_element() {
        let mut e = event();
        add_cue(&mut e, 0);
        add_cue(&mut e, 0);
        let before: Vec<_> = e.slots[0].cues.iter().map(|c| c.id.as_str().to_owned()).collect();
        assert_eq!(move_cue(&mut e, 0, 0, 2), Some(2));
        let after: Vec<_> = e.slots[0].cues.iter().map(|c| c.id.as_str().to_owned()).collect();
        assert_eq!(after, [before[1].clone(), before[2].clone(), before[0].clone()]);
        assert_eq!(move_cue(&mut e, 0, 5, 0), None);
        assert!(valid(&e));
    }

    #[test]
    fn les_bornes_du_format_sont_respectees() {
        let mut e = event();
        while add_slot(&mut e).is_some() {}
        assert_eq!(e.slots.len(), MAX_SLOTS);
        assert_eq!(duplicate_slot(&mut e, 0), None);
    }
}
