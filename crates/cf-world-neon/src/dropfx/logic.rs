//! Le paquet du drop vu du monde (`Effects.dropPackage`, `shockwave` et `burst` de `fx.ts`) : une gerbe de particules qui
//! jaillit tout de suite, des ondes de choc posées au sol qui se suivent en cascade. Logique pure, testable sans Bevy.
//!
//! # Même image que `drop_now`
//!
//! [`DropFx::step`] reçoit le paquet **de l'image** (`ShowFxInput::drop`, `Some` sur la seule image où `drop_now` est
//! vrai). La gerbe et la première onde partent dans ce même appel, avec le même âge de départ pour tout le reste du
//! paquet ; les ondes suivantes attendent leur tour dans [`DropFx::pending`].
//!
//! # Règle 7
//!
//! Chaque départ (gerbe + première onde ensemble, puis chaque onde de la cascade) est présenté au limiteur de flashs
//! ([`DropFlash`]) : accordé, il brille au plafond de la voie ; refusé, il reste sombre ([`UNGRANTED_GAIN`]). Flashs coupés :
//! tout est éteint, par une descente d'au moins `MIN_FADE` ([`DropFx::master`]). Les ondes montent en [`ATTACK_S`] : rien ne
//! s'allume d'un coup.
//!
//! # Écarts de réglage avec Sillage (voir la PR)
//!
//! - l'écart entre deux ondes passe de 0,14 s à [`DROP_RING_GAP_S`] (0,34 s) : à 0,14 s le limiteur (0,25 s entre deux
//!   départs) refuserait la deuxième onde ; à 0,34 s la cascade de trois ondes est accordée entière, trois départs en
//!   0,68 s, sous le plafond de 3 par seconde ;
//! - au plus [`DROP_RINGS_MAX`] = 3 ondes (le paquet n'en demande jamais plus ; Sillage en admettait 4) ;
//! - [`PARTICLES`] = 240 particules (4 gerbes de 60) au lieu de 900 (Sillage les partage avec les bonus du jeu).

use bevy::ecs::resource::Resource;
use bevy::math::{DVec3, Vec3};
use cf_show::drop::DropFxPackage;
use cf_show::limiter::MIN_FADE;
use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

use super::safety::{ATTACK_S, DropFlash, GRANTED_GAIN, UNGRANTED_GAIN};

/// Ondes de choc simultanées (`RINGS`).
pub const RINGS: usize = 6;
/// Ondes d'un même drop au plus.
pub const DROP_RINGS_MAX: usize = 3;
/// Écart entre les ondes d'un même drop (s).
pub const DROP_RING_GAP_S: f32 = 0.34;
/// Vie d'une onde (s).
pub const RING_LIFE_S: f32 = 1.1;
/// Vie d'une particule (s).
pub const PARTICLE_LIFE_S: f32 = 1.4;
/// Particules vivantes au plus (anneau : la plus ancienne cède la place).
pub const PARTICLES: usize = 240;
/// Hauteur de l'onde au sol (m).
pub const RING_Y_M: f64 = 0.06;
/// Hauteur de départ de la gerbe au-dessus du point du drop (m).
pub const BURST_Y_M: f64 = 0.5;
/// Les ondes grandissent de 1 à `1 + RING_GROWTH` fois leur taille de départ (`1 + t × 16`).
pub const RING_GROWTH: f32 = 16.0;
/// Teintes du cycle prismatique.
pub const CYCLE_LEN: usize = 6;

/// Une onde de choc.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ring {
    /// Âge (s) ; libre à partir de [`RING_LIFE_S`].
    pub age_s: f32,
    /// Centre (absolu, m).
    pub at: DVec3,
    /// Rang de la teinte dans le cycle prismatique.
    pub tint: usize,
    /// Le limiteur a accordé le départ de cette onde.
    pub granted: bool,
}

impl Ring {
    const FREE: Self = Self {
        age_s: RING_LIFE_S,
        at: DVec3::ZERO,
        tint: 0,
        granted: false,
    };

    /// L'onde est éteinte.
    #[must_use]
    pub fn is_free(&self) -> bool {
        self.age_s >= RING_LIFE_S
    }

    /// Avancement, 0 → 1.
    #[must_use]
    pub fn t(&self) -> f32 {
        (self.age_s / RING_LIFE_S).min(1.0)
    }

