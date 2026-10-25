//! Tests de `arcade_game`.

use super::*;

fn play(seconds: f32, kick_every: f32, with_boss_at: Option<f32>) -> ArcadeGame {
    let mut game = ArcadeGame::new(7);
    let dt = 1.0 / 60.0;
    let mut t = 0.0;
    let mut next_kick = 0.0;
    while t < seconds {
        if t >= next_kick {
            game.on_beat();
            next_kick += kick_every;
        }
        if with_boss_at.is_some_and(|at| (t - at).abs() < dt / 2.0) {
            game.start_boss();
        }
        game.advance(dt, t * 2.0, if (t * 4.0) as i32 % 2 == 0 { 0.8 } else { 0.1 });
        t += dt;
    }
    game
}

#[test]
fn la_grille_ne_contient_que_des_indices_de_palette() {
    let game = play(60.0, 0.47, Some(20.0));
    assert!(game.cells.iter().all(|c| *c <= Px::GLOW));
    assert_eq!(game.cells.len(), GRID_W * GRID_H);
}

#[test]
fn la_formation_est_dessinee_au_depart_et_le_sol_aussi() {
    let game = ArcadeGame::new(1);
    assert_eq!(game.invaders_alive(), COLS * ROWS);
    assert!(game.cells.iter().any(|c| (Px::INV_A..=Px::INV_C).contains(c)));
    assert!(game.cells[..GRID_W].contains(&Px::GROUND));
    assert!(game.cells.contains(&Px::PLAYER));
}

#[test]
fn deux_parties_de_meme_graine_jouent_pareil() {
    let a = play(30.0, 0.5, Some(10.0));
    let b = play(30.0, 0.5, Some(10.0));
    assert_eq!(a.cells, b.cells);
    let c = {
        let mut game = ArcadeGame::new(8);
        for i in 0..1800 {
            if i % 30 == 0 {
                game.on_beat();
            }
            game.advance(1.0 / 60.0, i as f32 / 30.0, 0.8);
        }
        game
    };
    assert_eq!(c.cells.len(), a.cells.len());
}

#[test]
fn un_drop_appelle_le_boss_qui_finit_par_partir() {
    let mut game = ArcadeGame::new(3);
    game.start_boss();
    assert!(game.boss_active());
    assert!((game.energy - 1.0).abs() < f32::EPSILON);
    for i in 0..(30 * 60) {
        if i % 28 == 0 {
            game.on_beat();
        }
        game.advance(1.0 / 60.0, i as f32 / 30.0, 0.7);
    }
    assert!(!game.boss_active(), "le boss est vaincu ou part au bout de 18 s");
}

#[test]
fn les_tampons_ne_depassent_jamais_leur_capacite() {
    let mut game = ArcadeGame::new(5);
    game.start_boss();
    for i in 0..(120 * 60) {
        game.on_beat();
        game.on_beat();
        game.advance(1.0 / 60.0, i as f32 / 30.0, 1.0);
        assert!(game.shots.len() <= SHOTS_MAX && game.bombs.len() <= BOMBS_MAX && game.booms.len() <= BOOMS_MAX);
        assert!(game.shots.capacity() <= SHOTS_MAX * 2, "pas de réallocation");
    }
}

#[test]
fn les_sprites_ont_les_dimensions_de_sillage() {
    let game = ArcadeGame::new(1);
    assert_eq!((game.invaders[0][0].w, game.invaders[0][0].h), (8, 6));
    assert_eq!((game.player_sprite.w, game.player_sprite.h), (9, 5));
    assert_eq!((game.boss_sprite.w, game.boss_sprite.h), (23, 11));
    assert_eq!((GRID_W, GRID_H), (GRID_W_I32 as usize, GRID_H_I32 as usize));
}
