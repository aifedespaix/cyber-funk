//! Les gestes en deux temps de la barre de statut (« Passer au suivant », reset) : armer, puis confirmer.
//!
//! Un double-clic ne doit pas confirmer : la confirmation apparaît à la place du bouton, elle refuse donc les clics
//! pendant [`CONFIRM_DELAY_S`]. Une confirmation oubliée ne doit pas servir plus tard : elle se désarme seule au bout de
//! [`CONFIRM_TIMEOUT_S`], et dès que son contexte change (autre DJ, autre onglet). Le temps est injecté (`now_s`,
//! secondes réelles de l'application) : rien n'est lu en dur, tout se teste sans attendre.

/// Délai avant qu'une confirmation accepte un clic, en s (plus long qu'un double-clic).
pub(crate) const CONFIRM_DELAY_S: f64 = 0.4;
/// Durée de vie d'une confirmation armée, en s.
pub(crate) const CONFIRM_TIMEOUT_S: f64 = 5.0;

/// Une confirmation en deux temps, liée à un contexte (`key` : le créneau visé, 0 sans contexte).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct Confirm {
    /// Instant de l'armement et contexte armé.
    armed: Option<(f64, u32)>,
}

impl Confirm {
    /// Armée (et pas encore expirée à la dernière mise à jour) ?
    pub(crate) fn is_armed(self) -> bool {
        self.armed.is_some()
    }

    /// La confirmation accepte-t-elle un clic à `now_s` ?
    pub(crate) fn ready(self, now_s: f64) -> bool {
        self.armed
            .is_some_and(|(at, _)| (CONFIRM_DELAY_S..CONFIRM_TIMEOUT_S).contains(&(now_s - at)))
    }

    /// Désarme.
    pub(crate) fn disarm(&mut self) {
        self.armed = None;
    }

    /// Désarme si le délai est passé ou si le contexte a changé.
    pub(crate) fn refresh(&mut self, now_s: f64, key: u32) {
        if self
            .armed
            .is_some_and(|(at, armed_key)| armed_key != key || now_s - at >= CONFIRM_TIMEOUT_S)
        {
            self.armed = None;
        }
    }

    /// Un clic : le premier arme (`false`) ; une fois armée, confirme (`true`, et désarme) seulement si elle est prête.
    /// Un clic trop tôt est ignoré et la laisse armée.
    pub(crate) fn click(&mut self, now_s: f64, key: u32) -> bool {
        self.refresh(now_s, key);
        if !self.is_armed() {
            self.armed = Some((now_s, key));
            return false;
        }
        if self.ready(now_s) {
            self.armed = None;
            return true;
        }
        false
    }
}
