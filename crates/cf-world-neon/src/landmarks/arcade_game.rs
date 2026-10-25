//! Une partie de borne d'arcade qui joue seule, au rythme de la musique (`arcade-game.ts`).
//!
//! Grille de 72 × 48 cases. Chaque kick fait avancer la formation d'envahisseurs d'un pas et lâcher une bombe ; le vaisseau
//! se replace et tire dès qu'il est aligné, et les aigus (charleston) déclenchent des tirs supplémentaires. Un drop appelle
//! un boss : il balance d'un bord à l'autre sur une période de quatre temps, répond par des éventails de bombes à chaque
//! kick, et son explosion rend la place à une nouvelle vague.
//!
//! Pure et déterministe : le hasard est injecté (graine). Aucune allocation en régime établi : les tirs, bombes et
//! explosions vivent dans des tampons de capacité fixe, la grille est écrite en place.

use self::sprites::{Sprite, boss_sprite, sprite};
use super::math::clamp01;
use crate::pilot::rand::Rand;

/// Largeur de la grille, en cases.
pub const GRID_W: usize = 72;
/// Hauteur de la grille, en cases.
pub const GRID_H: usize = 48;
/// Les mêmes, en entiers signés (le tracé accepte des coordonnées négatives, rognées au bord).
const GRID_W_I32: i32 = 72;
const GRID_H_I32: i32 = 48;

/// Indices de couleur écrits dans la grille ; le shader les convertit en palette.
mod draw;
mod motion;
mod sprites;

#[derive(Debug, Clone, Copy)]
pub struct Px;

impl Px {
    /// Case vide.
    pub const EMPTY: u8 = 0;
    /// Envahisseur de type A.
    pub const INV_A: u8 = 1;
    /// Envahisseur de type B.
    pub const INV_B: u8 = 2;
    /// Envahisseur de type C.
    pub const INV_C: u8 = 3;
    /// Vaisseau du joueur.
    pub const PLAYER: u8 = 4;
    /// Tir.
    pub const SHOT: u8 = 5;
    /// Bombe.
    pub const BOMB: u8 = 6;
    /// Explosion.
    pub const BOOM: u8 = 7;
    /// Boss.
    pub const BOSS: u8 = 8;
    /// Barre de vie du boss.
    pub const HP: u8 = 9;
    /// Sol.
    pub const GROUND: u8 = 10;
    /// Lueur.
    pub const GLOW: u8 = 11;
}

const COLS: usize = 6;
const ROWS: usize = 4;
const INV_W: f32 = 8.0;
const INV_H: f32 = 6.0;
const GAP_X: f32 = 3.0;
const GAP_Y: f32 = 3.0;
const START_Y: f32 = 41.0;
const TICK: f32 = 1.0 / 24.0;
const BOSS_HP: i32 = 26;
const BOSS_SECONDS: f32 = 18.0;
const SHOTS_MAX: usize = 16;
const BOMBS_MAX: usize = 32;
const BOOMS_MAX: usize = 32;

#[derive(Debug, Clone, Copy)]
struct Boss {
    x: f32,
    y: f32,
    hp: i32,
    age: f32,
}

