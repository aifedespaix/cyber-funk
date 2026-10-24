//! Les boutons à maintien de la vue Live (Blackout tenu au clavier).
//!
//! Règle : un maintien ne reste jamais enclenché quand son contexte disparaît. Changement de profil de contrôle ou de
//! DJ (`SlotLoaded`, début de passation), retour en Préparation, fenêtre qui perd le focus, champ texte actif : le
//! relâchement est émis tout de suite, et la touche, si elle est encore enfoncée, doit être relâchée puis repressée
//! avant de rearmer (sinon un Blackout se rallumerait tout seul après la passation).

use cf_control::Switch;

/// Suit un bouton à maintien : ce que l'on a enclenché, donc ce que l'on doit relâcher.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HoldTracker {
    /// Nous avons enclenché la bascule et ne l'avons pas encore relâchée.
    held: bool,
    /// Relâchement forcé alors que la touche est encore enfoncée : on attend qu'elle remonte.
    wait_release: bool,
}

impl HoldTracker {
    /// Le bouton est-il enclenché par nous ?
    #[cfg(test)]
    pub(crate) fn is_held(&self) -> bool {
        self.held
    }

    /// Une image : `down` = la touche est enfoncée, `allowed` = le contexte permet le maintien (mode Live, fenêtre
    /// active, pas de saisie de texte), `already_on` = la bascule est déjà allumée (par le bouton à verrou ou le
    /// MIDI : on ne la prend pas en charge, la relâcher couperait une demande qui n'est pas la nôtre).
    ///
    /// Rend l'ordre à envoyer à cette image, s'il y en a un.
    pub(crate) fn update(&mut self, down: bool, allowed: bool, already_on: bool) -> Option<Switch> {
        if self.held {
            if down && allowed {
                return None;
            }
            self.held = false;
            self.wait_release = down;
            return Some(Switch::Off);
        }
        if !down {
            self.wait_release = false;
            return None;
        }
        if self.wait_release || !allowed || already_on {
            return None;
        }
        self.held = true;
        Some(Switch::On)
    }

    /// Relâchement forcé (changement de profil ou de DJ). `down` : la touche est encore enfoncée.
    pub(crate) fn force_release(&mut self, down: bool) -> Option<Switch> {
        if !self.held {
            return None;
        }
        self.held = false;
        self.wait_release = down;
        Some(Switch::Off)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appui_puis_relachement_donne_on_puis_off() {
        let mut hold = HoldTracker::default();
        assert_eq!(hold.update(false, true, false), None);
        assert_eq!(hold.update(true, true, false), Some(Switch::On));
        assert!(hold.is_held());
        assert_eq!(hold.update(true, true, true), None, "tenu : rien de plus");
        assert_eq!(hold.update(false, true, true), Some(Switch::Off));
        assert!(!hold.is_held());
    }

    #[test]
    fn un_changement_de_dj_relache_et_exige_un_nouvel_appui() {
        let mut hold = HoldTracker::default();
        assert_eq!(hold.update(true, true, false), Some(Switch::On));
        // Passation : la touche est encore enfoncée.
        assert_eq!(hold.force_release(true), Some(Switch::Off));
        assert_eq!(hold.update(true, true, false), None, "ne se rearme pas tout seul");
        assert_eq!(hold.update(false, true, false), None);
        assert_eq!(hold.update(true, true, false), Some(Switch::On), "nouvel appui");
    }

    #[test]
    fn perdre_le_contexte_relache() {
        let mut hold = HoldTracker::default();
        assert_eq!(hold.update(true, true, false), Some(Switch::On));
        assert_eq!(hold.update(true, false, true), Some(Switch::Off), "fenêtre sans focus");
        assert_eq!(hold.update(true, true, false), None);
    }

    #[test]
    fn une_bascule_deja_allumee_n_est_pas_prise_en_charge() {
        let mut hold = HoldTracker::default();
        assert_eq!(hold.update(true, true, true), None);
        assert!(!hold.is_held());
        assert_eq!(hold.force_release(false), None);
    }
}
