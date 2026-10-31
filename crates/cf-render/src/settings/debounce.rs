//! Le délai de calme avant d'écrire les réglages : logique pure, le temps est injecté.

/// Secondes sans nouveau changement avant d'écrire.
pub const QUIET_S: f64 = 1.0;

/// Décide quand écrire. Une rafale de changements ne donne qu'une écriture ; un changement arrivé pendant une écriture
/// en programme une autre dès que la première est finie.
#[derive(Debug, Default)]
pub struct Debounce {
    /// Date du dernier changement non encore écrit.
    last_change_s: Option<f64>,
}

impl Debounce {
    /// À appeler à chaque image. `changed` : les réglages ont changé depuis l'image précédente ; `busy` : une
    /// écriture est en cours. Renvoie `true` quand il faut lancer une écriture maintenant.
    pub fn step(&mut self, changed: bool, now_s: f64, busy: bool) -> bool {
        if changed {
            self.last_change_s = Some(now_s);
        }
        match self.last_change_s {
            Some(at) if !busy && now_s - at >= QUIET_S => {
                self.last_change_s = None;
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pas_d_ecriture_sans_changement() {
        let mut debounce = Debounce::default();
        for frame in 0..600 {
            assert!(!debounce.step(false, f64::from(frame) / 60.0, false));
        }
    }

    #[test]
    fn une_rafale_donne_une_seule_ecriture() {
        let mut debounce = Debounce::default();
        let mut writes = 0;
        // Un changement toutes les 0,2 s pendant 3 s, puis le calme.
        for frame in 0..600 {
            let now = f64::from(frame) / 60.0;
            let changed = now < 3.0 && frame % 12 == 0;
            if debounce.step(changed, now, false) {
                writes += 1;
            }
        }
        assert_eq!(writes, 1);
    }

    #[test]
    fn l_ecriture_attend_le_calme() {
        let mut debounce = Debounce::default();
        assert!(!debounce.step(true, 0.0, false));
        assert!(!debounce.step(false, 0.99, false));
        assert!(debounce.step(false, 1.0, false));
    }

    #[test]
    fn un_changement_pendant_l_ecriture_en_programme_une_autre() {
        let mut debounce = Debounce::default();
        assert!(!debounce.step(true, 0.0, false));
        assert!(debounce.step(false, 1.0, false), "première écriture");
        // La première écriture dure : un nouveau changement arrive, aucune seconde écriture en parallèle.
        assert!(!debounce.step(true, 1.2, true));
        assert!(!debounce.step(false, 2.5, true));
        // Elle finit : la seconde part tout de suite, le calme est déjà écoulé.
        assert!(debounce.step(false, 2.6, false));
        assert!(!debounce.step(false, 2.7, false));
    }
}
