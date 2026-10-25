//! Le système qui joue le paquet du drop : lit le spectacle de l'image, la voiture, le mode calme.

use bevy::prelude::*;
use cf_render::{AbsolutePosition, AudioFeatures, CurrentShowFx, ScenePalette, StreamFocus};

use super::DropFx;
use super::logic::DropInput;
use crate::sky::SkyLook;

/// `CfSet::Simulate` : le paquet de l'image (`CurrentShowFx::drop`, présent sur la seule image du `drop_now`) part dans cet
/// appel même ; ondes, gerbe et limiteur avancent d'une image.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(super) fn step_dropfx(
    time: Res<Time>,
    show_fx: Option<Res<CurrentShowFx>>,
    look: Option<Res<SkyLook>>,
    features: Option<Res<AudioFeatures>>,
    palette: Option<Res<ScenePalette>>,
    car: Query<&AbsolutePosition, With<StreamFocus>>,
    mut fx: ResMut<DropFx>,
) {
    let Ok(car) = car.single() else { return };
    if let Some(palette) = palette {
        fx.set_cycle(palette.prismatic_cycle().map(|c| [c.r, c.g, c.b]));
    }
    // Mode calme (« réduire les animations ») : le paquet n'est pas joué (`at` absent dans Sillage).
    let calm = look.is_some_and(|l| l.calm);
    let input = DropInput {
        dt_s: time.delta_secs(),
        now_s: time.elapsed_secs_f64(),
        drop: show_fx.as_deref().and_then(|s| s.0.drop),
        at: (!calm).then_some(car.0),
        policy: show_fx.as_deref().map(|s| (s.0.flashes, s.0.breathing)),
        seed: features.map_or(0, |f| f.0.count),
    };
    fx.step(&input);
}
