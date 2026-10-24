//! Les murs de lumière des motos (`walls.ts`) : un ruban vertical qui reste dans le monde là où la moto est passée,
//! comme dans Tron. Chaque ruban est une bande de triangles (deux sommets par point, bas et haut) dont la teinte vient
//! de la moto ; il s'éteint en quelques secondes. Un seul maillage pour tous les rubans (voir `render`).
//!
//! Logique pure, sans allocation après la création : les points de chaque ruban vivent dans des tableaux de taille
//! fixe, le plus récent en premier.

use bevy::color::LinearRgba;
use bevy::math::DVec3;

/// Rubans : un par moto, avec assez de places pour un événement de cinq plus la queue des précédents.
pub const RIBBONS: usize = 8;
/// Points par ruban (la tête vivante comprise).
pub const SAMPLES: usize = 96;
/// Espacement minimal de deux points, m.
const STEP_M: f64 = 1.4;
/// Durée de vie d'un point, s.
const LIFE_S: f64 = 3.6;
/// Hauteur du mur, m.
pub const HEIGHT_M: f32 = 1.35;
/// Hauteur du pied du mur au-dessus de la route, m.
const FOOT_M: f32 = 0.04;
/// Sommets du maillage : deux par point.
pub const VERTEX_COUNT: usize = RIBBONS * SAMPLES * 2;

/// Poignée d'un ruban réservé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WallId(u8);

#[derive(Debug, Clone)]
struct Ribbon {
    active: bool,
    finished: bool,
    started: bool,
    tint: LinearRgba,
    head: (f64, f64),
    count: usize,
    xs: [f64; SAMPLES],
    zs: [f64; SAMPLES],
    ts: [f64; SAMPLES],
}

impl Ribbon {
    const fn idle() -> Self {
        Self {
            active: false,
            finished: true,
            started: false,
            tint: LinearRgba::BLACK,
            head: (0.0, 0.0),
            count: 0,
            xs: [0.0; SAMPLES],
            zs: [0.0; SAMPLES],
            ts: [0.0; SAMPLES],
        }
    }
}

/// Les sommets à écrire dans le maillage de rendu : trois tableaux de [`VERTEX_COUNT`] éléments.
#[derive(Debug)]
pub struct WallVertices<'a> {
    /// Positions relatives à l'origine de l'image.
    pub positions: &'a mut [[f32; 3]],
    /// Teinte (rgb) ; l'alpha vaut 1.
    pub colors: &'a mut [[f32; 4]],
    /// `x` : opacité (âge), `y` : hauteur 0 (pied) → 1 (haut du mur).
    pub fades: &'a mut [[f32; 2]],
}

/// Tous les murs de lumière.
#[derive(Debug, Clone)]
pub struct LightWalls {
    ribbons: Vec<Ribbon>,
}

impl Default for LightWalls {
    fn default() -> Self {
        Self::new()
    }
}

impl LightWalls {
    /// Aucun ruban actif.
    #[must_use]
    pub fn new() -> Self {
        Self {
            ribbons: vec![Ribbon::idle(); RIBBONS],
        }
    }

    /// Rubans en vie (la moto roule encore, ou la queue brille encore).
    #[must_use]
    pub fn active(&self) -> usize {
        self.ribbons.iter().filter(|r| r.active).count()
    }

    /// Réserve un ruban à la teinte donnée. `None` si tous sont pris (l'événement s'en passe).
    pub fn claim(&mut self, tint: LinearRgba) -> Option<WallId> {
        let index = self.ribbons.iter().position(|r| !r.active)?;
        let ribbon = &mut self.ribbons[index];
        *ribbon = Ribbon {
            active: true,
            finished: false,
            tint,
            ..Ribbon::idle()
        };
        Some(WallId(index as u8))
    }

