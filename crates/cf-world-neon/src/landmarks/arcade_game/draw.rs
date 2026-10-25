//! Le dessin de la grille : la partie écrite case par case dans la texture.

use super::{ArcadeGame, BOSS_HP, Boss, COLS, GRID_H, GRID_H_I32, GRID_W, GRID_W_I32, Px};

impl ArcadeGame {
    pub(super) fn render(&mut self) {
        self.cells.fill(Px::EMPTY);
        for x in 0..GRID_W {
            self.cells[x] = if x % 4 < 3 { Px::GROUND } else { Px::EMPTY };
        }
        if let Some(boss) = self.boss {
            self.blit_boss(boss);
            let width = ((GRID_W as f32 - 12.0) * boss.hp.max(0) as f32 / BOSS_HP as f32).round() as usize;
            for x in 0..width {
                self.cells[(GRID_H - 2) * GRID_W + 6 + x] = Px::HP;
                self.cells[(GRID_H - 3) * GRID_W + 6 + x] = Px::HP;
            }
        } else {
            for i in 0..self.alive.len() {
                if !self.alive[i] {
                    continue;
                }
                let row = i / COLS;
                let kind = if row == 0 { 2 } else { usize::from(row < 3) };
                let (x, y) = (self.invader_x(i % COLS), self.invader_y(row));
                self.blit(kind, self.frame, x, y, Px::INV_A + kind as u8);
            }
        }
        if self.invulnerable == 0 || self.invulnerable % 4 < 2 {
            let x = (self.px - self.player_sprite.w as f32 / 2.0).round();
            self.blit_player(x);
        }
        for i in 0..self.shots.len() {
            let (x, y) = self.shots[i];
            self.plot(x, y, Px::SHOT, 1, 3);
        }
        for i in 0..self.bombs.len() {
            let b = self.bombs[i];
            self.plot(b.x.round(), b.y.round(), Px::BOMB, 1, 2);
        }
        for i in 0..self.booms.len() {
            let b = self.booms[i];
            self.ring(b.x, b.y, (8 - b.life) as f32 * (b.size / 5.0) + 0.6);
        }
        self.dirty = true;
    }

    pub(super) fn blit(&mut self, kind: usize, frame: usize, x0: f32, y0: f32, value: u8) {
        let (w, h) = (self.invaders[kind][frame].w, self.invaders[kind][frame].h);
        for y in 0..h {
            for x in 0..w {
                if self.invaders[kind][frame].on[y * w + x] != 0 {
                    self.plot(x0 + x as f32, y0 + y as f32, value, 1, 1);
                }
            }
        }
    }

    pub(super) fn blit_player(&mut self, x0: f32) {
        let (w, h) = (self.player_sprite.w, self.player_sprite.h);
        for y in 0..h {
            for x in 0..w {
                if self.player_sprite.on[y * w + x] != 0 {
                    self.plot(x0 + x as f32, 2.0 + y as f32, Px::PLAYER, 1, 1);
                }
            }
        }
    }

    pub(super) fn blit_boss(&mut self, boss: Boss) {
        let (w, h) = (self.boss_sprite.w, self.boss_sprite.h);
        let x0 = (boss.x - w as f32 / 2.0).round();
        let y0 = boss.y.round();
        for y in 0..h {
            for x in 0..w {
                if self.boss_sprite.on[y * w + x] != 0 {
                    self.plot(x0 + x as f32, y0 + y as f32, Px::BOSS, 1, 1);
                }
            }
        }
    }

    pub(super) fn plot(&mut self, x: f32, y: f32, value: u8, w: i32, h: i32) {
        let (bx, by) = (x.round() as i32, y.round() as i32);
        for dy in 0..h {
            for dx in 0..w {
                let (cx, cy) = (bx + dx, by + dy);
                if (0..GRID_W_I32).contains(&cx) && (0..GRID_H_I32).contains(&cy) {
                    self.cells[cy as usize * GRID_W + cx as usize] = value;
                }
            }
        }
    }

    pub(super) fn ring(&mut self, x: f32, y: f32, r: f32) {
        let reach = r.ceil() as i32 + 1;
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let d = (dx.abs() + dy.abs()) as f32;
                if d > r - 1.4 && d <= r + 0.3 {
                    self.plot(x + dx as f32, y + dy as f32, Px::BOOM, 1, 1);
                }
            }
        }
    }
}
