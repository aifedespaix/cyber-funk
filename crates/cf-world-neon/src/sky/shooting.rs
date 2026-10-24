//! Étoiles filantes (`shooting-stars.ts`) : un trait à tête brillante qui glisse sur la voûte. Une toutes les quelques
//! secondes, plus une de temps en temps sur un kick. Logique pure : la position, la vie et l'opacité de chaque trait ; le
//! système Bevy n'en fait qu'un `Transform` et un uniforme.
//!
//! # Sécurité photosensible (règle 7)
//!
//! Une étoile filante n'est pas un flash plein écran : c'est un trait de moins de 3 m de large à 760 m, soit une fraction de
//! degré, bien en deçà des 25 % du champ visuel que vise WCAG 2.3.1. Elle ne passe donc pas par le limiteur du
//! post-process, mais elle en respecte les cadences, que [`ShootingStars::launch`] impose :
//!
//! - au plus [`MAX_PER_SECOND`] départs par seconde glissante ;
//! - au moins [`MIN_GAP_S`] s entre deux départs ;
//! - jamais de montée plus brusque que `MIN_FADE` (0,08 s) : la durée de vie, de [`LIFE_S`]`.0` s au moins, se répartit
//!   sur une enveloppe en sinus, et la montée prend donc plus de 0,35 s.

use bevy::math::{Mat3, Quat, Vec3};

/// Taille du pool (`POOL` de Sillage).
pub const POOL: usize = 5;
/// Rayon de la sphère où naissent les traits, en m (`RADIUS` de `shooting-stars.ts`).
pub const RADIUS_M: f32 = 760.0;
/// Écart minimal entre deux départs, en s (celui du limiteur de flashs).
pub const MIN_GAP_S: f64 = 0.25;
/// Nombre maximal de départs par seconde glissante (celui du limiteur de flashs).
pub const MAX_PER_SECOND: usize = 3;
/// Durée de vie d'un trait : `(base, étendue)` en s, soit 0,7 à 1,3 s.
pub const LIFE_S: (f32, f32) = (0.7, 0.6);
/// Nombre de teintes de traits : glace, menthe, azur, corail.
pub const TINT_COUNT: usize = 4;

const UP: Vec3 = Vec3::Y;

/// Hasard de Sillage (`rand` de `shooting-stars.ts`) : déterministe par `(graine, sel)`, dans [0, 1).
fn rand(seed: u32, salt: u32) -> f32 {
    let x = (f64::from(seed) * 91.7 + f64::from(salt) * 37.3 + 3.1).sin() * 43_758.545_3;
    (x - x.floor()) as f32
}

/// Un trait du pool.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Streak {
    /// Tête du trait, dans le repère du ciel, en m.
    pub position: Vec3,
    /// Direction du mouvement, unitaire.
    pub direction: Vec3,
    /// Vitesse, en m/s.
    pub speed_mps: f32,
    /// Durée de vie, en s.
    pub life_s: f32,
    /// Âge, en s : au-delà de `life_s`, le trait est libre.
    pub age_s: f32,
    /// Indice de teinte : glace, menthe, azur, corail.
    pub tint: usize,
    /// Longueur du trait, en m.
    pub length_m: f32,
    /// Largeur du trait, en m.
    pub width_m: f32,
}

impl Streak {
    const FREE: Self = Self {
        position: Vec3::ZERO,
        direction: Vec3::NEG_Z,
        speed_mps: 0.0,
        life_s: 1.0,
        age_s: 1.0,
        tint: 0,
        length_m: 1.0,
        width_m: 1.0,
    };

    /// Le trait est libre (hors de l'écran, prêt à repartir).
    #[must_use]
    pub fn is_free(&self) -> bool {
        self.age_s >= self.life_s
    }

    /// Opacité, 0..1 : une cloche en sinus sur la durée de vie, d'exposant 0,7 pour l'élargir.
    #[must_use]
    pub fn alpha(&self) -> f32 {
        if self.is_free() {
            return 0.0;
        }
        // `sin(π)` vaut −8,7e−8 en `f32` : le plancher évite un `powf` de nombre négatif (NaN).
        ((self.age_s / self.life_s).min(1.0) * std::f32::consts::PI)
            .sin()
            .max(0.0)
            .powf(0.7)
    }

    /// Pose du quad : centre, orientation, échelle `(longueur, largeur, 1)`.
    ///
    /// Le plan est tourné vers le centre du ciel ; son axe X suit la direction du mouvement, et la tête (u = 1) est à la
    /// position du trait, la queue derrière.
    #[must_use]
    pub fn pose(&self) -> (Vec3, Quat, Vec3) {
        let toward_center = (-self.position).normalize_or_zero();
        let side = toward_center.cross(self.direction).normalize_or_zero();
        // Le trait avance en ligne droite alors que la normale tourne : on rend la base orthonormée (Sillage la laissait
        // légèrement cisaillée, ce que `setFromRotationMatrix` absorbait).
        let facing = self.direction.cross(side).normalize_or_zero();
        let rotation = if side == Vec3::ZERO || facing == Vec3::ZERO {
            Quat::IDENTITY
        } else {
            Quat::from_mat3(&Mat3::from_cols(self.direction, side, facing))
        };
        (
            self.position - self.direction * (self.length_m * 0.5),
            rotation,
            Vec3::new(self.length_m, self.width_m, 1.0),
        )
    }
}