    /// Facteur d'échelle de l'anneau : `1 + t × 16` (`ring.mesh.scale`).
    #[must_use]
    pub fn scale(&self) -> f32 {
        1.0 + self.t() * RING_GROWTH
    }

    /// Éclat relatif, 0 → 1 : montée en [`ATTACK_S`] puis `(1 − t)²` (l'opacité de Sillage). À multiplier par le gain.
    #[must_use]
    pub fn level(&self) -> f32 {
        let t = self.t();
        (self.age_s / ATTACK_S).clamp(0.0, 1.0) * (1.0 - t) * (1.0 - t)
    }
}

/// Ce que lit [`DropFx::step`] à chaque image.
#[derive(Debug, Clone, Copy)]
pub struct DropInput {
    /// Durée de l'image (s).
    pub dt_s: f32,
    /// Horloge de la scène (s).
    pub now_s: f64,
    /// Le paquet qui frappe **dans cette image** (`ShowFxInput::drop`), `None` sur toutes les autres.
    pub drop: Option<DropFxPackage>,
    /// Où jouer le paquet (la voiture, absolu) ; `None` en mode calme (« réduire les animations »).
    pub at: Option<DVec3>,
    /// `(flashes, breathing)` du spectacle, `None` sans spectacle.
    pub policy: Option<(bool, bool)>,
    /// Décale les teintes du cycle (le compteur de kicks, `seed` de Sillage).
    pub seed: u32,
}

/// Une gerbe de particules : positions relatives à [`DropFx::origin`].
#[derive(Debug, Clone)]
struct Sparks {
    pos: [Vec3; PARTICLES],
    vel: [Vec3; PARTICLES],
    base: [[f32; 3]; PARTICLES],
    gain: [f32; PARTICLES],
    life_s: [f32; PARTICLES],
    age_s: [f32; PARTICLES],
}

impl Sparks {
    fn new() -> Self {
        Self {
            pos: [Vec3::ZERO; PARTICLES],
            vel: [Vec3::ZERO; PARTICLES],
            base: [[0.0; 3]; PARTICLES],
            gain: [0.0; PARTICLES],
            life_s: [0.0; PARTICLES],
            age_s: [0.0; PARTICLES],
        }
    }
}

/// Le paquet du drop en cours : ondes, ondes en attente, gerbe, limiteur. Écrit par le seul `systems::step_dropfx`.
#[derive(Resource, Debug, Clone)]
pub struct DropFx {
    rings: [Ring; RINGS],
    /// Délai restant (s) et rang de chaque onde en attente.
    pending: [(f32, usize); DROP_RINGS_MAX],
    pending_count: usize,
    pending_at: DVec3,
    pending_seed: u32,
    /// Le départ de la gerbe et de la première onde a-t-il été accordé ?
    first_granted: bool,
    sparks: Sparks,
    cursor: usize,
    origin: DVec3,
    flash: DropFlash,
    master: f32,
    cycle: [[f32; 3]; CYCLE_LEN],
    rng: Pcg32,
    /// Paquets joués depuis le début.
    packages: u32,
    /// Image (compteur d'appels de `step`) du dernier paquet joué.
    last_package_frame: Option<u64>,
    frame: u64,
}

impl Default for DropFx {
    fn default() -> Self {
        Self::new(0)
    }
}

