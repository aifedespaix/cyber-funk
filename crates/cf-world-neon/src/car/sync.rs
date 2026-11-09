//! La synchronisation de la voiture : elle ne se « regonfle » pas, elle a déjà sa forme et se **synchronise**, comme un
//! avatar qui se charge dans la Matrice ou l'Animus d'Assassin's Creed.
//!
//! La présence `p` (0 à 1) déroule le film suivant ; l'extinction (1 → 0) le joue à l'envers :
//!
//! ```text
//! p   0 ─────── 0,2 ──────────────── 0,75 ──────── 1
//!     absente    │ squelette           │ lumières     │ synchronisée
//!                └ lignes lumineuses, glitch fort     │
//!                  carrosserie noire ── couleurs ─────┘
//! ```
//!
//! - **squelette** : un calque additif (`LightKind::Skeleton`) redessine tous les maillages en lignes de balayage, arêtes
//!   et silhouette, avec des bandes qui glissent (glitch) ; il s'efface à la fin ;
//! - **couleurs** : la carrosserie passe du noir à sa teinte, par paliers (numérique, pas un fondu mou) ;
//! - **lumières** : néons et optiques arrivent en dernier.
//!
//! Tout est géométrique ou local : aucune variation plein écran, rien à faire passer par le limiteur photosensible
//! (règle 7). En mode calme (`reduce_motion`) le glitch est nul : le film se déroule sans bégaiement.
//!
//! La voiture n'est jamais créée ni masquée : elle porte le foyer du streaming (`StreamFocus`). Absente (`p = 0`), elle
//! reste dans la scène, écrasée à une échelle invisible, pour que ses pipelines restent chauds (règle 8 de `CLAUDE.md`).

use bevy::prelude::*;

use super::rig::{Car, CarRig, CarState};
use super::systems::CarControls;
use crate::car::materials::CarLightMaterial;

/// Échelle plancher : assez petite pour disparaître (moins d'un millième de pixel), assez grande pour que la voiture
/// reste dessinée et ses pipelines chauds.
pub const MIN_SCALE: f32 = 1e-4;

/// Amplitude du bégaiement de largeur de la voiture en plein glitch (part de la largeur).
const STUTTER: f32 = 0.07;
/// Cadence du bégaiement (Hz), quantifiée : bien sous le seuil de 3 flashs par seconde pour la luminance, et le bégaiement
/// est géométrique.
const STUTTER_HZ: f32 = 11.0;
/// Nombre de paliers de la couleur de la carrosserie.
const COLOR_STEPS: f32 = 7.0;

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Ce que la présence `p` donne à voir de la voiture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SyncLook {
    /// Le calque squelette, 0 (absent) à 1 (plein).
    pub skeleton: f32,
    /// Part de couleur de la carrosserie, 0 (noire) à 1 (sa teinte), par paliers.
    pub body: f32,
    /// Part des lumières (néons, optiques), 0 à 1.
    pub lights: f32,
    /// Force du glitch, 0 à 1 (nulle en mode calme).
    pub glitch: f32,
}

impl SyncLook {
    /// Voiture synchronisée : rien de plus à voir.
    pub const DONE: Self = Self {
        skeleton: 0.0,
        body: 1.0,
        lights: 1.0,
        glitch: 0.0,
    };

    /// Le calque squelette est-il visible ?
    #[must_use]
    pub fn skeleton_visible(&self) -> bool {
        self.skeleton > 0.0
    }
}

/// La synchronisation pour une présence `p` ; `calm` coupe le glitch.
#[must_use]
pub fn sync_look(p: f32, calm: bool) -> SyncLook {
    let p = if p.is_finite() { p.clamp(0.0, 1.0) } else { 1.0 };
    if p >= 1.0 {
        return SyncLook::DONE;
    }
    let skeleton = smooth(p / 0.2) * (1.0 - smooth((p - 0.8) / 0.2));
    // Couleur : de 0,2 à 0,85, par paliers, jamais en arrière.
    let raw = smooth((p - 0.2) / 0.65);
    let body = ((raw * COLOR_STEPS).floor() / COLOR_STEPS).min(raw);
    let lights = smooth((p - 0.75) / 0.25);
    // Fort pendant le squelette, il retombe quand la couleur arrive.
    let glitch = if calm { 0.0 } else { 1.0 - smooth((p - 0.3) / 0.7) };
    SyncLook {
        skeleton,
        body,
        lights,
        glitch,
    }
}

/// L'échelle de la voiture : pleine dès qu'elle est présente (elle a déjà sa forme), avec un bégaiement de largeur
/// quantifié pendant le glitch. `time_s` : l'horloge, `p` : la présence.
#[must_use]
pub fn sync_scale(p: f32, glitch: f32, time_s: f64) -> Vec3 {
    if p <= 0.0 || !p.is_finite() {
        return Vec3::splat(MIN_SCALE);
    }
    let step = (time_s * f64::from(STUTTER_HZ)).floor() as i64;
    // Hasard déterministe de l'instant : aucune horloge cachée.
    let h = (step.wrapping_mul(7919).rem_euclid(1000)) as f32 / 1000.0 - 0.5;
    let x = 1.0 + STUTTER * h * glitch.clamp(0.0, 1.0) * 2.0;
    Vec3::new(x, 1.0, x)
}

