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
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn la_premiere_etoile_part_apres_une_seconde_et_demie() {
        let mut stars = ShootingStars::new();
        let mut elapsed = 0.0;
        while stars.streaks().iter().all(Streak::is_free) {
            stars.update(DT, false);
            elapsed += DT;
            assert!(elapsed < 3.0, "aucun départ");
        }
        assert!((elapsed - 1.5).abs() < 0.05, "{elapsed}");
    }

    #[test]
    fn un_depart_reste_tangent_a_la_voute_et_descendant() {
        let mut stars = ShootingStars::new();
        for _ in 0..8 {
            let slot = stars.launch().expect("pool libre");
            let streak = stars.streaks()[slot];
            assert!((streak.position.length() - RADIUS_M).abs() < 1e-2);
            assert!(streak.direction.dot(streak.position.normalize()).abs() < 1e-4, "tangente");
            assert!((streak.direction.length() - 1.0).abs() < 1e-5);
            assert!(streak.direction.y < 0.0, "descendant : {:?}", streak.direction);
            assert!((0.7..=1.3).contains(&streak.life_s));
            assert!((420.0..=760.0).contains(&streak.speed_mps));
            // Libère le pool et laisse passer la cadence : 8 départs d'affilée.
            stars.clock_s += 2.0;
            stars.streaks = [Streak::FREE; POOL];
        }
    }

    #[test]
    fn le_depart_est_deterministe() {
        let a = {
            let mut s = ShootingStars::new();
            s.launch().map(|i| s.streaks()[i])
        };
        let b = {
            let mut s = ShootingStars::new();
            s.launch().map(|i| s.streaks()[i])
        };
        assert_eq!(a, b);
    }

    #[test]
    fn l_opacite_monte_et_descend_en_cloche_sans_jamais_etre_nan() {
        let mut streak = Streak::FREE;
        streak.life_s = 1.0;
        streak.age_s = 0.0;
        let mut previous = 0.0;
        let mut peak = 0.0f32;
        for step in 0..=100 {
            streak.age_s = step as f32 / 100.0 * 1.2;
            let alpha = streak.alpha();
            assert!(alpha.is_finite() && (0.0..=1.0).contains(&alpha), "{alpha}");
            peak = peak.max(alpha);
            previous = alpha;
        }
        assert!(peak > 0.99, "{peak}");
        assert!(previous.abs() < 1e-6, "libre à la fin");
        // Le sinus de π en `f32` est négatif : exactement l'âge qui donnait NaN sans plancher.
        streak.age_s = streak.life_s - f32::EPSILON;
        assert!(streak.alpha().is_finite());
    }

    #[test]
    fn la_montee_est_plus_lente_que_la_limite_photosensible() {
        // Variation maximale d'opacité d'une image à l'autre, à 60 i/s, sur toute la vie : jamais un saut brutal.
        let streak = Streak {
            life_s: LIFE_S.0,
            ..Streak::FREE
        };
        let mut worst = 0.0f32;
        let mut last = 0.0;
        for step in 1..(LIFE_S.0 / DT) as u32 {
            let alpha = Streak {
                age_s: step as f32 * DT,
                ..streak
            }
            .alpha();
            worst = worst.max((alpha - last).abs());
            last = alpha;
        }
        // Les 0,08 s de MIN_FADE font ~5 images : une montée de 0 à 1 en moins de cinq images serait un flash.
        // L'enveloppe en sinus^0,7 a sa pente maximale au tout début (0,7 de puissance), mais sur un trait de moins de 3 m
        // de large : l'écart d'une image reste sous 0,3.
        assert!(worst < 0.3, "{worst}");
    }

    #[test]
    fn la_cadence_respecte_trois_par_seconde_et_un_quart_de_seconde() {
        let mut stars = ShootingStars::new();
        let mut times = Vec::new();
        for frame in 0..600 {
            // Demande à chaque image pendant dix secondes : le limiteur tranche.
            if stars.launch().is_some() {
                times.push(f64::from(frame) * f64::from(DT));
            }
            stars.update(DT, false);
        }
        assert!(times.len() >= 10, "{times:?}");
        for pair in times.windows(2) {
            assert!(pair[1] - pair[0] >= MIN_GAP_S - 1e-6, "écart {pair:?}");
        }
        for window in times.windows(MAX_PER_SECOND + 1) {
            assert!(window[MAX_PER_SECOND] - window[0] >= 1.0 - 1e-6, "4 départs en moins d'une seconde : {window:?}");
        }
    }

    #[test]
    fn le_pool_plein_refuse_le_depart() {
        let mut stars = ShootingStars::new();
        let mut launched = 0;
        // Cinq départs possibles à la cadence maximale, puis le pool (vie ≥ 0,7 s) est encore occupé.
        for _ in 0..(3.0 / f64::from(DT)) as u32 {
            if stars.launch().is_some() {
                launched += 1;
            }
            stars.update(DT, false);
            let flying = stars.streaks().iter().filter(|s| !s.is_free()).count();
            assert!(flying <= POOL);
        }
        assert!(launched >= 2);
    }

    #[test]
    fn la_tete_est_a_la_position_et_la_base_est_orthonormee() {
        let mut stars = ShootingStars::new();
        let slot = stars.launch().expect("libre");
        for _ in 0..30 {
            stars.update(DT, true);
        }
        let streak = stars.streaks()[slot];
        let (center, rotation, scale) = streak.pose();
        // L'axe X local est la direction du mouvement ; la tête est à +longueur/2 du centre.
        let x = rotation * Vec3::X;
        assert!(x.dot(streak.direction) > 0.9999);
        let head = center + x * (scale.x * 0.5);
        assert!(head.distance(streak.position) < 1e-2);
        // La face du quad regarde vers le centre du ciel (la direction du vol, elle, ne tourne pas avec la normale).
        let face = rotation * Vec3::Z;
        assert!(face.dot(-streak.position.normalize()) > 0.7);
        assert!(face.dot(x).abs() < 1e-5, "base orthogonale");
        assert!((rotation.length() - 1.0).abs() < 1e-5);
    }

    /// Les départs contre ceux de `shooting-stars.ts` (voir `tools/sillage-golden/sky/`, `tests/golden/sky.json`).
    #[test]
    fn les_departs_sont_ceux_de_sillage() {
        let golden: serde_json::Value = serde_json::from_str(include_str!("../../tests/golden/sky.json")).expect("json");
        let launches = golden["launches"].as_array().expect("départs");
        assert!(!launches.is_empty());
        let near = |v: &serde_json::Value, expected: f32, tol: f32, what: &str| {
            let v = v.as_f64().expect("nombre") as f32;
            assert!((v - expected).abs() <= tol, "{what} : {v} contre {expected}");
        };
        let mut stars = ShootingStars::new();
        for launch in launches {
            // Un départ de plus, après la pause qu'il faut à la cadence maximale, sur un pool libéré : le trait de Sillage
            // repart de zéro de la même façon.
            stars.clock_s += 2.0;
            stars.streaks = [Streak::FREE; POOL];
            let slot = stars.launch().expect("départ");
            let streak = stars.streaks()[slot];
            let k = launch["k"].as_u64().expect("k");
            for (axis, value) in streak.position.to_array().iter().enumerate() {
                near(&launch["position"][axis], *value, 0.02, &format!("départ {k} position"));
            }
            for (axis, value) in streak.direction.to_array().iter().enumerate() {
                near(&launch["direction"][axis], *value, 1e-3, &format!("départ {k} direction"));
            }
            near(&launch["speed"], streak.speed_mps, 0.02, "vitesse");
            near(&launch["life"], streak.life_s, 1e-3, "vie");
            near(&launch["scale"][0], streak.length_m, 0.02, "longueur");
            near(&launch["scale"][1], streak.width_m, 1e-3, "largeur");
            // Teinte : numéro du rôle de couleur de la scène (glace 7, menthe 2, azur 3, corail 5).
            let roles = [7, 2, 3, 5];
            assert_eq!(launch["tint"].as_u64(), Some(roles[streak.tint]), "teinte du départ {k}");
        }
    }

    #[test]
    fn calme_espace_les_departs() {
        let count = |calm: bool| {
            let mut stars = ShootingStars::new();
            let mut launches = 0;
            let mut was_free = true;
            for _ in 0..(120.0 / f64::from(DT)) as u32 {
                stars.update(DT, calm);
                let any = stars.streaks().iter().any(|s| s.age_s < DT * 1.5);
                if any && was_free {
                    launches += 1;
                }
                was_free = !any;
            }
            launches
        };
        assert!(count(true) < count(false));
    }
}
