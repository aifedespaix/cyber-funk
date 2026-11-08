//! Textes partagés par les onglets : horloges, noms de DJ, temps restant d'un créneau.

/// Minutes dans une journée : un créneau qui finit « avant » son début s'entend le lendemain.
const DAY_MIN: u32 = 24 * 60;

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

/// Durée d'une piste ou d'une playlist : `m:ss`, `h:mm:ss`, ou `—` quand elle est inconnue.
pub(crate) fn duration(seconds: Option<f64>) -> String {
    let Some(seconds) = seconds else {
        return "—".to_owned();
    };
    let total = if seconds.is_finite() && seconds > 0.0 {
        seconds as u64
    } else {
        0
    };
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Nom affiché d'un DJ : `DJ n` (à partir de 1) quand le nom est vide.
pub(crate) fn dj_name(name: &str, slot: usize) -> String {
    if name.trim().is_empty() {
        format!("DJ {}", slot + 1)
    } else {
        name.to_owned()
    }
}

/// Secondes restantes d'un créneau : durée prévue (fin − début, minuit compris) moins le temps écoulé depuis son
/// chargement. `None` sans horaire. Négatif quand le DJ déborde.
pub(crate) fn remaining_s(schedule: Option<(u16, u16)>, elapsed_s: f64) -> Option<f64> {
    let (start, end) = schedule?;
    let length_min = (u32::from(end) + DAY_MIN - u32::from(start)) % DAY_MIN;
    Some(f64::from(length_min) * 60.0 - elapsed_s)
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
    }

    #[test]
    fn une_duree_de_piste_se_lit_sans_zero_de_tete() {
        assert_eq!(duration(Some(242.0)), "4:02");
        assert_eq!(duration(Some(9.0)), "0:09");
        assert_eq!(duration(Some(3725.0)), "1:02:05");
        assert_eq!(duration(None), "—");
    }

    #[test]
    fn un_dj_sans_nom_s_appelle_dj_n() {
        assert_eq!(dj_name("  ", 0), "DJ 1");
        assert_eq!(dj_name("Nova", 3), "Nova");
    }

    #[test]
    fn le_temps_restant_passe_minuit_et_devient_negatif_en_debordant() {
        // 23:00 → 01:00 : deux heures.
        assert_eq!(remaining_s(Some((23 * 60, 60)), 600.0), Some(7200.0 - 600.0));
        assert_eq!(remaining_s(Some((60, 120)), 4000.0), Some(3600.0 - 4000.0));
        assert_eq!(remaining_s(None, 5.0), None);
    }
}
