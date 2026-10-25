//! Outils partagés par les événements du lot A : tirages, enveloppes, teintes du cycle et véhicules « scénarisés ».
//!
//! Un véhicule scénarisé (`Motion::Scripted`) n'est plus avancé par la flotte : l'événement écrit sa pose à chaque image,
//! avant la mise à jour de la flotte. C'est le pendant du `craft.step` de Sillage, sans fermeture.

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};

use crate::pilot::rand::Rand;
use crate::traffic::event::EventCtx;
use crate::traffic::fleet::CraftId;
use crate::traffic::motion::Motion;

/// Rampe lisse de `a` à `b` (`smooth` de `context.ts`) : 0 avant `a`, 1 après `b`.
pub(crate) fn smooth(a: f32, b: f32, v: f32) -> f32 {
    let k = ((v - a) / (b - a)).clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

/// Enveloppe d'un événement : monte sur `fade_s` secondes au début, descend sur les dernières `fade_s` secondes de `life_s`.
pub(crate) fn envelope(age: f32, life_s: f32, fade_s: f32) -> f32 {
    (age / fade_s).min(1.0) * ((life_s - age) / fade_s).min(1.0)
}

/// Un indice tiré dans `0..len` (`pick` de `context.ts`). `len` doit être non nul.
pub(crate) fn index(rng: &mut Rand, len: usize) -> usize {
    ((rng.unit() * len as f64) as usize).min(len.saturating_sub(1))
}

/// Un flottant tiré dans `[min, max)`.
pub(crate) fn range(rng: &mut Rand, min: f32, max: f32) -> f32 {
    rng.range(f64::from(min), f64::from(max)) as f32
}

/// Une teinte du cycle de la palette. Les teintes nommées de Sillage (`amber`, `azure`…) ne sont pas dans
/// [`EventCtx`] : on prend celles du cycle, par position.
pub(crate) fn tone(ctx: &EventCtx<'_>, k: usize) -> LinearRgba {
    ctx.frame.cycle[k % ctx.frame.cycle.len()]
}

/// Une teinte du cycle, au hasard (`pick(ctx.cycle)`).
pub(crate) fn pick_tone(ctx: &mut EventCtx<'_>) -> LinearRgba {
    let k = index(ctx.rng, ctx.frame.cycle.len());
    ctx.frame.cycle[k]
}

/// Mélange linéaire de deux couleurs linéaires (`Color.lerp`).
pub(crate) fn lerp_color(a: LinearRgba, b: LinearRgba, t: f32) -> LinearRgba {
    LinearRgba::new(
        a.red + (b.red - a.red) * t,
        a.green + (b.green - a.green) * t,
        a.blue + (b.blue - a.blue) * t,
        1.0,
    )
}

/// Prend un véhicule du pool, scénarisé à `position` avec la vitesse `velocity`. `None` si le pool est plein.
pub(crate) fn spawn_scripted(
    ctx: &mut EventCtx<'_>,
    variant: u8,
    position: DVec3,
    velocity: Vec3,
    tint: LinearRgba,
    scale: f32,
) -> Option<CraftId> {
    let id = ctx.fleet.spawn(variant, position, velocity, tint, scale, ctx.rng)?;
    if let Some(craft) = ctx.fleet.get_mut(id) {
        craft.motion = Motion::Scripted;
    }
    Some(id)
}

/// Écrit la pose d'un véhicule scénarisé. Faux s'il n'est plus vivant (la flotte l'a recyclé).
pub(crate) fn set_pose(ctx: &mut EventCtx<'_>, id: CraftId, position: DVec3, velocity: Vec3) -> bool {
    let Some(craft) = ctx.fleet.get_mut(id) else {
        return false;
    };
    craft.position = position;
    craft.velocity = velocity;
    true
}

/// Vrai si le véhicule `id` est encore vivant.
pub(crate) fn alive(ctx: &EventCtx<'_>, id: Option<CraftId>) -> bool {
    id.is_some_and(|id| ctx.fleet.get(id).is_some())
}

/// Rend un véhicule au pool, sans effet s'il est déjà libre.
pub(crate) fn release(ctx: &mut EventCtx<'_>, id: Option<CraftId>) {
    if let Some(id) = id {
        ctx.fleet.release(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_enveloppe_monte_puis_descend() {
        assert_eq!(envelope(0.0, 10.0, 2.0), 0.0);
        assert!((envelope(5.0, 10.0, 2.0) - 1.0).abs() < 1e-6);
        assert!(envelope(9.5, 10.0, 2.0) < 0.5);
    }

    #[test]
    fn l_indice_reste_dans_les_bornes() {
        let mut rng = Rand::new(7);
        for _ in 0..1000 {
            assert!(index(&mut rng, 6) < 6);
        }
    }
}
