//! Les lampes que la route mouillée reflète : une vingtaine de points lumineux en repère rendu, choisis à chaque image.
//!
//! Le shader de la route ne connaît pas la scène : il reçoit ici de quoi calculer des reflets réalistes sans miroir ni
//! seconde caméra. Les huit premières lampes sont celles de la voiture (deux phares, deux feux arrière qui rougissent au
//! freinage, quatre néons de bas de caisse) ; les autres sont les points lumineux les plus forts autour d'elle dans les
//! [`Glints`](crate::traffic::glints::Glints) du trafic : gyrophares de la police, drones, feux de position, lanternes…
//!
//! Aucune allocation : le tableau est de taille fixe, la sélection garde les meilleurs candidats par insertion.

use bevy::color::LinearRgba;
use bevy::math::{Vec3, Vec4};
use bevy::render::render_resource::ShaderType;

use super::RoadLook;
use crate::traffic::glints::Glint;

/// Nombre de lampes du shader (`array<vec4<f32>, 24>`).
pub const MAX_LAMPS: usize = 24;
/// Lampes réservées à la voiture : le reste va aux points lumineux du trafic.
pub const CAR_LAMPS: usize = 8;

/// Fenêtre où un point lumineux du trafic mérite un reflet, par rapport à la voiture : latéralement, devant et derrière (m).
const MAX_SIDE_M: f32 = 75.0;
const MAX_AHEAD_M: f32 = 260.0;
const MAX_BEHIND_M: f32 = 35.0;

/// Le bloc d'uniformes des lampes. Même disposition que `Lamps` dans `road.wesl`.
#[derive(Debug, Clone, Copy, PartialEq, ShaderType)]
pub struct RoadLamps {
    /// `xyz` : position en repère rendu, `w` : rayon d'éclat, m.
    pub pos: [Vec4; MAX_LAMPS],
    /// `rgb` : couleur déjà multipliée par l'éclat (lumière linéaire). Nulle : lampe éteinte.
    pub color: [Vec4; MAX_LAMPS],
}

impl Default for RoadLamps {
    fn default() -> Self {
        Self {
            pos: [Vec4::new(0.0, -999.0, 0.0, 1.0); MAX_LAMPS],
            color: [Vec4::ZERO; MAX_LAMPS],
        }
    }
}

impl RoadLamps {
    fn set(&mut self, slot: usize, at: Vec3, radius: f32, color: [f32; 3]) {
        if let (Some(pos), Some(tint)) = (self.pos.get_mut(slot), self.color.get_mut(slot)) {
            *pos = at.extend(radius);
            *tint = Vec4::new(color[0], color[1], color[2], 0.0);
        }
    }

    /// Nombre de lampes allumées.
    #[must_use]
    pub fn lit(&self) -> usize {
        self.color.iter().filter(|c| c.x + c.y + c.z > 0.001).count()
    }
}

/// Ce que la sélection lit de l'image.
#[derive(Debug, Clone, Copy)]
pub struct LampInputs<'a> {
    /// La voiture, en repère rendu.
    pub car: Vec3,
    /// Les points lumineux du trafic : positions relatives à `glint_origin`.
    pub glints: &'a [Glint],
    /// L'origine des positions des points lumineux (la voiture à l'image où ils ont été posés), en repère rendu.
    pub glint_origin: Vec3,
    /// Ce que la route montre : palette, lueur sous la voiture, freinage.
    pub look: &'a RoadLook,
}

fn rgb(color: LinearRgba, gain: f32) -> [f32; 3] {
    [color.red * gain, color.green * gain, color.blue * gain]
}

