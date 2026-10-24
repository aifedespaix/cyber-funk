//! La traînée lumineuse : une bande qui mémorise les positions passées de l'arrière de la voiture (`trail.ts`).
//!
//! « Chaque point garde la teinte qu'avait la voiture en le posant (celle du dernier projecteur traversé) et s'estompe
//! avec l'âge ; la longueur visible suit la vitesse. Un tampon circulaire évite toute allocation par image. »
//!
//! Les points sont gardés en coordonnées **absolues** (`f64`) : à 400 km du départ, `f32` n'a plus que le centimètre.
//! Le maillage, lui, est écrit en repère rendu (relatif à l'origine flottante) à chaque image, par [`Trail::write`] qui
//! remplit les tableaux d'attributs du `Mesh` en place : aucune allocation en régime établi.

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};

/// Nombre de points mémorisés (`POINTS`).
pub const POINTS: usize = 130;
/// Demi-largeur maximale de la bande, m (`WIDTH`).
const WIDTH: f32 = 0.55;
/// Distance minimale entre deux points, m : la longueur ne dépend pas du taux d'images (`STEP`).
const STEP: f64 = 0.35;
/// Plus petite part de la longueur visible.
const MIN_LENGTH: f32 = 0.15;

/// Nombre de sommets du maillage : deux (gauche, droite) par point.
pub const VERTICES: usize = POINTS * 2;
/// Nombre d'indices du maillage : deux triangles entre deux points consécutifs.
pub const INDICES: usize = (POINTS - 1) * 6;

/// Les indices du maillage, constants : le tampon des sommets change, pas la topologie.
#[must_use]
pub fn indices() -> Vec<u32> {
    let mut out = Vec::with_capacity(INDICES);
    for i in 0..(POINTS - 1) as u32 {
        let a = i * 2;
        out.extend_from_slice(&[a, a + 1, a + 2, a + 1, a + 3, a + 2]);
    }
    out
}

/// La mémoire de la bande.
#[derive(Debug, Clone)]
pub struct Trail {
    history: [DVec3; POINTS],
    tints: [LinearRgba; POINTS],
    head: usize,
    filled: usize,
    last: Option<(f64, f64)>,
    length: f32,
}

impl Default for Trail {
    fn default() -> Self {
        Self {
            history: [DVec3::ZERO; POINTS],
            tints: [LinearRgba::BLACK; POINTS],
            head: 0,
            filled: 0,
            last: None,
            length: 1.0,
        }
    }
}

impl Trail {
    /// Nombre de points posés.
    #[must_use]
    pub fn len(&self) -> usize {
        self.filled
    }

