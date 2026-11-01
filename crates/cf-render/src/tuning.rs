//! Les deux curseurs de l'opérateur pour les façades, vus de la fenêtre opérateur.
//!
//! Comme [`crate::CarSelection`] : la fenêtre opérateur ne connaît pas le monde (`cf-world-neon`). Elle écrit cette
//! ressource, le monde la recopie dans ses propres réglages, et les réglages sauvegardés la retiennent.

use bevy::prelude::*;

/// Réglages des façades faits à la main, de 0 à 1 (1 : plein effet, comme avant ces curseurs).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct FacadeTuning {
    /// Force de la traîne lumineuse de la voiture sur les façades.
    pub car_follow: f32,
    /// Part des fenêtres qui ont leur vie propre (télé, extinction lente, néon).
    pub window_life: f32,
}

impl Default for FacadeTuning {
    fn default() -> Self {
        Self {
            car_follow: 1.0,
            window_life: 1.0,
        }
    }
}

impl FacadeTuning {
    /// Les mêmes réglages bornés à 0..1 (un fichier de réglages abîmé ne sort jamais de la plage).
    #[must_use]
    pub fn clamped(self) -> Self {
        let fix = |v: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { 1.0 };
        Self {
            car_follow: fix(self.car_follow),
            window_life: fix(self.window_life),
        }
    }
}

/// Préfixe, dans `collapsed_panels` des réglages sauvegardés, de la force de la traîne (pourcentage entier).
pub const CAR_FOLLOW_KEY: &str = "tuning:car-follow:";
/// Préfixe de la vie des fenêtres (pourcentage entier).
pub const WINDOW_LIFE_KEY: &str = "tuning:window-life:";

fn percent(entries: &[String], prefix: &str) -> Option<f32> {
    let n: u32 = entries.iter().find_map(|e| e.strip_prefix(prefix))?.parse().ok()?;
    Some(n.min(100) as f32 / 100.0)
}

/// Les réglages retenus dans la liste des panneaux ; une valeur absente ou illisible vaut 1.
#[must_use]
pub fn tuning_from_entries(entries: &[String]) -> FacadeTuning {
    FacadeTuning {
        car_follow: percent(entries, CAR_FOLLOW_KEY).unwrap_or(1.0),
        window_life: percent(entries, WINDOW_LIFE_KEY).unwrap_or(1.0),
    }
}

/// Écrit les réglages dans la liste (remplace les anciennes entrées). Renvoie vrai si la liste a changé.
pub fn store_tuning(entries: &mut Vec<String>, tuning: FacadeTuning) -> bool {
    let t = tuning.clamped();
    let mut next: Vec<String> = entries
        .iter()
        .filter(|e| !e.starts_with(CAR_FOLLOW_KEY) && !e.starts_with(WINDOW_LIFE_KEY))
        .cloned()
        .collect();
    // Seuls les écarts au défaut sont écrits : un fichier intact reste intact.
    if t.car_follow < 1.0 {
        next.push(format!("{CAR_FOLLOW_KEY}{}", (t.car_follow * 100.0).round() as u32));
    }
    if t.window_life < 1.0 {
        next.push(format!("{WINDOW_LIFE_KEY}{}", (t.window_life * 100.0).round() as u32));
    }
    let changed = next != *entries;
    if changed {
        *entries = next;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_the_saved_list() {
        let mut entries = vec!["debug-open:trafic".to_owned()];
        let tuning = FacadeTuning {
            car_follow: 0.35,
            window_life: 0.0,
        };
        assert!(store_tuning(&mut entries, tuning));
        assert!(entries.contains(&"debug-open:trafic".to_owned()));
        assert_eq!(tuning_from_entries(&entries), tuning);
        assert!(!store_tuning(&mut entries, tuning), "rien à réécrire");
        assert!(store_tuning(&mut entries, FacadeTuning::default()));
        assert_eq!(entries, vec!["debug-open:trafic".to_owned()]);
    }

    #[test]
    fn unreadable_entries_mean_full_effect() {
        let entries = vec!["tuning:car-follow:abc".to_owned()];
        assert_eq!(tuning_from_entries(&entries), FacadeTuning::default());
        let wild = FacadeTuning {
            car_follow: f32::NAN,
            window_life: 7.0,
        };
        assert_eq!(wild.clamped(), FacadeTuning::default());
    }
}