#[derive(Debug, Clone, Copy)]
struct Bomb {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

#[derive(Debug, Clone, Copy)]
struct Boom {
    x: f32,
    y: f32,
    life: i32,
    size: f32,
}

/// La partie.
#[derive(Debug)]
pub struct ArcadeGame {
    /// La grille, un octet par case (voir [`Px`]), ligne 0 en bas.
    pub cells: Vec<u8>,
    /// Vrai quand la grille a changé depuis le dernier envoi au GPU.
    pub dirty: bool,
    /// Activité récente (explosions, boss) 0–1, pour les lumières.
    pub energy: f32,
    rng: Rand,
    invaders: [[Sprite; 2]; 3],
    player_sprite: Sprite,
    boss_sprite: Sprite,
    ox: f32,
    oy: f32,
    dir: f32,
    frame: usize,
    alive: [bool; COLS * ROWS],
    shots: Vec<(f32, f32)>,
    bombs: Vec<Bomb>,
    booms: Vec<Boom>,
    px: f32,
    target: f32,
    cooldown: i32,
    want_fire: bool,
    invulnerable: i32,
    wave_delay: i32,
    boss: Option<Boss>,
    clock: f32,
    acc: f32,
    beats: u32,
}

impl ArcadeGame {
    /// Une partie neuve, de graine `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let inv = |a: [&str; 6], b: [&str; 6]| [sprite(&a), sprite(&b)];
        let mut game = Self {
            cells: vec![0; GRID_W * GRID_H],
            dirty: true,
            energy: 0.0,
            rng: Rand::new(seed),
            invaders: [
                inv(
                    ["..####..", ".######.", "##.##.##", "########", ".#.##.#.", "#.#..#.#"],
                    ["..####..", ".######.", "##.##.##", "########", "..#..#..", ".#.##.#."],
                ),
                inv(
                    ["#.####.#", "########", "##.##.##", "########", ".#....#.", "#.#..#.#"],
                    [".#.##.#.", "########", "##.##.##", "########", "#.#..#.#", ".#....#."],
                ),
                inv(
                    ["..#..#..", ".######.", "########", "#.####.#", "########", ".##..##."],
                    ["..#..#..", ".######.", "########", "#.####.#", "########", "#.#..#.#"],
                ),
            ],
            player_sprite: sprite(&["....#....", "...###...", ".#######.", "#########", "#########"]),
            boss_sprite: boss_sprite(),
            ox: 0.0,
            oy: START_Y,
            dir: 1.0,
            frame: 0,
            alive: [true; COLS * ROWS],
            shots: Vec::with_capacity(SHOTS_MAX),
            bombs: Vec::with_capacity(BOMBS_MAX),
            booms: Vec::with_capacity(BOOMS_MAX),
            px: GRID_W as f32 / 2.0,
            target: GRID_W as f32 / 2.0,
            cooldown: 0,
            want_fire: false,
            invulnerable: 0,
            wave_delay: 0,
            boss: None,
            clock: 0.0,
            acc: 0.0,
            beats: 0,
        };
        game.new_wave();
        game.render();
        game
    }

    /// Un boss est à l'écran.
    #[must_use]
    pub fn boss_active(&self) -> bool {
        self.boss.is_some()
    }

    /// Nombre d'envahisseurs vivants.
    #[must_use]
    pub fn invaders_alive(&self) -> usize {
        self.alive.iter().filter(|a| **a).count()
    }

    /// Appelé à chaque image avec la position continue dans la mesure (`beat.beat`) et l'énergie des aigus.
    pub fn advance(&mut self, dt: f32, clock: f32, high: f32) {
        self.clock = clock;
        if high > 0.55 {
            self.want_fire = true;
        }
        self.acc += dt;
        let mut guard = 3;
        while self.acc >= TICK && guard > 0 {
            guard -= 1;
            self.acc -= TICK;
            self.tick();
        }
        self.acc = self.acc.min(TICK);
        self.energy = (self.energy * 0.94).max(clamp01(self.booms.len() as f32 / 5.0));
    }

    /// Un kick : la formation avance, une bombe tombe, le vaisseau vise.
    pub fn on_beat(&mut self) {
        self.beats += 1;
        self.want_fire = true;
        if let Some(boss) = self.boss {
            for vx in [-0.45, 0.0, 0.45] {
                self.push_bomb(Bomb {
                    x: boss.x,
                    y: boss.y - 1.0,
                    vx,
                    vy: -0.9,
                });
            }
        } else {
            self.march();
            if self.beats.is_multiple_of(2) {
                self.drop_bomb();
            }
        }
        self.aim();
    }

    /// Un drop : le boss arrive.
    pub fn start_boss(&mut self) {
        if self.boss.is_some() {
            return;
        }
        self.boss = Some(Boss {
            x: GRID_W as f32 / 2.0,
            y: 32.0,
            hp: BOSS_HP,
            age: 0.0,
        });
        self.bombs.clear();
        self.energy = 1.0;
    }

    fn push_bomb(&mut self, bomb: Bomb) {
        if self.bombs.len() < BOMBS_MAX {
            self.bombs.push(bomb);
        }
    }

    fn new_wave(&mut self) {
        self.ox = 0.0;
        self.oy = START_Y;
        self.dir = 1.0;
        self.alive = [true; COLS * ROWS];
        self.shots.clear();
        self.bombs.clear();
    }

    fn invader_x(&self, c: usize) -> f32 {
        self.ox + c as f32 * (INV_W + GAP_X)
    }

    fn invader_y(&self, r: usize) -> f32 {
        self.oy - r as f32 * (INV_H + GAP_Y)
    }

