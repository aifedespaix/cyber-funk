//! Une partie de borne d'arcade qui joue seule, au rythme de la musique (`arcade-game.ts`).
//!
//! Grille de 72 × 48 cases. Chaque kick fait avancer la formation d'envahisseurs d'un pas et lâcher une bombe ; le vaisseau
//! se replace et tire dès qu'il est aligné, et les aigus (charleston) déclenchent des tirs supplémentaires. Un drop appelle
//! un boss : il balance d'un bord à l'autre sur une période de quatre temps, répond par des éventails de bombes à chaque
//! kick, et son explosion rend la place à une nouvelle vague.
//!
//! Pure et déterministe : le hasard est injecté (graine). Aucune allocation en régime établi : les tirs, bombes et
//! explosions vivent dans des tampons de capacité fixe, la grille est écrite en place.

use super::math::clamp01;
use crate::pilot::rand::Rand;

/// Largeur de la grille, en cases.
pub const GRID_W: usize = 72;
/// Hauteur de la grille, en cases.
pub const GRID_H: usize = 48;

/// Indices de couleur écrits dans la grille ; le shader les convertit en palette.
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

#[derive(Debug, Clone)]
struct Sprite {
    w: usize,
    h: usize,
    on: Vec<u8>,
}

/// Lit un dessin en texte (`#` allumé), haut du dessin en haut ; stocké l'axe y vers le haut.
fn sprite(rows: &[&str]) -> Sprite {
    let w = rows.first().map_or(0, |r| r.len());
    let h = rows.len();
    let mut on = vec![0_u8; w * h];
    for (ry, row) in rows.iter().enumerate() {
        for (x, c) in row.bytes().enumerate() {
            on[(h - 1 - ry) * w + x] = u8::from(c == b'#');
        }
    }
    Sprite { w, h, on }
}

/// Le boss : une soucoupe à dôme, deux yeux creux à pupilles, une rangée de dents. Construit à partir d'un profil de
/// demi-largeurs.
fn boss_sprite() -> Sprite {
    let half = [4_i32, 6, 8, 9, 10, 11, 11, 11, 11, 10, 9];
    let rows: Vec<String> = half
        .iter()
        .enumerate()
        .map(|(y, &hw)| {
            (0..23_i32)
                .map(|x| {
                    let d = (x - 11).abs();
                    if d > hw {
                        return '.';
                    }
                    let eye = (3..=6).contains(&y) && ((5..=8).contains(&x) || (14..=17).contains(&x));
                    let pupil = (4..=5).contains(&y) && ((6..=7).contains(&x) || (15..=16).contains(&x));
                    if eye {
                        return if pupil { '#' } else { '.' };
                    }
                    if y >= 9 && x % 2 == 0 && d <= 7 {
                        return '.';
                    }
                    '#'
                })
                .collect()
        })
        .collect();
    let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
    sprite(&refs)
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

    fn move_shots(&mut self) {
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
    fn advance_shot(&mut self, i: usize) -> bool {
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

    fn move_bombs(&mut self) {
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

    fn update_boss(&mut self) {
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

    fn render(&mut self) {
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
                let kind = if row == 0 {
                    2
                } else if row < 3 {
                    1
                } else {
                    0
                };
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

    fn blit(&mut self, kind: usize, frame: usize, x0: f32, y0: f32, value: u8) {
        let (w, h) = (self.invaders[kind][frame].w, self.invaders[kind][frame].h);
        for y in 0..h {
            for x in 0..w {
                if self.invaders[kind][frame].on[y * w + x] != 0 {
                    self.plot(x0 + x as f32, y0 + y as f32, value, 1, 1);
                }
            }
        }
    }

    fn blit_player(&mut self, x0: f32) {
        let (w, h) = (self.player_sprite.w, self.player_sprite.h);
        for y in 0..h {
            for x in 0..w {
                if self.player_sprite.on[y * w + x] != 0 {
                    self.plot(x0 + x as f32, 2.0 + y as f32, Px::PLAYER, 1, 1);
                }
            }
        }
    }

    fn blit_boss(&mut self, boss: Boss) {
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

    fn plot(&mut self, x: f32, y: f32, value: u8, w: usize, h: usize) {
        let (bx, by) = (x.round() as i32, y.round() as i32);
        for dy in 0..h as i32 {
            for dx in 0..w as i32 {
                let (cx, cy) = (bx + dx, by + dy);
                if (0..GRID_W as i32).contains(&cx) && (0..GRID_H as i32).contains(&cy) {
                    self.cells[cy as usize * GRID_W + cx as usize] = value;
                }
            }
        }
    }

    fn ring(&mut self, x: f32, y: f32, r: f32) {
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

#[cfg(test)]
mod tests {
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
        assert!(c.cells.len() == a.cells.len());
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
    }
}
