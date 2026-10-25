//! Le mouvement des tirs, des bombes et du boss, un pas de jeu à la fois.

use super::{ArcadeGame, BOSS_SECONDS, COLS, GRID_H, GRID_W, INV_H, INV_W, TICK};

impl ArcadeGame {
    pub(super) fn move_shots(&mut self) {
        let mut i = 0;
        while i < self.shots.len() {
            if self.advance_shot(i) {
                i += 1;
            } else {
                self.shots.remove(i);
            }
        }
    }

    /// Avance le tir `i`. Faux s'il est détruit (touché ou sorti).
    pub(super) fn advance_shot(&mut self, i: usize) -> bool {
        self.shots[i].1 += 3.0;
        let (sx, sy) = self.shots[i];
        let boss_height = self.boss_sprite.h as f32;
        let hit_boss = self
            .boss
            .is_some_and(|boss| (sx - boss.x).abs() <= 11.0 && sy >= boss.y && sy <= boss.y + boss_height);
        if hit_boss {
            if let Some(boss) = &mut self.boss {
                boss.hp -= 1;
            }
            self.explode(sx, sy, 2.0);
            return false;
        }
        for index in 0..self.alive.len() {
            if !self.alive[index] {
                continue;
            }
            let x = self.invader_x(index % COLS);
            let y = self.invader_y(index / COLS);
            if sx >= x && sx < x + INV_W && sy >= y && sy < y + INV_H {
                self.alive[index] = false;
                self.explode(x + INV_W / 2.0, y + INV_H / 2.0, 4.0);
                return false;
            }
        }
        sy < GRID_H as f32
    }

    pub(super) fn move_bombs(&mut self) {
        let mut i = 0;
        while i < self.bombs.len() {
            let mut b = self.bombs[i];
            b.x += b.vx;
            b.y += b.vy;
            self.bombs[i] = b;
            let hit = self.invulnerable == 0 && (b.x - self.px).abs() <= 4.0 && b.y <= 7.0 && b.y >= 2.0;
            if hit {
                self.explode(self.px, 4.0, 5.0);
                self.invulnerable = 36;
                self.px = GRID_W as f32 / 2.0;
            }
            if !hit && b.y > 1.0 && b.x > 0.0 && b.x < GRID_W as f32 {
                i += 1;
            } else {
                self.bombs.remove(i);
            }
        }
    }

    pub(super) fn update_boss(&mut self) {
        let Some(mut boss) = self.boss else { return };
        boss.age += TICK;
        // Période de quatre temps : le balancier reste calé sur le tempo.
        boss.x = GRID_W as f32 / 2.0 + (self.clock * std::f32::consts::PI / 2.0).sin() * (GRID_W as f32 / 2.0 - 14.0);
        boss.y = 31.0 + (self.clock * std::f32::consts::PI).sin() * 2.0;
        if boss.hp <= 0 {
            for _ in 0..7 {
                let x = boss.x + (self.rng.unit() as f32 - 0.5) * 20.0;
                let y = boss.y + self.rng.unit() as f32 * self.boss_sprite.h as f32;
                let size = 3.0 + (self.rng.unit() * 3.0).floor() as f32;
                self.explode(x, y, size);
            }
            self.energy = 1.0;
            self.boss = None;
            self.wave_delay = 20;
        } else if boss.age > BOSS_SECONDS {
            self.boss = None;
            self.new_wave();
        } else {
            self.boss = Some(boss);
        }
    }
}
