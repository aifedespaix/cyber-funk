//! L'explosion d'un astre contre un immeuble (`MeteorBlasts` de `meteor-blast.ts`) : un éclair, une onde de choc qui
//! s'élargit face à la caméra et une gerbe de débris incandescents qui retombent. Pool préalloué de [`BLASTS`]
//! explosions, rien n'est créé pendant la lecture.
//!
//! Logique pure : positions des débris relatives au point d'impact (en `f32`, l'impact lui-même est en absolu `f64`),
//! âges, niveaux. Le rendu ([`super::rig`]) en fait des transformations et des couleurs.

use bevy::math::{DVec3, Vec3};

use crate::rng::hash;

/// Nombre d'explosions simultanées.
pub const BLASTS: usize = 6;
/// Débris par explosion.
pub const DEBRIS: usize = 40;
/// Vie de l'éclair (s).
pub const FLASH_LIFE: f32 = 0.9;
/// Vie de l'onde de choc (s).
pub const SHOCK_LIFE: f32 = 1.6;
/// Vie maximale d'un débris (s).
pub const DEBRIS_LIFE: f32 = 2.2;
/// Gravité des débris (m/s²).
const GRAVITY: f32 = 22.0;
/// Montée de tout ce qui s'allume à l'impact (s) : un peu plus que `MIN_FADE` (0,08 s), jamais de départ sec.
pub const ATTACK_S: f32 = 0.1;

/// Une explosion du pool.
#[derive(Debug, Clone, Copy)]
pub struct Blast {
    /// Point d'impact (absolu, m).
    pub at: DVec3,
    /// Rayon de l'astre (m).
    pub radius: f32,
    /// Âge (s) : libre à partir de [`SHOCK_LIFE`] pour l'éclair et l'onde (les débris vivent plus longtemps).
    pub age_s: f32,
    /// Indice de teinte de chaleur.
    pub tint: usize,
    /// Le limiteur de flashs a accordé l'éclair à cette explosion.
    pub flash_granted: bool,
    offsets: [Vec3; DEBRIS],
    velocities: [Vec3; DEBRIS],
    life_s: [f32; DEBRIS],
}

impl Blast {
    const FREE: Self = Self {
        at: DVec3::ZERO,
        radius: 1.0,
        age_s: SHOCK_LIFE,
        tint: 0,
        flash_granted: false,
        offsets: [Vec3::ZERO; DEBRIS],
        velocities: [Vec3::ZERO; DEBRIS],
        life_s: [0.0; DEBRIS],
    };

    /// L'éclair et l'onde sont éteints.
    #[must_use]
    pub fn shock_done(&self) -> bool {
        self.age_s >= SHOCK_LIFE
    }

    /// Au moins un débris vit encore.
    #[must_use]
    pub fn debris_alive(&self) -> bool {
        self.life_s.iter().any(|&l| l > 0.0)
    }

    /// Taille de référence de l'éclair et de l'onde (m).
    #[must_use]
    pub fn size_m(&self) -> f32 {
        10.0 + self.radius * 11.0
    }

    /// Montée commune, 0 → 1 en [`ATTACK_S`] : rien ne s'allume d'un coup.
    #[must_use]
    pub fn attack(&self) -> f32 {
        (self.age_s / ATTACK_S).clamp(0.0, 1.0)
    }

    /// Avancement de l'éclair, 0 → 1.
    #[must_use]
    pub fn flash_t(&self) -> f32 {
        (self.age_s / FLASH_LIFE).min(1.0)
    }

    /// Côté de l'éclair (m) : il grossit pendant sa vie.
    #[must_use]
    pub fn flash_scale(&self) -> f32 {
        self.size_m() * (0.5 + self.flash_t() * 1.3)
    }

    /// Avancement de l'onde de choc, 0 → 1.
    #[must_use]
    pub fn shock_t(&self) -> f32 {
        (self.age_s / SHOCK_LIFE).min(1.0)
    }

    /// Rayon de l'anneau de l'onde (`uR`), 0 → 1 : il ralentit en s'élargissant.
    #[must_use]
    pub fn shock_radius(&self) -> f32 {
        let t = self.shock_t();
        1.0 - (1.0 - t) * (1.0 - t)
    }

    /// Opacité de l'onde de choc (hors gain et montée), 1 → 0.
    #[must_use]
    pub fn shock_fade(&self) -> f32 {
        let t = self.shock_t();
        (1.0 - t) * (1.0 - t)
    }