impl DropFx {
    /// Un état neuf dont les tirages de la gerbe partent de `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            rings: [Ring::FREE; RINGS],
            pending: [(0.0, 0); DROP_RINGS_MAX],
            pending_count: 0,
            pending_at: DVec3::ZERO,
            pending_seed: 0,
            first_granted: false,
            sparks: Sparks::new(),
            cursor: 0,
            origin: DVec3::ZERO,
            flash: DropFlash::default(),
            master: 0.0,
            cycle: [[1.0; 3]; CYCLE_LEN],
            rng: Pcg32::seed_from_u64(seed),
            packages: 0,
            last_package_frame: None,
            frame: 0,
        }
    }

    /// Les teintes du cycle prismatique (RVB linéaire), de la palette de l'image.
    pub fn set_cycle(&mut self, cycle: [[f32; 3]; CYCLE_LEN]) {
        self.cycle = cycle;
    }

    /// Les teintes du cycle prismatique (RVB linéaire).
    #[must_use]
    pub fn cycle(&self) -> &[[f32; 3]; CYCLE_LEN] {
        &self.cycle
    }

    /// Les ondes du pool.
    #[must_use]
    pub fn rings(&self) -> &[Ring; RINGS] {
        &self.rings
    }

    /// Ondes en vie.
    #[must_use]
    pub fn rings_alive(&self) -> usize {
        self.rings.iter().filter(|r| !r.is_free()).count()
    }

    /// Ondes encore en attente de leur tour.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.pending_count
    }

    /// Particules en vie.
    #[must_use]
    pub fn particles_alive(&self) -> usize {
        self.sparks.life_s.iter().filter(|&&l| l > 0.0).count()
    }

    /// Paquets joués depuis le début.
    #[must_use]
    pub fn packages(&self) -> u32 {
        self.packages
    }

    /// Numéro d'appel de `step` où le dernier paquet est parti (0 pour le premier appel).
    #[must_use]
    pub fn last_package_frame(&self) -> Option<u64> {
        self.last_package_frame
    }

    /// Numéro du dernier appel de `step` (0 pour le premier).
    #[must_use]
    pub fn frame(&self) -> u64 {
        self.frame.saturating_sub(1)
    }

    /// Facteur de permission lissé, 0 → 1 : multiplie l'éclat de tout ce que montre le paquet. Il tombe à 0 en `MIN_FADE`
    /// quand les flashs sont coupés (jamais de coupure sèche) et remonte de même.
    #[must_use]
    pub fn master(&self) -> f32 {
        self.master
    }

    /// Les flashs sont-ils permis en ce moment ?
    #[must_use]
    pub fn permitted(&self) -> bool {
        self.flash.permitted()
    }

    /// Le limiteur du paquet.
    #[must_use]
    pub fn flash(&self) -> &DropFlash {
        &self.flash
    }

    /// Éclat d'une onde, 0 → `MAX_FLASH_PEAK` : accordée ou non, montée, décroissance, permission.
    #[must_use]
    pub fn ring_level(&self, ring: &Ring) -> f32 {
        let gain = if ring.granted { GRANTED_GAIN } else { UNGRANTED_GAIN };
        ring.level() * gain * self.master
    }

    /// Point d'origine des positions des particules (absolu).
    #[must_use]
    pub fn origin(&self) -> DVec3 {
        self.origin
    }

    /// Position d'une particule relativement à [`Self::origin`] (m).
    #[must_use]
    pub fn particle_offset(&self, i: usize) -> Vec3 {
        self.sparks.pos[i]
    }

    /// Couleur d'une particule (RVB linéaire, éclat compris), nulle si elle est éteinte.
    #[must_use]
    pub fn particle_color(&self, i: usize) -> [f32; 3] {
        let s = &self.sparks;
        if s.life_s[i] <= 0.0 {
            return [0.0; 3];
        }
        let fade = (s.life_s[i] / PARTICLE_LIFE_S).max(0.0);
        let attack = (s.age_s[i] / ATTACK_S).clamp(0.0, 1.0);
        let k = fade * attack * s.gain[i] * self.master;
        s.base[i].map(|c| c * k)
    }

    /// Plus petit parallélépipède qui contient les particules vivantes (relatif à l'origine), marge comprise : l'`Aabb` de la
    /// gerbe, recalculé à chaque image pour que le culling reste exact.
    #[must_use]
    pub fn bounds(&self, margin: f32) -> Option<(Vec3, Vec3)> {
        let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        let mut any = false;
        for (p, &life) in self.sparks.pos.iter().zip(&self.sparks.life_s) {
            if life > 0.0 {
                min = min.min(*p);
                max = max.max(*p);
                any = true;
            }
        }
        any.then(|| (min - Vec3::splat(margin), max + Vec3::splat(margin)))
    }

    fn unit(&mut self) -> f32 {
        self.rng.next_u32() as f32 / 4_294_967_296.0
    }

    /// Avance d'une image.
    pub fn step(&mut self, input: &DropInput) {
        let dt = input.dt_s.clamp(0.0, 0.25);
        self.flash.set_policy(input.policy);
        // La permission glisse : jamais plus vite que `MIN_FADE` pour une course complète.
        let target = if self.flash.permitted() { 1.0 } else { 0.0 };
        if self.frame == 0 {
            // Rien n'est encore à l'écran : pas de montée à jouer au tout premier appel.
            self.master = target;
        }
        let max_move = dt / MIN_FADE;
        self.master += (target - self.master).clamp(-max_move, max_move);

        if let (Some(fx), Some(at)) = (input.drop, input.at) {
            self.start_package(&fx, at, input);
        }
        self.start_due_rings(dt, input.now_s);

        for ring in &mut self.rings {
            if !ring.is_free() {
                ring.age_s += dt;
            }
        }
        self.step_particles(dt);
        self.flash.step(dt);
        self.frame += 1;
    }

    /// Le paquet du drop : la gerbe tout de suite, les ondes en cascade (la première dès cette image).
    fn start_package(&mut self, fx: &DropFxPackage, at: DVec3, input: &DropInput) {
        self.packages += 1;
        self.last_package_frame = Some(self.frame);
        self.pending_at = at;
        self.pending_seed = input.seed;
        self.pending_count = usize::from(fx.shockwaves).min(DROP_RINGS_MAX);
        for (i, slot) in self.pending.iter_mut().enumerate().take(self.pending_count) {
            *slot = (i as f32 * DROP_RING_GAP_S, i);
        }
        // La gerbe et la première onde partent ensemble : un seul départ pour le limiteur.
        self.first_granted = self.flash.request(input.now_s);
        let gain = if self.first_granted {
            GRANTED_GAIN
        } else {
            UNGRANTED_GAIN
        } / GRANTED_GAIN;
        self.burst(at, usize::from(fx.burst), input.seed, gain);
    }

    fn start_due_rings(&mut self, dt: f32, now_s: f64) {
        let mut left = 0;
        for i in 0..self.pending_count {
            let (wait, index) = self.pending[i];
            let wait = wait - dt;
            if wait <= 0.0 {
                let granted = if index == 0 {
                    self.first_granted
                } else {
                    self.flash.request(now_s)
                };
                self.shockwave(index, granted);
            } else {
                self.pending[left] = (wait, index);
                left += 1;
            }
        }
        self.pending_count = left;
    }

    /// Onde de choc posée au sol, sous le point du drop.
    fn shockwave(&mut self, index: usize, granted: bool) {
        let slot = self.rings.iter().position(Ring::is_free).unwrap_or(index % RINGS);
        self.rings[slot] = Ring {
            age_s: 0.0,
            at: DVec3::new(self.pending_at.x, RING_Y_M, self.pending_at.z),
            tint: (self.pending_seed as usize).wrapping_add(index) % CYCLE_LEN,
            granted,
        };
    }

    /// Explosion de particules à partir d'un point, en teintes du cycle prisme. `gain` : 1 accordé, `UNGRANTED/GRANTED` sinon.
    fn burst(&mut self, at: DVec3, count: usize, seed: u32, gain: f32) {
        let origin = at + DVec3::new(0.0, BURST_Y_M, 0.0);
        // Les particules déjà en vol suivent le nouveau repère : leurs positions sont relatives à l'origine.
        let shift = (origin - self.origin).as_vec3();
        for (p, &life) in self.sparks.pos.iter_mut().zip(&self.sparks.life_s) {
            if life > 0.0 {
                *p -= shift;
            }
        }
        self.origin = origin;
        for n in 0..count {
            let i = self.cursor;
            self.cursor = (self.cursor + 1) % PARTICLES;
            let angle = self.unit() * std::f32::consts::TAU;
            let speed = 3.0 + self.unit() * 9.0;
            let up = 2.0 + self.unit() * 6.0;
            let life = PARTICLE_LIFE_S * (0.6 + self.unit() * 0.4);
            let s = &mut self.sparks;
            s.pos[i] = Vec3::ZERO;
            s.vel[i] = Vec3::new(angle.cos() * speed * 0.6, up, angle.sin() * speed);
            s.base[i] = self.cycle[(seed as usize).wrapping_add(n) % CYCLE_LEN];
            s.gain[i] = gain;
            s.life_s[i] = life;
            s.age_s[i] = 0.0;
        }
    }

    fn step_particles(&mut self, dt: f32) {
        let s = &mut self.sparks;
        for i in 0..PARTICLES {
            if s.life_s[i] <= 0.0 {
                continue;
            }
            s.life_s[i] -= dt;
            s.age_s[i] += dt;
            let drag = (1.0 - dt * 1.8).max(0.0);
            let mut v = s.vel[i] * drag;
            v.y = s.vel[i].y * drag - 9.8 * dt * 0.5;
            s.vel[i] = v;
            s.pos[i] += v * dt;
        }
    }
}
