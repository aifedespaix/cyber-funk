//! La présence de la voiture : elle n'« apparaît » jamais à l'image. Elle arrive.
//!
//! La mise en scène (voir `run/timeline.rs`) fait toujours changer la présence loin de l'objectif ou hors champ : à
//! l'ouverture la voiture s'allume derrière la caméra puis entre dans l'image, à la clôture elle file vers le point de
//! fuite, à la passation la nouvelle arrive de loin derrière et l'ancienne décroche. Le film de la présence `p` (0 à 1)
//! prolonge donc ce que l'œil attend d'un objet lointain ; l'extinction (1 → 0) le joue à l'envers :
//!
//! ```text
//! p   0 ───────── 0,3 ───────── 0,6 ────────── 1
//!     absente      │ à sa taille  │ à sa couleur  │ pleinement là
//!     ├ grandit depuis un point (vue de loin : elle approche)
//!                  ├ la carrosserie sort du noir, en fondu
//!                        ├ néons puis optiques montent ──────┤
//! ```
//!
//! - **taille** : une échelle uniforme autour du point de contact au sol. De loin, grandir ou rapetisser se lit comme
//!   approcher ou s'éloigner, jamais comme un effet ;
//! - **couleurs** : la carrosserie passe du noir à sa teinte, en fondu continu ;
//! - **lumières** : néons et optiques arrivent en dernier (et partent en premier).
//!
//! Plus de calque « squelette », de lignes de balayage ni de bégaiement : rien ne se superpose à la voiture. Tout est
//! lent et local : aucune variation plein écran, rien à faire passer par le limiteur photosensible (règle 7).
//!
//! La voiture n'est jamais créée ni masquée : elle porte le foyer du streaming (`StreamFocus`). Absente (`p = 0`), elle
//! reste dans la scène, écrasée à une échelle invisible, pour que ses pipelines restent chauds (règle 8 de `CLAUDE.md`).

use bevy::prelude::*;

use super::rig::{Car, CarState};
use super::systems::CarControls;

/// Échelle plancher : assez petite pour disparaître (moins d'un millième de pixel), assez grande pour que la voiture
/// reste dessinée et ses pipelines chauds.
pub const MIN_SCALE: f32 = 1e-4;

/// Présence à laquelle la voiture a atteint sa taille.
const FULL_SIZE_AT: f32 = 0.3;
/// Bornes de présence du fondu de la carrosserie.
const BODY: [f32; 2] = [0.1, 0.6];
/// Présence à partir de laquelle les lumières montent.
const LIGHTS_FROM: f32 = 0.3;

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Ce que la présence `p` donne à voir de la voiture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SyncLook {
    /// Taille, de 0 (un point) à 1.
    pub size: f32,
    /// Part de couleur de la carrosserie, 0 (noire) à 1 (sa teinte).
    pub body: f32,
    /// Part des lumières (néons, optiques), 0 à 1.
    pub lights: f32,
}

impl SyncLook {
    /// Voiture pleinement là : rien de plus à voir.
    pub const DONE: Self = Self {
        size: 1.0,
        body: 1.0,
        lights: 1.0,
    };
}

/// Le film de la présence `p`. `_calm` est gardé pour les appelants : le film n'a plus rien d'agité à couper.
#[must_use]
pub fn sync_look(p: f32, _calm: bool) -> SyncLook {
    let p = if p.is_finite() { p.clamp(0.0, 1.0) } else { 1.0 };
    if p >= 1.0 {
        return SyncLook::DONE;
    }
    SyncLook {
        size: smooth(p / FULL_SIZE_AT),
        body: smooth((p - BODY[0]) / (BODY[1] - BODY[0])),
        lights: smooth((p - LIGHTS_FROM) / (1.0 - LIGHTS_FROM)),
    }
}

/// L'échelle de la voiture pour une présence `p` : uniforme, d'un point à sa taille.
#[must_use]
pub fn sync_scale(p: f32) -> Vec3 {
    if p <= 0.0 || !p.is_finite() {
        return Vec3::splat(MIN_SCALE);
    }
    Vec3::splat(sync_look(p, false).size.max(MIN_SCALE))
}

/// `CfSet::Simulate`, après `drive_car` : applique la présence à la voiture réelle.
pub(super) fn apply_sync(controls: Res<CarControls>, mut cars: Query<(&mut CarState, &mut Transform), With<Car>>) {
    for (mut state, mut transform) in &mut cars {
        let look = sync_look(controls.presence, controls.calm);
        let scale = sync_scale(controls.presence);
        if transform.scale != scale {
            transform.scale = scale;
        }
        if state.sync != look {
            state.sync = look;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_one_is_done() {
        assert_eq!(sync_look(1.0, false), SyncLook::DONE);
        assert_eq!(sync_scale(1.0), Vec3::ONE);
    }

    #[test]
    fn an_absent_car_is_a_point_and_it_grows_uniformly() {
        assert_eq!(sync_scale(0.0), Vec3::splat(MIN_SCALE));
        for k in 1..=100 {
            let s = sync_scale(k as f32 / 100.0);
            assert!(
                (s.x - s.y).abs() < 1e-6 && (s.y - s.z).abs() < 1e-6,
                "jamais déformée : {s:?}"
            );
            assert!(s.x >= MIN_SCALE && s.x <= 1.0);
        }
        assert_eq!(sync_scale(FULL_SIZE_AT), Vec3::ONE);
    }

    #[test]
    fn the_size_comes_first_then_the_colours_then_the_lights() {
        let early = sync_look(0.2, false);
        assert!(early.size > 0.6 && early.body < 0.2 && early.lights == 0.0, "{early:?}");
        let mid = sync_look(0.5, false);
        assert!(mid.size > 0.999 && mid.body > 0.7 && mid.lights < 0.3, "{mid:?}");
        let late = sync_look(0.9, false);
        assert!(late.body > 0.999 && late.lights > 0.9, "{late:?}");
    }

    #[test]
    fn the_film_is_monotonic_continuous_and_bounded() {
        let mut prev = sync_look(0.0, false);
        for k in 1..=400 {
            let l = sync_look(k as f32 / 400.0, false);
            for (now, before) in [(l.size, prev.size), (l.body, prev.body), (l.lights, prev.lights)] {
                assert!((0.0..=1.0).contains(&now), "{l:?}");
                assert!(now >= before - 1e-6, "{prev:?} -> {l:?}");
                assert!(now - before < 0.02, "aucun palier : {prev:?} -> {l:?}");
            }
            prev = l;
        }
    }

    #[test]
    fn calm_mode_changes_nothing_there_is_no_glitch_left() {
        for k in 0..=100 {
            let p = k as f32 / 100.0;
            assert_eq!(sync_look(p, true), sync_look(p, false));
        }
    }

    #[test]
    fn non_finite_presence_is_treated_as_done() {
        assert_eq!(sync_look(f32::NAN, false), SyncLook::DONE);
    }
}