/// Pose le film de synchronisation sur la voiture : calque squelette (visibilité et uniformes), état lu par la
/// peinture et les néons.
pub(super) fn apply_look(
    rig: &CarRig,
    state: &mut CarState,
    look: SyncLook,
    time_s: f32,
    lights: &mut Assets<CarLightMaterial>,
    visibility: &mut Query<&mut Visibility>,
) {
    let was_visible = state.sync.skeleton_visible();
    state.sync = look;
    let shown = if look.skeleton_visible() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for entity in &rig.skeleton_parts {
        if let Ok(mut v) = visibility.get_mut(*entity) {
            v.set_if_neq(shown);
        }
    }
    // Synchronisée, la voiture ne réécrit plus rien : le matériau n'est touché que pendant le film.
    if !look.skeleton_visible() && !was_visible {
        return;
    }
    if let Some(mut material) = lights.get_mut(&rig.skeleton) {
        // La teinte du squelette est celle du moment, poussée au-dessus de 1 pour que le bloom la fasse rayonner.
        let glow = state.look.map_or(LinearRgba::rgb(0.3, 0.9, 1.0), |l| l.glow);
        let tint = LinearRgba::rgb(glow.red * 1.8, glow.green * 1.8, glow.blue * 1.8);
        material.set_skeleton(tint, look.skeleton, look.glitch, time_s % 600.0);
    }
}

/// `CfSet::Simulate`, après `drive_car` : applique la présence à la voiture réelle.
pub(super) fn apply_sync(
    time: Res<Time>,
    controls: Res<CarControls>,
    mut cars: Query<(&CarRig, &mut CarState, &mut Transform), With<Car>>,
    mut lights: ResMut<Assets<CarLightMaterial>>,
    mut visibility: Query<&mut Visibility>,
) {
    for (rig, mut state, mut transform) in &mut cars {
        let look = sync_look(controls.presence, controls.calm);
        let scale = sync_scale(controls.presence, look.glitch, time.elapsed_secs_f64());
        if transform.scale != scale {
            transform.scale = scale;
        }
        apply_look(rig, &mut state, look, time.elapsed_secs(), &mut lights, &mut visibility);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_one_is_done() {
        assert_eq!(sync_look(1.0, false), SyncLook::DONE);
        assert_eq!(sync_scale(1.0, 0.0, 3.0), Vec3::ONE);
    }

    #[test]
    fn absent_car_is_a_point_but_a_present_one_has_its_full_shape() {
        assert_eq!(sync_scale(0.0, 1.0, 1.0), Vec3::splat(MIN_SCALE));
        let tiny = sync_scale(0.01, 0.0, 1.0);
        assert!((tiny.x - 1.0).abs() < 1e-6 && (tiny.y - 1.0).abs() < 1e-6, "{tiny:?}");
    }

    #[test]
    fn the_skeleton_comes_first_then_the_colours_then_the_lights() {
        let early = sync_look(0.15, false);
        assert!(early.skeleton > 0.4 && early.body == 0.0 && early.lights == 0.0, "{early:?}");
        let mid = sync_look(0.5, false);
        assert!(mid.skeleton > 0.99 && mid.body > 0.2 && mid.body < 0.9 && mid.lights == 0.0, "{mid:?}");
        let late = sync_look(0.9, false);
        assert!(late.body > 0.9 && late.lights > 0.5 && late.skeleton <= 0.5, "{late:?}");
    }

    #[test]
    fn calm_mode_cancels_the_glitch_and_the_stutter() {
        for k in 0..=100 {
            let look = sync_look(k as f32 / 100.0, true);
            assert_eq!(look.glitch, 0.0);
            assert_eq!(sync_scale(k as f32 / 100.0 + 0.01, look.glitch, 2.7).x, 1.0);
        }
    }

    #[test]
    fn the_film_is_monotonic_and_bounded() {
        let mut prev = sync_look(0.0, false);
        for k in 1..=400 {
            let l = sync_look(k as f32 / 400.0, false);
            for v in [l.skeleton, l.body, l.lights, l.glitch] {
                assert!((0.0..=1.0).contains(&v), "{l:?}");
            }
            assert!(l.body >= prev.body - 1e-6 && l.lights >= prev.lights - 1e-6, "{prev:?} -> {l:?}");
            assert!(l.glitch <= prev.glitch + 1e-6, "{prev:?} -> {l:?}");
            prev = l;
        }
    }

    #[test]
    fn colour_steps_are_discrete() {
        let mut seen = std::collections::BTreeSet::new();
        for k in 0..=1000 {
            seen.insert((sync_look(k as f32 / 1000.0, false).body * 1000.0).round() as i32);
        }
        assert!(seen.len() <= COLOR_STEPS as usize + 3, "{}", seen.len());
    }

    #[test]
    fn non_finite_presence_is_treated_as_done() {
        assert_eq!(sync_look(f32::NAN, false), SyncLook::DONE);
    }

    #[test]
    fn stutter_stays_small_and_geometric() {
        for step in 0..200 {
            let s = sync_scale(0.5, 1.0, f64::from(step) * 0.03);
            assert!((s.x - 1.0).abs() <= STUTTER + 1e-6 && s.y == 1.0, "{s:?}");
        }
    }
}
