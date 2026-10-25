//! Points lumineux additifs (`glints.ts`) : gyrophares, drones, feux de position, lampadaires. Les appelants repeuplent
//! la liste à chaque image ([`Glints::begin`], [`Glints::add`]…) ; un seul dessin pour tout (voir `render`).
//!
//! Les positions sont gardées **relatives à l'origine de l'image** (la voiture) : les sommets du maillage de rendu
//! restent petits et précis à 400 km du départ.

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};

/// Nombre de points par image (`CAPACITY`).
pub const GLINT_CAPACITY: usize = 320;

/// Un point lumineux.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Glint {
    /// Position relative à l'origine de l'image, m.
    pub offset: Vec3,
    /// Couleur déjà multipliée par l'intensité (au-delà de 1, le bloom s'en empare).
    pub color: [f32; 3],
    /// Diamètre en mètres.
    pub size: f32,
}

/// La liste des points de l'image. Capacité fixe : au-delà, les suivants sont ignorés.
#[derive(Debug, Clone)]
pub struct Glints {
    points: Vec<Glint>,
    count: usize,
    origin: DVec3,
}

impl Default for Glints {
    fn default() -> Self {
        Self::new()
    }
}

impl Glints {
    /// Une liste vide, de capacité [`GLINT_CAPACITY`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            points: vec![Glint::default(); GLINT_CAPACITY],
            count: 0,
            origin: DVec3::ZERO,
        }
    }

    /// Vide la liste ; `origin` (absolue) est le repère des sommets de l'image.
    pub fn begin(&mut self, origin: DVec3) {
        self.count = 0;
        self.origin = origin;
    }

    /// L'origine des positions de l'image, absolue.
    #[must_use]
    pub fn origin(&self) -> DVec3 {
        self.origin
    }

    /// Nombre de points posés cette image.
    #[must_use]
    pub fn used(&self) -> usize {
        self.count
    }

    /// Les points posés cette image.
    #[must_use]
    pub fn points(&self) -> &[Glint] {
        &self.points[..self.count]
    }

    /// Pose un point à la position absolue `at`. `size` en mètres ; la couleur est multipliée par `intensity`.
    /// Ignoré quand la liste est pleine.
    pub fn add(&mut self, at: DVec3, size: f32, color: LinearRgba, intensity: f32) {
        let Some(slot) = self.points.get_mut(self.count) else {
            return;
        };
        *slot = Glint {
            offset: (at - self.origin).as_vec3(),
            color: [color.red * intensity, color.green * intensity, color.blue * intensity],
            size,
        };
        self.count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_liste_est_bornee_et_se_vide() {
        let mut glints = Glints::new();
        glints.begin(DVec3::new(0.0, 0.0, -400_000.0));
        for i in 0..(GLINT_CAPACITY + 50) {
            glints.add(DVec3::new(0.0, 5.0, -400_000.0 - i as f64), 1.0, LinearRgba::WHITE, 2.0);
        }
        assert_eq!(glints.used(), GLINT_CAPACITY);
        glints.begin(DVec3::ZERO);
        assert_eq!(glints.used(), 0);
    }

    #[test]
    fn les_positions_sont_relatives_et_precises_loin_du_depart() {
        let mut glints = Glints::new();
        let car = DVec3::new(0.0, 0.0, -400_000.0);
        glints.begin(car);
        glints.add(
            car + DVec3::new(1.0, 2.0, -3.25),
            1.0,
            LinearRgba::rgb(0.5, 1.0, 0.0),
            2.0,
        );
        let g = glints.points()[0];
        assert_eq!(g.offset, Vec3::new(1.0, 2.0, -3.25));
        assert_eq!(g.color, [1.0, 2.0, 0.0]);
    }
}
