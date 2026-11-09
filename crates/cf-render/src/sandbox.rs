//! Le bac à sable de l'onglet Debug : ce que l'opérateur règle pour tester un événement seul, sans le bruit du monde.
//!
//! L'interface écrit [`DebugSandbox`] ; le monde (`cf-world-neon::sandbox`) le lit et l'applique. Rien n'est sauvegardé :
//! chaque session repart du monde normal (vitesse pleine, ville présente).

use bevy::prelude::*;

/// Vitesses proposées par les boutons de l'interface (multiples de l'allure normale).
pub const SPEED_PRESETS: [(&str, f32); 5] = [
    ("Arrêt", 0.0),
    ("Rampant", 0.1),
    ("Lent", 0.3),
    ("Moitié", 0.5),
    ("Normal", 1.0),
];

/// Les réglages du bac à sable.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct DebugSandbox {
    /// Multiple de l'allure de la voiture, 0..=1 : 0 l'arrête (en freinant), 1 la laisse comme la musique la mène.
    car_speed: f32,
    /// Quartier vide : la route seule, ni immeubles ni enseignes ni graffitis.
    empty: bool,
    /// Compteur des demandes « Tout nettoyer » ; le monde réagit quand il monte.
    clear_seq: u32,
}

impl Default for DebugSandbox {
    fn default() -> Self {
        Self {
            car_speed: 1.0,
            empty: false,
            clear_seq: 0,
        }
    }
}

impl DebugSandbox {
    /// Vitesse de la voiture, 0..=1.
    #[must_use]
    pub const fn car_speed(&self) -> f32 {
        self.car_speed
    }

    /// Règle la vitesse (bornée à 0..=1 ; une valeur non finie compte pour 1).
    pub fn set_car_speed(&mut self, speed: f32) {
        self.car_speed = if speed.is_finite() { speed.clamp(0.0, 1.0) } else { 1.0 };
    }

    /// Le quartier est vide.
    #[must_use]
    pub const fn empty(&self) -> bool {
        self.empty
    }

    /// Vide (ou rend) le quartier.
    pub const fn set_empty(&mut self, empty: bool) {
        self.empty = empty;
    }

    /// Demande à tout nettoyer : arrête tous les événements en cours et retire leurs véhicules.
    pub const fn request_clear(&mut self) {
        self.clear_seq = self.clear_seq.wrapping_add(1);
    }

    /// Nombre de demandes de nettoyage depuis l'ouverture.
    #[must_use]
    pub const fn clear_seq(&self) -> u32 {
        self.clear_seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_is_clamped_and_non_finite_means_normal() {
        let mut sandbox = DebugSandbox::default();
        sandbox.set_car_speed(2.0);
        assert!((sandbox.car_speed() - 1.0).abs() < f32::EPSILON);
        sandbox.set_car_speed(-1.0);
        assert!(sandbox.car_speed().abs() < f32::EPSILON);
        sandbox.set_car_speed(f32::NAN);
        assert!((sandbox.car_speed() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn each_clear_request_raises_the_counter() {
        let mut sandbox = DebugSandbox::default();
        let before = sandbox.clear_seq();
        sandbox.request_clear();
        sandbox.request_clear();
        assert_eq!(sandbox.clear_seq(), before.wrapping_add(2));
    }
}