    /// Pose la tête du ruban (position au sol de la roue arrière) ; ajoute un point si on a avancé assez.
    pub fn push(&mut self, id: WallId, x: f64, z: f64, time: f64) {
        let Some(ribbon) = self.ribbons.get_mut(usize::from(id.0)).filter(|r| r.active) else {
            return;
        };
        ribbon.head = (x, z);
        if ribbon.started && (x - ribbon.xs[0]).hypot(z - ribbon.zs[0]) < STEP_M {
            return;
        }
        ribbon.started = true;
        // Le plus récent en premier ; le plus ancien tombe quand la place manque (la tête vivante prend la dernière).
        let keep = ribbon.count.min(SAMPLES - 2);
        ribbon.xs.copy_within(0..keep, 1);
        ribbon.zs.copy_within(0..keep, 1);
        ribbon.ts.copy_within(0..keep, 1);
        ribbon.xs[0] = x;
        ribbon.zs[0] = z;
        ribbon.ts[0] = time;
        ribbon.count = (ribbon.count + 1).min(SAMPLES - 1);
    }

    /// La moto est partie : le mur n'avance plus et s'éteint tout seul.
    pub fn finish(&mut self, id: WallId) {
        if let Some(ribbon) = self.ribbons.get_mut(usize::from(id.0)) {
            ribbon.finished = true;
        }
    }

    /// Fait vieillir les points et libère les rubans éteints.
    pub fn update(&mut self, time: f64) {
        for ribbon in self.ribbons.iter_mut().filter(|r| r.active) {
            while ribbon.count > 0 && time - ribbon.ts[ribbon.count - 1] > LIFE_S {
                ribbon.count -= 1;
            }
            if ribbon.finished && ribbon.count == 0 {
                *ribbon = Ribbon::idle();
            }
        }
    }

    /// Écrit tous les sommets : pour chaque ruban, la tête vivante puis les points du plus récent au plus ancien ;
    /// les places inutilisées sont repliées sur le dernier point, sans opacité, pour ne rien dessiner.
    pub fn write(&self, origin: DVec3, time: f64, out: &mut WallVertices<'_>) {
        for (r, ribbon) in self.ribbons.iter().enumerate() {
            if !ribbon.active {
                let start = r * SAMPLES * 2;
                out.positions[start..start + SAMPLES * 2].fill([0.0; 3]);
                out.fades[start..start + SAMPLES * 2].fill([0.0; 2]);
                continue;
            }
            let use_head = !ribbon.finished;
            let tint = [ribbon.tint.red, ribbon.tint.green, ribbon.tint.blue, 1.0];
            let (mut last_x, mut last_z) = ribbon.head;
            for i in 0..SAMPLES {
                let j = if use_head { i.checked_sub(1) } else { Some(i) };
                let (x, z, alpha) = match j {
                    None => (ribbon.head.0, ribbon.head.1, 1.0),
                    Some(j) if j < ribbon.count => {
                        let age = ((time - ribbon.ts[j]) / LIFE_S).min(1.0);
                        (ribbon.xs[j], ribbon.zs[j], ((1.0 - age).max(0.0)).powf(1.5) as f32)
                    }
                    Some(_) => (last_x, last_z, 0.0),
                };
                (last_x, last_z) = (x, z);
                let (dx, dz) = ((x - origin.x) as f32, (z - origin.z) as f32);
                let v = (r * SAMPLES + i) * 2;
                out.positions[v] = [dx, FOOT_M - origin.y as f32, dz];
                out.positions[v + 1] = [dx, HEIGHT_M - origin.y as f32, dz];
                out.colors[v] = tint;
                out.colors[v + 1] = tint;
                out.fades[v] = [alpha, 0.0];
                out.fades[v + 1] = [alpha, 1.0];
            }
        }
    }
}