/// Compose les lampes de l'image.
#[must_use]
pub fn build_lamps(input: &LampInputs<'_>) -> RoadLamps {
    let mut lamps = RoadLamps::default();
    let (car, look) = (input.car, input.look);
    let brake = look.brake.clamp(0.0, 1.0);

    // La voiture : phares à l'avant (vers −z), feux à l'arrière, néons de bas de caisse.
    let mut slot = 0;
    for sx in [-0.75_f32, 0.75] {
        lamps.set(slot, car + Vec3::new(sx, 0.62, -2.1), 0.32, rgb(look.palette.ice, 3.0));
        slot += 1;
    }
    for sx in [-0.75_f32, 0.75] {
        lamps.set(
            slot,
            car + Vec3::new(sx, 0.72, 2.0),
            0.3,
            rgb(look.palette.rose, 1.0 + brake * 3.0),
        );
        slot += 1;
    }
    for sx in [-0.92_f32, 0.92] {
        for sz in [-0.9_f32, 0.9] {
            lamps.set(slot, car + Vec3::new(sx, 0.3, sz), 0.25, rgb(look.glow, 1.2));
            slot += 1;
        }
    }
    debug_assert_eq!(slot, CAR_LAMPS);

    // Les points lumineux du trafic : les plus forts dans la fenêtre, par insertion dans un petit tableau trié.
    let mut best: [(f32, usize); MAX_LAMPS - CAR_LAMPS] = [(0.0, usize::MAX); MAX_LAMPS - CAR_LAMPS];
    for (index, glint) in input.glints.iter().enumerate() {
        let o = glint.offset;
        // La voiture est l'origine des points lumineux : `offset` est relatif à elle (à l'écart d'origine près).
        let from_car = o + (input.glint_origin - car);
        if from_car.x.abs() > MAX_SIDE_M
            || from_car.z > MAX_BEHIND_M
            || from_car.z < -MAX_AHEAD_M
            || o.y + input.glint_origin.y < 0.3
        {
            continue;
        }
        let power = (glint.color[0] + glint.color[1] + glint.color[2]) / 3.0 * glint.size;
        let score = power / (1.0 + (from_car.length() / 60.0).powi(2));
        if score <= 0.02 {
            continue;
        }
        // Insertion : on garde les meilleurs, du plus fort au plus faible.
        if let Some(rank) = best.iter().position(|&(s, _)| score > s) {
            let last = best.len() - 1;
            best.copy_within(rank..last, rank + 1);
            best[rank] = (score, index);
        }
    }
    for (_, index) in best {
        let Some(glint) = input.glints.get(index) else { continue };
        let at = input.glint_origin + glint.offset;
        lamps.set(
            slot,
            at,
            (glint.size * 0.5).clamp(0.2, 4.0),
            [glint.color[0] * 0.5, glint.color[1] * 0.5, glint.color[2] * 0.5],
        );
        slot += 1;
    }
    lamps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glint(offset: Vec3, intensity: f32, size: f32) -> Glint {
        Glint {
            offset,
            color: [intensity; 3],
            size,
        }
    }

    fn inputs<'a>(glints: &'a [Glint], look: &'a RoadLook) -> LampInputs<'a> {
        LampInputs {
            car: Vec3::new(0.0, 0.0, -500.0),
            glints,
            glint_origin: Vec3::new(0.0, 0.0, -500.0),
            look,
        }
    }

    #[test]
    fn la_voiture_allume_ses_huit_lampes_et_les_feux_rougissent_au_freinage() {
        let look = RoadLook::default();
        let lamps = build_lamps(&inputs(&[], &look));
        assert_eq!(lamps.lit(), CAR_LAMPS);
        let braking = RoadLook { brake: 1.0, ..look };
        let hard = build_lamps(&inputs(&[], &braking));
        // Les feux arrière sont aux rangs 2 et 3 : plus forts au freinage, et rouges.
        assert!(hard.color[2].x > lamps.color[2].x * 3.0);
        assert!(hard.color[2].x > hard.color[2].z);
        // Les phares (rangs 0 et 1) sont devant la voiture (z plus négatif).
        assert!(lamps.pos[0].z < -500.0 && lamps.pos[2].z > -500.0);
    }

    #[test]
    fn les_points_lumineux_les_plus_forts_gagnent_et_les_hors_champ_sont_ignores() {
        let look = RoadLook::default();
        let mut glints = Vec::new();
        // 40 points proches, d'éclat croissant : seuls les 16 plus forts passent.
        for i in 0..40 {
            glints.push(glint(Vec3::new(3.0, 6.0, -20.0 - i as f32), 0.1 + i as f32 * 0.1, 2.0));
        }
        // Un point très fort mais derrière la voiture, un autre trop loin sur le côté, un troisième sous le sol.
        glints.push(glint(Vec3::new(0.0, 5.0, 200.0), 50.0, 5.0));
        glints.push(glint(Vec3::new(500.0, 5.0, -20.0), 50.0, 5.0));
        glints.push(glint(Vec3::new(0.0, -5.0, -20.0), 50.0, 5.0));
        let lamps = build_lamps(&inputs(&glints, &look));
        assert_eq!(lamps.lit(), MAX_LAMPS);
        // Les lampes du trafic sont triées du plus fort au plus faible.
        let powers: Vec<f32> = (CAR_LAMPS..MAX_LAMPS).map(|i| lamps.color[i].x).collect();
        assert!(powers.windows(2).all(|w| w[0] >= w[1] - 1e-6), "{powers:?}");
        // Le plus faible retenu est plus fort que les 24 écartés (éclat 0,1 à 2,4, soit une couleur de 1,2 au plus).
        assert!(powers[powers.len() - 1] > 1.2, "{powers:?}");
        // Aucune lampe n'est derrière, loin sur le côté ou sous le sol.
        for i in CAR_LAMPS..MAX_LAMPS {
            let p = lamps.pos[i];
            assert!(p.x.abs() < 75.0 && p.z < -500.0 + 35.0 && p.y > 0.3, "{p:?}");
        }
    }

    #[test]
    fn les_positions_suivent_l_origine_des_points_lumineux() {
        let look = RoadLook::default();
        let glints = [glint(Vec3::new(1.0, 5.0, -30.0), 2.0, 3.0)];
        let mut input = inputs(&glints, &look);
        // Les points ont été posés une image plus tôt, quand la voiture était 2 m derrière.
        input.glint_origin = Vec3::new(0.0, 0.0, -498.0);
        let lamps = build_lamps(&input);
        assert!(
            (lamps.pos[CAR_LAMPS].z + 528.0).abs() < 1e-4,
            "{:?}",
            lamps.pos[CAR_LAMPS]
        );
    }
}