/// Le pool d'étoiles filantes et son minuteur.
#[derive(Debug, Clone)]
pub struct ShootingStars {
    streaks: [Streak; POOL],
    countdown_s: f32,
    spawned: u32,
    clock_s: f64,
    /// Instants des derniers départs (anneau de `MAX_PER_SECOND`), pour la cadence maximale.
    launches: [f64; MAX_PER_SECOND],
    next_launch_slot: usize,
}

impl Default for ShootingStars {
    fn default() -> Self {
        Self::new()
    }
}

impl ShootingStars {
    /// Pool vide ; la première étoile part après 1,5 s (`countdown` de Sillage).
    #[must_use]
    pub fn new() -> Self {
        Self {
            streaks: [Streak::FREE; POOL],
            countdown_s: 1.5,
            spawned: 0,
            clock_s: 0.0,
            launches: [f64::NEG_INFINITY; MAX_PER_SECOND],
            next_launch_slot: 0,
        }
    }

    /// Les traits du pool, libres ou en vol.
    #[must_use]
    pub fn streaks(&self) -> &[Streak; POOL] {
        &self.streaks
    }

    /// Déclenche une étoile filante tout de suite. Ignoré (renvoie `None`) si le pool est plein ou si la cadence maximale
    /// serait dépassée (voir la sécurité photosensible plus haut). Renvoie l'indice du trait lancé.
    pub fn launch(&mut self) -> Option<usize> {
        let oldest = self.launches[self.next_launch_slot];
        let latest = self.launches[(self.next_launch_slot + MAX_PER_SECOND - 1) % MAX_PER_SECOND];
        if self.clock_s - oldest < 1.0 || self.clock_s - latest < MIN_GAP_S {
            return None;
        }
        let slot = self.streaks.iter().position(Streak::is_free)?;
        self.launches[self.next_launch_slot] = self.clock_s;
        self.next_launch_slot = (self.next_launch_slot + 1) % MAX_PER_SECOND;

        let k = self.spawned;
        self.spawned += 1;
        // Point de départ dans l'hémisphère avant, assez haut pour ne pas finir derrière les tours.
        let azimuth = (rand(k, 1) - 0.5) * std::f32::consts::PI * 1.5;
        let elevation = 0.28 + rand(k, 2) * 0.85;
        let position = Vec3::new(
            azimuth.sin() * elevation.cos(),
            elevation.sin(),
            -azimuth.cos() * elevation.cos(),
        ) * RADIUS_M;
        // Direction : tangente à la sphère, descendante, plutôt latérale.
        let heading = if rand(k, 3) < 0.5 { -1.0 } else { 1.0 } * (0.45 + rand(k, 4) * 0.5);
        let normal = position.normalize();
        let side = UP.cross(normal).normalize();
        let mut direction = side * heading + UP * (-0.35 - rand(k, 5) * 0.4);
        direction -= normal * direction.dot(normal);
        self.streaks[slot] = Streak {
            position,
            direction: direction.normalize_or(Vec3::NEG_Y),
            speed_mps: 420.0 + rand(k, 6) * 340.0,
            life_s: LIFE_S.0 + rand(k, 7) * LIFE_S.1,
            age_s: 0.0,
            tint: k as usize % TINT_COUNT,
            length_m: 90.0 + rand(k, 8) * 90.0,
            width_m: 1.6 + rand(k, 9) * 1.2,
        };
        Some(slot)
    }

    /// Avance d'une image : minuteur de la prochaine étoile, vol des traits. `calm` espace les départs
    /// (`reducedMotion` de Sillage).
    pub fn update(&mut self, dt_s: f32, calm: bool) {
        self.clock_s += f64::from(dt_s);
        self.countdown_s -= dt_s;
        if self.countdown_s <= 0.0 {
            // Un départ refusé (pool plein, cadence) n'est pas rejoué : on retire un délai comme s'il avait eu lieu.
            self.launch();
            let (base, span) = if calm { (9.0, 8.0) } else { (3.0, 5.0) };
            self.countdown_s = base + rand(self.spawned, 10) * span;
        }
        for streak in &mut self.streaks {
            if streak.is_free() {
                continue;
            }
            streak.age_s += dt_s;
            streak.position += streak.direction * (streak.speed_mps * dt_s);
        }
    }
}

#[cfg(test)]
#[path = "shooting_tests.rs"]
mod tests;