    /// Position d'un débris relativement au point d'impact (m).
    #[must_use]
    pub fn offset(&self, i: usize) -> Vec3 {
        self.offsets[i]
    }

    /// Niveau lumineux d'un débris, 0 → 2,7 : incandescent au départ (au-delà de 1 : le bloom s'en empare), puis il
    /// rougeoie et s'éteint. Multiplié par la montée commune : un débris ne naît jamais à plein feu.
    #[must_use]
    pub fn debris_level(&self, i: usize) -> f32 {
        if self.life_s[i] <= 0.0 {
            return 0.0;
        }
        let fade = (self.life_s[i] / DEBRIS_LIFE).max(0.0);
        (fade * fade * 2.6 + 0.1 * fade) * self.attack()
    }

    /// Plus petit parallélépipède qui contient tous les débris vivants (relatif à l'impact), marge comprise : l'`Aabb`
    /// de l'entité des débris, recalculé à chaque image pour que le culling reste exact.
    #[must_use]
    pub fn debris_bounds(&self, margin: f32) -> (Vec3, Vec3) {
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for (offset, life) in self.offsets.iter().zip(&self.life_s) {
            if *life > 0.0 {
                min = min.min(*offset);
                max = max.max(*offset);
            }
        }
        if min.x > max.x {
            return (Vec3::splat(-margin), Vec3::splat(margin));
        }
        (min - Vec3::splat(margin), max + Vec3::splat(margin))
    }
}

/// Le pool d'explosions.
#[derive(Debug, Clone)]
pub struct Blasts {
    slots: [Blast; BLASTS],
    count: u32,
}

impl Default for Blasts {
    fn default() -> Self {
        Self::new()
    }
}