    /// La bande est-elle vide ?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.filled == 0
    }

    /// Part de la longueur visible, 0 à 1 : la traînée se raccourcit quand la voiture ralentit.
    pub fn set_length(&mut self, fraction: f32) {
        self.length = fraction.clamp(MIN_LENGTH, 1.0);
    }

    /// Ajoute le point `anchor` (arrière de la voiture, absolu) dans la teinte `tint`. Ignoré s'il est à moins de
    /// [`STEP`] du précédent (mesuré dans le plan de la route).
    pub fn push(&mut self, anchor: DVec3, tint: LinearRgba) {
        if let Some((x, z)) = self.last
            && (anchor.x - x).hypot(anchor.z - z) < STEP
        {
            return;
        }
        self.last = Some((anchor.x, anchor.z));
        self.history[self.head] = anchor;
        self.tints[self.head] = tint;
        self.head = (self.head + 1) % POINTS;
        self.filled = (self.filled + 1).min(POINTS);
    }

    /// Efface la mémoire (en vol, pas de traînée : elle repart à zéro à l'atterrissage).
    pub fn clear(&mut self) {
        self.filled = 0;
        self.head = 0;
        self.last = None;
    }

    /// Remplit les sommets (repère rendu, `origin` = origine flottante) et leurs couleurs (RGBA linéaire, en lumière
    /// additive : la couleur porte déjà l'opacité). Retourne la boîte englobante `(min, max)` des points posés, pour
    /// l'`Aabb` : la bande suit la voiture, son boîtier ne peut pas rester celui du premier maillage.
    ///
    /// `positions` et `colors` doivent avoir [`VERTICES`] éléments.
    pub fn write(&self, origin: DVec3, positions: &mut [[f32; 3]], colors: &mut [[f32; 4]]) -> Option<(Vec3, Vec3)> {
        debug_assert!(positions.len() >= VERTICES && colors.len() >= VERTICES);
        let mut bounds: Option<(Vec3, Vec3)> = None;
        // Les points pas encore posés se replient sur le plus ancien : triangles dégénérés, rien à dessiner.
        let oldest = match self.filled {
            0 => Vec3::ZERO,
            n => (self.history[(self.head + POINTS - n) % POINTS] - origin).as_vec3(),
        };
        for i in 0..POINTS {
            // i = 0 : point le plus récent ; i croît vers le plus ancien.
            let index = (self.head + POINTS - 1 - i) % POINTS;
            let (left, right) = (i * 2, i * 2 + 1);
            if i >= self.filled {
                positions[left] = oldest.to_array();
                positions[right] = oldest.to_array();
                colors[left] = [0.0; 4];
                colors[right] = [0.0; 4];
                continue;
            }
            let life = (1.0 - i as f32 / (POINTS as f32 * self.length)).max(0.0);
            let point: Vec3 = (self.history[index] - origin).as_vec3();
            let half = WIDTH * (0.35 + life * 0.65);
            positions[left] = [point.x - half, point.y, point.z];
            positions[right] = [point.x + half, point.y, point.z];
            let (lo, hi) = bounds.unwrap_or((point, point));
            bounds = Some((lo.min(point - Vec3::X * half), hi.max(point + Vec3::X * half)));
            // Le cœur de la traînée blanchit un peu : elle paraît plus chaude à la source.
            let alpha = life * life * 0.6;
            let core = life.powi(6) * 0.35;
            let tint = self.tints[index];
            let lit = |c: f32| (c + (1.0 - c) * core) * alpha;
            let color = [lit(tint.red), lit(tint.green), lit(tint.blue), 1.0];
            colors[left] = color;
            colors[right] = color;
        }
        bounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arrays() -> (Vec<[f32; 3]>, Vec<[f32; 4]>) {
        (vec![[0.0; 3]; VERTICES], vec![[0.0; 4]; VERTICES])
    }

    fn violet() -> LinearRgba {
        LinearRgba::rgb(0.14, 0.07, 0.54)
    }

    #[test]
    fn deux_triangles_relient_deux_points_consecutifs() {
        let indices = indices();
        assert_eq!(indices.len(), INDICES);
        assert_eq!(&indices[..6], &[0, 1, 2, 1, 3, 2]);
        assert!(indices.iter().all(|&i| (i as usize) < VERTICES));
    }

    #[test]
    fn les_points_trop_proches_sont_ignores() {
        let mut trail = Trail::default();
        trail.push(DVec3::new(0.0, 0.5, 0.0), violet());
        trail.push(DVec3::new(0.0, 0.5, -0.2), violet());
        assert_eq!(trail.len(), 1, "à moins de 0,35 m du précédent");
        trail.push(DVec3::new(0.0, 0.5, -0.4), violet());
        assert_eq!(trail.len(), 2);
    }

    /// À cadence élevée (le pas par image est sous 0,35 m), la longueur de la bande ne dépend plus de la cadence : 10 m
    /// avancés en 300 ou en 3 000 images donnent autant de points. (À 60 i/s et 40 m/s, chaque image pose un point.)
    #[test]
    fn la_longueur_ne_depend_pas_du_taux_d_images() {
        let count = |frames: usize| {
            let mut trail = Trail::default();
            for i in 0..=frames {
                trail.push(DVec3::new(0.0, 0.5, -10.0 * i as f64 / frames as f64), violet());
            }
            trail.len()
        };
        assert!(count(3000).abs_diff(count(300)) <= 1, "{} vs {}", count(3000), count(300));
    }

    #[test]
    fn le_tampon_circulaire_garde_les_130_derniers_points() {
        let mut trail = Trail::default();
        for i in 0..400 {
            trail.push(DVec3::new(0.0, 0.5, -f64::from(i)), violet());
        }
        assert_eq!(trail.len(), POINTS);
        let (mut positions, mut colors) = arrays();
        trail.write(DVec3::ZERO, &mut positions, &mut colors);
        // Le plus récent (i = 0) est le dernier posé, le plus ancien 129 points derrière.
        assert!((positions[0][2] - -399.0).abs() < 1e-4);
        assert!((positions[(POINTS - 1) * 2][2] - -(399.0 - 129.0)).abs() < 1e-4);
    }

    #[test]
    fn la_bande_s_estompe_avec_l_age_et_s_affine() {
        let mut trail = Trail::default();
        for i in 0..POINTS {
            trail.push(DVec3::new(0.0, 0.5, -(i as f64)), violet());
        }
        let (mut positions, mut colors) = arrays();
        trail.write(DVec3::ZERO, &mut positions, &mut colors);
        let brightness = |i: usize| colors[i * 2][0] + colors[i * 2][1] + colors[i * 2][2];
        assert!(brightness(0) > brightness(40) && brightness(40) > brightness(100));
        assert!(brightness(POINTS - 1) < 1e-3, "le plus ancien est éteint");
        let width = |i: usize| positions[i * 2 + 1][0] - positions[i * 2][0];
        assert!((width(0) - 2.0 * 0.55).abs() < 1e-5, "pleine largeur à la source : {}", width(0));
        assert!(width(100) < width(0));
        // L'alpha reste à 1 : en lumière additive, la couleur porte l'opacité.
        assert!((colors[0][3] - 1.0).abs() < f32::EPSILON);
    }

    /// Plus la voiture ralentit, plus la bande visible se raccourcit : à longueur 0,15 elle est éteinte au-delà de 15 %.
    #[test]
    fn la_longueur_visible_suit_la_vitesse() {
        let mut trail = Trail::default();
        for i in 0..POINTS {
            trail.push(DVec3::new(0.0, 0.5, -(i as f64)), violet());
        }
        let (mut positions, mut colors) = arrays();
        trail.set_length(0.3);
        trail.write(DVec3::ZERO, &mut positions, &mut colors);
        assert!(colors[60 * 2][0] < 1e-6, "au-delà de 30 % de la mémoire : éteint");
        assert!(colors[10 * 2][0] > 0.0);
        trail.set_length(0.0);
        trail.write(DVec3::ZERO, &mut positions, &mut colors);
        assert!(colors[30 * 2][0] < 1e-6, "plancher à 15 %");
        assert!(colors[5 * 2][0] > 0.0);
    }

    /// À 400 km, les positions rendues restent précises : l'absolu est en `f64`, seule la différence passe en `f32`.
    #[test]
    fn la_precision_tient_a_400_km() {
        let mut trail = Trail::default();
        let base = -400_000.0;
        for i in 0..5 {
            trail.push(DVec3::new(0.3, 0.5, base - f64::from(i) * 0.5), violet());
        }
        let origin = DVec3::new(0.0, 0.0, base);
        let (mut positions, mut colors) = arrays();
        let bounds = trail.write(origin, &mut positions, &mut colors).expect("points posés");
        assert!((positions[0][2] - -2.0).abs() < 1e-6, "{}", positions[0][2]);
        assert!((positions[2][2] - -1.5).abs() < 1e-6);
        assert!(bounds.0.z <= -2.0 && bounds.1.z >= 0.0);
    }

    #[test]
    fn effacer_repart_de_zero() {
        let mut trail = Trail::default();
        trail.push(DVec3::ZERO, violet());
        trail.push(DVec3::new(0.0, 0.0, -1.0), violet());
        trail.clear();
        assert!(trail.is_empty());
        // Le premier point d'après n'est pas comparé à l'ancien dernier.
        trail.push(DVec3::new(0.0, 0.0, -1.0), violet());
        assert_eq!(trail.len(), 1);
    }
}
