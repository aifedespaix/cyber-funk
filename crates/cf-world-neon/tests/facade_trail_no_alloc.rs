//! Aucune allocation par image en régime établi (règle 4) : la traîne de la voiture (écriture d'un point, calcul des
//! uniformes du shader) tourne sous `assert_no_alloc`, qui fait avorter le processus à la moindre allocation.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use bevy::math::{DVec3, Vec3};
use cf_world_neon::facade::CarTrail;

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const DT: f64 = 1.0 / 60.0;

#[test]
fn trail_update_does_not_allocate() {
    let mut trail = CarTrail::default();
    // La voiture roule à 60 m/s sur une route qui tourne, à 400 km du départ, puis s'arrête.
    let position = |i: u32| {
        let t = f64::from(i) * DT;
        DVec3::new((t * 0.2).sin() * 300.0, 0.0, -400_000.0 - t * 60.0)
    };
    let mut lit = 0.0_f32;
    for i in 0..(60 * 120) {
        let now = f64::from(i) * DT;
        let at = if i < 60 * 90 { position(i) } else { position(60 * 90) };
        assert_no_alloc(|| {
            trail.record(at, now);
            trail.set_tint(Vec3::new(0.1, 0.8, 1.0), 1.0);
            let params = trail.params(DVec3::new(0.0, 0.0, -400_000.0), now);
            lit = lit.max(params.iter().map(|v| v.w).fold(0.0, f32::max));
        });
    }
    assert_eq!(trail.len(), 16, "la traîne a vécu");
    assert!(lit > 0.9, "des points lumineux ont été produits : {lit}");
}