impl Blasts {
    /// Pool au repos.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: [Blast::FREE; BLASTS],
            count: 0,
        }
    }

    /// Les explosions du pool.
    #[must_use]
    pub fn slots(&self) -> &[Blast; BLASTS] {
        &self.slots
    }

    /// Les explosions du pool, en écriture (le limiteur y marque l'éclair accordé).
    pub(crate) fn slots_mut(&mut self) -> &mut [Blast; BLASTS] {
        &mut self.slots
    }

    /// Fait exploser un astre : `at` est le point d'impact, `radius` celui de l'astre (la taille de l'explosion en
    /// découle), `tint` sa teinte de chaleur. Renvoie l'emplacement pris.
    pub fn trigger(&mut self, at: DVec3, radius: f32, tint: usize) -> usize {
        // Une explosion éteinte, sinon la plus ancienne : elle est presque finie.
        let slot = self.slots.iter().position(Blast::shock_done).unwrap_or_else(|| {
            self.slots
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.age_s.total_cmp(&b.1.age_s))
                .map_or(0, |(i, _)| i)
        });
        let seed = f64::from(self.count);
        self.count = self.count.wrapping_add(1);
        let blast = &mut self.slots[slot];
        blast.at = at;
        blast.radius = radius;
        blast.age_s = 0.0;
        blast.tint = tint;
        blast.flash_granted = false;
        let power = 14.0 + radius * 7.0;
        for n in 0..DEBRIS {
            let salt = (n * 3) as f64;
            // Gerbe dans toutes les directions, un peu plus vers le haut.
            let theta = hash(seed, salt + 1.0) as f32 * std::f32::consts::TAU;
            let phi = (1.0 - 2.0 * hash(seed, salt + 2.0) as f32).acos();
            let speed = power * (0.25 + hash(seed, salt + 3.0) as f32 * 0.9);
            blast.offsets[n] = Vec3::ZERO;
            blast.velocities[n] = Vec3::new(
                phi.sin() * theta.cos() * speed,
                phi.cos() * speed * 0.8 + power * 0.25,
                phi.sin() * theta.sin() * speed,
            );
            blast.life_s[n] = DEBRIS_LIFE * (0.55 + hash(seed, salt + 4.0) as f32 * 0.45);
        }
        slot
    }

    /// Retire l'éclair accordé à toutes les explosions (le limiteur en accorde un autre : un seul brille à la fois).
    pub(crate) fn clear_flash_holders(&mut self) {
        for blast in &mut self.slots {
            blast.flash_granted = false;
        }
    }

    /// Avance d'une image : âges, vol des débris (traînée, gravité).
    pub fn update(&mut self, dt_s: f32) {
        let dt = dt_s.max(0.0);
        for blast in &mut self.slots {
            if !blast.shock_done() {
                blast.age_s += dt;
            }
            for i in 0..DEBRIS {
                if blast.life_s[i] <= 0.0 {
                    continue;
                }
                blast.life_s[i] -= dt;
                let drag = (1.0 - dt * 0.7).max(0.0);
                let v = &mut blast.velocities[i];
                v.x *= drag;
                v.y = v.y * drag - GRAVITY * dt;
                v.z *= drag;
                blast.offsets[i] += *v * dt;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn une_explosion_vit_puis_se_libere() {
        let mut blasts = Blasts::new();
        let slot = blasts.trigger(DVec3::new(10.0, 20.0, -400.0), 3.0, 2);
        assert!(!blasts.slots()[slot].shock_done());
        assert!(blasts.slots()[slot].debris_alive());
        for _ in 0..(3.0 / DT) as usize {
            blasts.update(DT);
        }
        let blast = &blasts.slots()[slot];
        assert!(blast.shock_done());
        assert!(
            !blast.debris_alive(),
            "tous les débris sont éteints après {DEBRIS_LIFE} s"
        );
    }

    #[test]
    fn le_pool_est_borne_et_reutilise_la_plus_ancienne() {
        let mut blasts = Blasts::new();
        let mut slots = Vec::new();
        for i in 0..BLASTS {
            slots.push(blasts.trigger(DVec3::splat(i as f64), 1.0, 0));
            blasts.update(DT);
        }
        slots.sort_unstable();
        slots.dedup();
        assert_eq!(slots.len(), BLASTS, "chaque explosion prend sa place");
        // Pool plein : la septième écrase la plus ancienne (la première).
        let extra = blasts.trigger(DVec3::splat(99.0), 1.0, 0);
        assert_eq!(extra, 0);
    }

    #[test]
    fn rien_ne_s_allume_d_un_coup() {
        let mut blasts = Blasts::new();
        let slot = blasts.trigger(DVec3::ZERO, 2.0, 0);
        let blast = &blasts.slots()[slot];
        assert_eq!(blast.attack(), 0.0);
        assert!(
            (0..DEBRIS).all(|i| blast.debris_level(i) == 0.0),
            "débris sombres à l'instant 0"
        );
        let mut last = 0.0;
        for _ in 0..8 {
            blasts.update(DT);
            let level = blasts.slots()[slot].debris_level(0);
            assert!(level - last <= 2.7 * DT / 0.08 * 1.01 + 1e-3, "montée trop rapide");
            last = level;
        }
    }

    #[test]
    fn les_debris_retombent_sous_la_gravite() {
        let mut blasts = Blasts::new();
        let slot = blasts.trigger(DVec3::ZERO, 1.0, 0);
        for _ in 0..(2.0 / DT) as usize {
            blasts.update(DT);
        }
        let blast = &blasts.slots()[slot];
        let falling = (0..DEBRIS)
            .filter(|&i| blast.life_s[i] > 0.0 && blast.velocities[i].y < 0.0)
            .count();
        assert!(falling > 0, "au bout de 2 s, des débris vivants descendent");
    }

    #[test]
    fn la_boite_contient_tous_les_debris_vivants() {
        let mut blasts = Blasts::new();
        let slot = blasts.trigger(DVec3::ZERO, 4.0, 1);
        for step in 0..120 {
            blasts.update(DT);
            let blast = &blasts.slots()[slot];
            let (min, max) = blast.debris_bounds(0.5);
            for i in 0..DEBRIS {
                if blast.life_s[i] > 0.0 {
                    let p = blast.offset(i);
                    assert!(
                        p.cmpge(min).all() && p.cmple(max).all(),
                        "débris {i} hors boîte à l'image {step}"
                    );
                }
            }
        }
    }

    #[test]
    fn le_tirage_est_deterministe() {
        let (mut a, mut b) = (Blasts::new(), Blasts::new());
        a.trigger(DVec3::ZERO, 2.0, 0);
        b.trigger(DVec3::ZERO, 2.0, 0);
        for _ in 0..30 {
            a.update(DT);
            b.update(DT);
        }
        assert_eq!(a.slots()[0].offsets, b.slots()[0].offsets);
    }
}
