//! Les phrases de la vue Live. Fonctions pures : elles ne lisent que des valeurs et se testent sans écran.

use cf_show::director::ShowState;
use cf_show::model::{CueAdvance, CueEntry};

/// Où en est un cue dans la liste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CueStatus {
    /// Déjà passé.
    Played,
    /// À l'écran.
    Current,
    /// Le prochain (ou celui qu'on attend : appelé, en attente de son entrée).
    Next,
    /// Plus loin dans la liste.
    Later,
}

/// Le cue appelé qui attend son entrée dans le créneau en cours, s'il y en a un.
pub(crate) fn called_cue(state: &ShowState) -> Option<usize> {
    state.pending.as_ref().filter(|p| p.slot == state.slot).map(|p| p.cue)
}

/// Statut du cue `index` : `current` est le cue à l'écran, `called` celui qui attend son entrée.
pub(crate) fn cue_status(current: usize, called: Option<usize>, index: usize) -> CueStatus {
    if index == current {
        CueStatus::Current
    } else if called == Some(index) || (called.is_none() && index == current + 1) {
        CueStatus::Next
    } else if index < current {
        CueStatus::Played
    } else {
        CueStatus::Later
    }
}

/// `mm:ss`, ou `h:mm:ss` à partir d'une heure. Un temps négatif ou non fini donne `00:00`.
pub(crate) fn clock(seconds: f64) -> String {
    let total = if seconds.is_finite() && seconds > 0.0 {
        seconds as u64
    } else {
        0
    };
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// `HH:MM` à partir de minutes depuis minuit.
pub(crate) fn hour_minute(minutes: u16) -> String {
    format!("{:02}:{:02}", minutes / 60 % 24, minutes % 60)
}

fn plural(n: u32) -> &'static str {
    if n > 1 { "s" } else { "" }
}

/// Comment on entre dans le cue, en clair.
pub(crate) fn entry_text(entry: CueEntry) -> String {
    match entry {
        CueEntry::Cut => "entrée : coupe sur le temps fort".to_owned(),
        CueEntry::Fade { bars } => format!("entrée : fondu de {bars} mesure{}", plural(u32::from(bars))),
        CueEntry::OnDrop => "entrée : au drop".to_owned(),
        CueEntry::OnPhrase => "entrée : à la phrase".to_owned(),
    }
}

/// Quand le cue suivant est dû, en clair.
pub(crate) fn advance_text(advance: CueAdvance) -> String {
    match advance {
        CueAdvance::Manual => "suite : à la main".to_owned(),
        CueAdvance::Bars { bars } => format!("suite : après {bars} mesure{}", plural(u32::from(bars))),
        CueAdvance::OnDrop => "suite : au drop".to_owned(),
        CueAdvance::At { time } => format!("suite : à {}", hour_minute(time.minutes_since_midnight())),
        CueAdvance::After { seconds } => format!("suite : après {}", clock(seconds)),
        CueAdvance::OnMusic => "suite : quand la musique est là".to_owned(),
    }
}

/// Le compte à rebours de la ligne du cue courant : attente d'entrée, avancée automatique ou rien.
///
/// `hold` prime sur l'avancée : elle est figée, annoncer un compte à rebours serait faux.
pub(crate) fn countdown_text(pending: Option<(CueEntry, f32)>, hold: bool, next_cue_in: Option<f32>) -> Option<String> {
    if let Some((entry, waited)) = pending {
        return Some(format!("{} (attente {})", entry_text(entry), clock(f64::from(waited))));
    }
    if hold {
        return Some("HOLD : avancée figée".to_owned());
    }
    next_cue_in.map(|s| format!("auto dans {}", clock(f64::from(s))))
}

/// Nom affiché d'un DJ : `DJ n` (à partir de 1) quand le nom est vide.
pub(crate) fn dj_name(name: &str, slot: usize) -> String {
    if name.trim().is_empty() {
        format!("DJ {}", slot + 1)
    } else {
        name.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_horloge_se_lit_comme_une_feuille_de_conduite() {
        assert_eq!(clock(0.0), "00:00");
        assert_eq!(clock(47.0 * 60.0 + 12.9), "47:12");
        assert_eq!(clock(3600.0 + 125.0), "1:02:05");
        assert_eq!(clock(-4.0), "00:00");
        assert_eq!(clock(f64::NAN), "00:00");
        assert_eq!(hour_minute(23 * 60 + 30), "23:30");
        assert_eq!(hour_minute(25 * 60), "01:00");
    }

    #[test]
    fn les_regles_se_disent_en_clair() {
        assert_eq!(entry_text(CueEntry::Fade { bars: 1 }), "entrée : fondu de 1 mesure");
        assert_eq!(entry_text(CueEntry::Fade { bars: 4 }), "entrée : fondu de 4 mesures");
        assert_eq!(entry_text(CueEntry::OnDrop), "entrée : au drop");
        assert_eq!(advance_text(CueAdvance::Bars { bars: 8 }), "suite : après 8 mesures");
        assert_eq!(advance_text(CueAdvance::After { seconds: 90.0 }), "suite : après 01:30");
        assert_eq!(advance_text(CueAdvance::Manual), "suite : à la main");
    }

    #[test]
    fn les_cues_se_rangent_joue_en_cours_suivant() {
        let row: Vec<_> = (0..5).map(|i| cue_status(2, None, i)).collect();
        assert_eq!(
            row,
            [
                CueStatus::Played,
                CueStatus::Played,
                CueStatus::Current,
                CueStatus::Next,
                CueStatus::Later
            ]
        );
        // Un cue appelé (saut) devient « le suivant » à la place du cue juste après.
        assert_eq!(cue_status(2, Some(4), 4), CueStatus::Next);
        assert_eq!(cue_status(2, Some(4), 3), CueStatus::Later);
    }

    #[test]
    fn le_compte_a_rebours_suit_la_priorite_attente_hold_auto() {
        assert_eq!(
            countdown_text(Some((CueEntry::OnDrop, 12.0)), true, Some(5.0)).as_deref(),
            Some("entrée : au drop (attente 00:12)")
        );
        assert_eq!(
            countdown_text(None, true, Some(5.0)).as_deref(),
            Some("HOLD : avancée figée")
        );
        assert_eq!(
            countdown_text(None, false, Some(65.0)).as_deref(),
            Some("auto dans 01:05")
        );
        assert_eq!(countdown_text(None, false, None), None);
    }

    #[test]
    fn un_nom_vide_devient_dj_n() {
        assert_eq!(dj_name("KRMN", 1), "KRMN");
        assert_eq!(dj_name("  ", 2), "DJ 3");
    }
}