    fn march(&mut self) {
        if self.wave_delay > 0 {
            return;
        }
        let (mut min_c, mut max_c) = (COLS, None::<usize>);
        for (i, alive) in self.alive.iter().enumerate() {
            if *alive {
                min_c = min_c.min(i % COLS);
                max_c = Some(max_c.map_or(i % COLS, |m| m.max(i % COLS)));
            }
        }
        let Some(max_c) = max_c else { return };
        let step = self.dir * 2.0;
        let left = self.invader_x(min_c) + step;
        let right = self.invader_x(max_c) + INV_W + step;
        if left < 1.0 || right > GRID_W as f32 - 1.0 {
            self.oy -= 3.0;
            self.dir = -self.dir;
        } else {
            self.ox += step;
        }
        self.frame ^= 1;
        // Les envahisseurs ont atteint le sol : le vaisseau saute, une nouvelle vague arrive.
        if self.lowest_y() < 9.0 {
            self.explode(self.px, 4.0, 5.0);
            self.wave_delay = 14;
        }
    }

    fn lowest_y(&self) -> f32 {
        let mut low = f32::INFINITY;
        for (i, alive) in self.alive.iter().enumerate() {
            if *alive {
                low = low.min(self.invader_y(i / COLS));
            }
        }
        low
    }

    /// Le `n`-ième envahisseur vivant, ou `None`.
    fn nth_alive(&self, n: usize) -> Option<usize> {
        self.alive
            .iter()
            .enumerate()
            .filter(|(_, a)| **a)
            .map(|(i, _)| i)
            .nth(n)
    }

    fn aim(&mut self) {
        let mut goal = self.px;
        if let Some(boss) = self.boss {
            goal = boss.x;
        } else {
            let live = self.invaders_alive();
            if live > 0 {
                let pick = (self.rng.unit() * live as f64) as usize;
                if let Some(i) = self.nth_alive(pick.min(live - 1)) {
                    goal = self.invader_x(i % COLS) + INV_W / 2.0;
                }
            }
        }
        // Esquive : une bombe proche et basse pousse le vaisseau de côté.
        if let Some(threat) = self.bombs.iter().find(|b| b.y < 20.0 && (b.x - self.px).abs() < 6.0) {
            goal = self.px + if self.px < threat.x { -9.0 } else { 9.0 };
        }
        self.target = goal.clamp(6.0, GRID_W as f32 - 7.0);
    }

    fn drop_bomb(&mut self) {
        if self.wave_delay > 0 {
            return;
        }
        let live = self.invaders_alive();
        if live == 0 {
            return;
        }
        let pick = (self.rng.unit() * live as f64) as usize;
        let Some(index) = self.nth_alive(pick.min(live - 1)) else {
            return;
        };
        let c = index % COLS;
        // Seul le plus bas de sa colonne peut lâcher une bombe.
        let mut r = ROWS - 1;
        while r > 0 && !self.alive[r * COLS + c] {
            r -= 1;
        }
        let bomb = Bomb {
            x: self.invader_x(c) + INV_W / 2.0,
            y: self.invader_y(r) - 1.0,
            vx: 0.0,
            vy: -0.7,
        };
        self.push_bomb(bomb);
    }

    fn explode(&mut self, x: f32, y: f32, size: f32) {
        if self.booms.len() < BOOMS_MAX {
            self.booms.push(Boom { x, y, life: 8, size });
        }
    }

    fn tick(&mut self) {
        self.px += (self.target - self.px).clamp(-2.2, 2.2);
        self.cooldown = (self.cooldown - 1).max(0);
        self.invulnerable = (self.invulnerable - 1).max(0);
        if self.want_fire && self.cooldown == 0 && (self.px - self.target).abs() < 3.0 && self.shots.len() < SHOTS_MAX {
            self.shots.push((self.px.round(), 7.0));
            self.cooldown = 4;
            self.want_fire = false;
        }
        self.move_shots();
        self.move_bombs();
        self.update_boss();
        for boom in &mut self.booms {
            boom.life -= 1;
        }
        self.booms.retain(|b| b.life > 0);

        if self.wave_delay > 0 {
            self.wave_delay -= 1;
            if self.wave_delay == 0 {
                self.new_wave();
            }
        } else if self.boss.is_none() && !self.alive.iter().any(|a| *a) {
            self.wave_delay = 18;
        }
        self.render();
    }
}

#[cfg(test)]
mod tests;
