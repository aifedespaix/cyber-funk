//! La présence de la voiture : elle apparaît et disparaît comme une vieille télé qui s'allume ou s'éteint.
//!
//! À l'extinction (présence 1 → 0) la voiture s'écrase d'abord en une ligne horizontale, puis la ligne se rétrécit en un
//! point, puis plus rien. À l'allumage c'est le même film à l'envers. Tout est géométrique : aucun flash, donc rien à
//! faire passer par le limiteur photosensible (règle 7).
//!
//! La voiture n'est jamais créée ni détruite : elle porte le foyer du streaming (`StreamFocus`). Elle n'est pas non plus
//! masquée (`Visibility::Hidden`) : une voiture jamais dessinée ne compilerait ses pipelines qu'au premier `Démarrer`
//! (règle 8 de `CLAUDE.md`, aucune compilation de shader pendant un show). Éteinte, elle reste donc dans la scène,
//! écrasée à une échelle invisible.

use bevy::prelude::*;

use super::rig::Car;
use super::systems::CarControls;

/// Hauteur de la ligne, en part de la hauteur normale, au milieu de l'extinction.
const LINE: f32 = 0.04;
/// Échelle plancher : assez petite pour disparaître (moins d'un millième de pixel), assez grande pour que la voiture
/// reste dessinée et ses pipelines chauds.
pub const MIN_SCALE: f32 = 1e-4;

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// L'échelle de la voiture pour une présence `p` (0 à 1).
#[must_use]
pub fn tv_scale(p: f32) -> Vec3 {
    let p = p.clamp(0.0, 1.0);
    // 1 → 0,5 : écrasement vertical jusqu'à la ligne ; 0,5 → 0 : la ligne se referme en point.
    let height = LINE + (1.0 - LINE) * smooth((p - 0.5) / 0.5);
    let width = smooth(p / 0.5);
    Vec3::new(width, height, width)
}

/// `CfSet::Simulate`, après `drive_car` : applique la présence à la racine de la voiture.
pub(super) fn apply_presence(controls: Res<CarControls>, mut cars: Query<&mut Transform, With<Car>>) {
    let scale = tv_scale(controls.presence).max(Vec3::splat(MIN_SCALE));
    for mut transform in &mut cars {
        if transform.scale != scale {
            transform.scale = scale;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_one_is_identity() {
        assert_eq!(tv_scale(1.0), Vec3::ONE);
    }

    #[test]
    fn presence_zero_collapses_the_car_to_a_point() {
        assert_eq!(tv_scale(0.0).x, 0.0);
    }

    #[test]
    fn tv_squash_profile_goes_through_a_line_then_a_point() {
        let mid = tv_scale(0.5);
        assert!((mid.y - LINE).abs() < 1e-5 && (mid.x - 1.0).abs() < 1e-5, "{mid:?}");
        let quarter = tv_scale(0.25);
        assert!(
            (quarter.y - LINE).abs() < 1e-5 && quarter.x > 0.0 && quarter.x < 1.0,
            "{quarter:?}"
        );
    }

    #[test]
    fn scale_is_monotonic_and_bounded() {
        let mut prev = Vec3::ZERO;
        for k in 0..=200 {
            let s = tv_scale(k as f32 / 200.0);
            assert!(s.x >= prev.x - 1e-6 && s.y >= prev.y - 1e-6, "{prev:?} -> {s:?}");
            assert!(s.max_element() <= 1.0 && s.min_element() >= 0.0);
            prev = s;
        }
    }

    #[test]
    fn the_car_stays_drawn_when_off_so_its_pipelines_stay_warm() {
        let mut app = App::new();
        app.insert_resource(CarControls {
            presence: 0.0,
            ..CarControls::default()
        })
        .add_systems(Update, apply_presence);
        let car = app
            .world_mut()
            .spawn((Car, Transform::default(), Visibility::default()))
            .id();
        app.update();
        let off = app.world().get::<Transform>(car).unwrap().scale;
        // Une ligne d'un dix-millième de large : invisible, mais toujours dessinée.
        assert!(
            off.x <= MIN_SCALE + f32::EPSILON && off.z <= MIN_SCALE + f32::EPSILON && off.min_element() > 0.0,
            "{off:?}"
        );
        assert_eq!(*app.world().get::<Visibility>(car).unwrap(), Visibility::Inherited);
        app.world_mut().resource_mut::<CarControls>().presence = 1.0;
        app.update();
        assert_eq!(app.world().get::<Transform>(car).unwrap().scale, Vec3::ONE);
    }
}