/// Les indices de la bande de triangles de tous les rubans : fixes, construits une fois.
#[must_use]
pub fn wall_indices() -> Vec<u32> {
    let mut index = Vec::with_capacity(RIBBONS * (SAMPLES - 1) * 6);
    for r in 0..RIBBONS {
        for i in 0..SAMPLES - 1 {
            let a = ((r * SAMPLES + i) * 2) as u32;
            let b = a + 2;
            index.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
        }
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffers() -> (Vec<[f32; 3]>, Vec<[f32; 4]>, Vec<[f32; 2]>) {
        (vec![[0.0; 3]; VERTEX_COUNT], vec![[0.0; 4]; VERTEX_COUNT], vec![[0.0; 2]; VERTEX_COUNT])
    }

    #[test]
    fn huit_rubans_au_plus_puis_le_neuvieme_est_refuse() {
        let mut walls = LightWalls::new();
        for _ in 0..RIBBONS {
            assert!(walls.claim(LinearRgba::WHITE).is_some());
        }
        assert!(walls.claim(LinearRgba::WHITE).is_none());
        assert_eq!(walls.active(), RIBBONS);
    }

    #[test]
    fn un_point_n_est_ajoute_qu_apres_un_pas_assez_grand() {
        let mut walls = LightWalls::new();
        let id = walls.claim(LinearRgba::WHITE).expect("libre");
        walls.push(id, 0.0, 0.0, 0.0);
        walls.push(id, 0.0, -0.5, 0.01);
        assert_eq!(walls.ribbons[0].count, 1);
        walls.push(id, 0.0, -1.5, 0.02);
        assert_eq!(walls.ribbons[0].count, 2);
        assert!((walls.ribbons[0].zs[0] + 1.5).abs() < 1e-12, "le plus récent d'abord");
    }

    #[test]
    fn le_ruban_ne_depasse_pas_sa_capacite_et_s_eteint_apres_la_moto() {
        let mut walls = LightWalls::new();
        let id = walls.claim(LinearRgba::WHITE).expect("libre");
        for i in 0..(SAMPLES * 2) {
            walls.push(id, 0.0, -(i as f64) * 2.0, i as f64 * 0.01);
        }
        assert_eq!(walls.ribbons[0].count, SAMPLES - 1);
        walls.finish(id);
        walls.update(100.0);
        assert_eq!(walls.active(), 0, "tous les points ont expiré");
        // Un ruban libéré se réserve de nouveau.
        assert!(walls.claim(LinearRgba::BLACK).is_some());
    }

    #[test]
    fn les_sommets_sont_relatifs_et_les_places_vides_sont_repliees() {
        let mut walls = LightWalls::new();
        let id = walls.claim(LinearRgba::rgb(1.0, 0.5, 0.25)).expect("libre");
        let origin = DVec3::new(0.0, 0.0, -1000.0);
        walls.push(id, 1.0, -1005.0, 0.0);
        let (mut p, mut c, mut f) = buffers();
        walls.write(origin, 0.1, &mut WallVertices { positions: &mut p, colors: &mut c, fades: &mut f });
        // Tête vivante : opaque, au pied puis en haut du mur.
        assert_eq!(p[0], [1.0, FOOT_M, -5.0]);
        assert_eq!(p[1], [1.0, HEIGHT_M, -5.0]);
        assert_eq!(f[0], [1.0, 0.0]);
        assert_eq!(f[1], [1.0, 1.0]);
        assert_eq!(c[0], [1.0, 0.5, 0.25, 1.0]);
        // Les places suivantes sont repliées sur le dernier point (le point posé), transparentes ou presque.
        assert_eq!(p[(SAMPLES - 1) * 2][0], p[2][0]);
        assert_eq!(f[(SAMPLES - 1) * 2][0], 0.0);
        // Un ruban inactif ne dessine rien.
        assert_eq!(f[SAMPLES * 2], [0.0, 0.0]);
    }

    #[test]
    fn les_indices_couvrent_chaque_bande_sans_sortir_des_sommets() {
        let indices = wall_indices();
        assert_eq!(indices.len(), RIBBONS * (SAMPLES - 1) * 6);
        assert!(indices.iter().all(|&i| (i as usize) < VERTEX_COUNT));
    }
}
