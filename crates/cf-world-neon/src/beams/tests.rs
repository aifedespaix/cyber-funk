//! Les projecteurs dans une application Bevy sans rendu : points d'attache de la ville, pool d'entités, sécurité du pouls.

use bevy::math::DVec3;
use cf_render::{AbsolutePosition, CfSet, CurrentShowFx, ShowFxInput, StreamFocus};

use super::*;
use crate::NeonWorld;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>()
        .configure_sets(
            Update,
            (
                CfSet::Input,
                CfSet::Show,
                CfSet::Simulate,
                CfSet::Direct,
                CfSet::Present,
            )
                .chain(),
        )
        .insert_resource(NeonWorld::default())
        .add_plugins(BeamsPlugin);
    app.world_mut()
        .spawn((AbsolutePosition(DVec3::new(0.0, 0.0, -5000.0)), StreamFocus));
    app
}

fn tick(app: &mut App, frames: usize) {
    for _ in 0..frames {
        std::thread::sleep(std::time::Duration::from_millis(2));
        app.update();
    }
}

fn fx(flashes: bool) -> CurrentShowFx {
    CurrentShowFx(ShowFxInput {
        flashes,
        breathing: false,
        heat: 0.5,
        time_scale: 1.0,
        cue_seq: 0,
        fx_rgb_split: 0.0,
        fx_flash: 0.0,
        drop: None,
    })
}

#[test]
fn la_ville_fournit_des_balayeurs_et_le_pool_d_entites_les_montre() {
    let mut app = app();
    tick(&mut app, 3);
    let count = app.world().resource::<Beams>().field().slots().len();
    assert!(count > 10 && count <= MAX_BEAMS, "{count} projecteurs");
    let mut pieces = app.world_mut().query::<(&BeamPiece, &Visibility)>();
    let (mut cones, mut pools, mut total) = (0, 0, 0);
    for (piece, visibility) in pieces.iter(app.world()) {
        total += 1;
        if *visibility == Visibility::Visible {
            match piece {
                BeamPiece::Cone(_) => cones += 1,
                BeamPiece::Pool(_) => pools += 1,
            }
        }
    }
    assert_eq!(total, MAX_BEAMS * 2, "pool préalloué : deux pièces par projecteur");
    assert_eq!((cones, pools), (count, count));
}

#[test]
fn le_nombre_d_entites_ne_change_pas_en_regime_etabli() {
    let mut app = app();
    tick(&mut app, 3);
    let count = |app: &mut App| app.world_mut().query::<Entity>().iter(app.world()).count();
    let before = count(&mut app);
    // La voiture avance de dix rangées : d'autres tours, d'autres projecteurs, pas une entité de plus.
    for step in 1..=10 {
        let mut cars = app
            .world_mut()
            .query_filtered::<&mut AbsolutePosition, With<StreamFocus>>();
        for mut car in cars.iter_mut(app.world_mut()) {
            car.0.z = -5000.0 - 26.0 * f64::from(step);
        }
        tick(&mut app, 2);
    }
    assert_eq!(count(&mut app), before, "rien n'est créé ni détruit");
}

#[test]
fn sans_ville_aucun_balayeur() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>()
        .configure_sets(
            Update,
            (
                CfSet::Input,
                CfSet::Show,
                CfSet::Simulate,
                CfSet::Direct,
                CfSet::Present,
            )
                .chain(),
        )
        .insert_resource(NeonWorld::default().without_city())
        .add_plugins(BeamsPlugin);
    app.world_mut()
        .spawn((AbsolutePosition(DVec3::new(0.0, 0.0, -5000.0)), StreamFocus));
    tick(&mut app, 3);
    assert_eq!(app.world().resource::<Beams>().field().slots().len(), 0);
}

#[test]
fn le_pouls_est_nul_sans_spectacle_et_quand_les_flashs_sont_coupes() {
    for show in [None, Some(fx(false))] {
        let mut app = app();
        if let Some(show) = show {
            app.insert_resource(show);
        }
        tick(&mut app, 5);
        let beams = app.world().resource::<Beams>();
        assert_eq!(beams.pulse().pulse(), 0.0);
        assert!(!beams.pulse().permitted());
    }
}
