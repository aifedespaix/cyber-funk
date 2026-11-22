//! Le paramètre de système qui donne aux ponts le rythme tel que le contrat le publie (`docs/rythme.md`).
//!
//! Il regroupe l'énergie (`AudioFeatures`) et les signaux filtrés (`FilteredSignals` : phase, accents, confiance de
//! grille) en un seul paramètre : un système Bevy ne dépasse pas seize paramètres, et les ponts n'ont ainsi qu'une
//! porte à franchir.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::AudioFeatures;
use super::routing::FilteredSignals;

/// L'énergie et les signaux filtrés de l'image. Les deux sont optionnels : sans pont audio ou sans routage, on lit le
/// repos (`FilteredSignals::default()`, grille sans confiance).
#[derive(SystemParam, Debug)]
pub struct RhythmInputs<'w> {
    /// L'état d'analyse de l'image (énergie, section, tempo).
    pub features: Option<Res<'w, AudioFeatures>>,
    /// Les signaux filtrés de l'image (accents, grille, confiance).
    pub signals: Option<Res<'w, FilteredSignals>>,
}

impl RhythmInputs<'_> {
    /// Les signaux filtrés de l'image, au repos s'ils n'existent pas.
    #[must_use]
    pub fn signals(&self) -> FilteredSignals {
        self.signals.as_deref().copied().unwrap_or_default()
    }
}

/// Pour les démos et les bancs d'essai **sans** pipeline audio : recopie chaque image les accents posés à la main dans
/// `AudioFeatures` (`pulse`, `kick`, `count`, `beat`, `onset`, `hat`, `transient`) vers `FilteredSignals`, que les effets
/// lisent désormais. À ne pas ajouter avec `CfAudioPlugin` : le routage publie déjà les signaux.
#[derive(Debug, Default)]
pub struct DemoRhythmPlugin;

impl Plugin for DemoRhythmPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioFeatures>()
            .init_resource::<FilteredSignals>()
            .add_systems(Update, mirror_features.in_set(crate::CfSet::Show));
    }
}

fn mirror_features(features: Res<AudioFeatures>, mut signals: ResMut<FilteredSignals>) {
    let f = &features.0;
    let mut s = FilteredSignals::at_pulse(f.pulse);
    s.kick_now = f.kick;
    s.kick_count = f.count;
    s.kick_beat = f.beat;
    s.transient_accent = f.transient;
    s.onset_accent = f.onset;
    s.hat_accent = f.hat;
    s.grid_trust = if f.live { 1.0 } else { 0.0 };
    s.beat_phase = f.phase;
    *signals = s;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_demo_plugin_mirrors_the_hand_set_accents() {
        let mut app = App::new();
        app.add_plugins(DemoRhythmPlugin);
        app.configure_sets(Update, crate::CfSet::Show);
        {
            let mut f = app.world_mut().resource_mut::<AudioFeatures>();
            f.0.pulse = 0.5;
            f.0.kick = true;
            f.0.count = 7;
        }
        app.update();
        let s = app.world().resource::<FilteredSignals>();
        assert!((s.kick_pulse() - 0.5).abs() < 1e-4);
        assert!(s.kick_now);
        assert_eq!(s.kick_count, 7);
    }
}
