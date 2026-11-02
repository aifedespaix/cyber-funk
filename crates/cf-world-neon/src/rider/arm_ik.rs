//! Les bras du conducteur : cinématique inverse à deux segments, pour que ses mains suivent le volant qui tourne.
//!
//! Le GLB du conducteur (`tools/char/bake_rider.py`) découpe chaque bras en deux pièces rigides, `arm_X_up` (pivot à
//! l'épaule) et `arm_X_low` (pivot au coude, avec l'avant-bras, la main et les doigts figés), plus un nœud `wrist_X` au
//! poignet. Le lecteur de GLB ne lit pas de squelette : on tourne ces pivots à la main.
//!
//! Tout est calculé dans le repère du modèle, où les pivots n'ont qu'une translation (pas de rotation de repos).

use bevy::math::{Quat, Vec3};

/// La géométrie d'un bras à sa pose de repos, repère du modèle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArmGeometry {
    /// Épaule : pivot de `arm_X_up`.
    pub shoulder: Vec3,
    /// Coude : pivot de `arm_X_low`.
    pub elbow: Vec3,
    /// Poignet au repos : c'est ce point qui suit le volant.
    pub wrist: Vec3,
    /// Où va le coude (direction approximative, repère du modèle). Au repos le bras est presque tendu : son coude est trop
    /// près de la corde pour en donner le côté, il faut le dire.
    pub pole: Vec3,
}

/// Rotations locales à donner aux pivots : (`arm_X_up`, `arm_X_low`).
pub type ArmPose = (Quat, Quat);

impl ArmGeometry {
    /// Longueur du bras (épaule au coude).
    #[must_use]
    pub fn upper(&self) -> f32 {
        self.elbow.distance(self.shoulder)
    }

    /// Longueur de l'avant-bras (coude au poignet).
    #[must_use]
    pub fn lower(&self) -> f32 {
        self.wrist.distance(self.elbow)
    }

    /// Les rotations qui amènent le poignet sur `target` (ramenée à la portée du bras s'il est trop loin ou trop près).
    ///
    /// Le coude va du côté du pôle (`pole`, ramené perpendiculaire à l'épaule-poignet). La torsion autour de l'axe des segments n'est pas gérée : les rotations sont les plus courtes qui conviennent.
    #[must_use]
    pub fn solve(&self, target: Vec3) -> ArmPose {
        let (l1, l2) = (self.upper(), self.lower());
        let chord0 = (self.wrist - self.shoulder).normalize_or_zero();
        let to_target = target - self.shoulder;
        let dir = if to_target.length_squared() > 1e-8 {
            to_target.normalize()
        } else {
            chord0
        };
        let reach = to_target.length().clamp((l1 - l2).abs() + 1e-3, l1 + l2 - 1e-3);
        let pole = self.pole - dir * self.pole.dot(dir);
        let pole = if pole.length_squared() > 1e-8 {
            pole.normalize()
        } else {
            dir.any_orthonormal_vector()
        };
        // Loi des cosinus : angle à l'épaule entre la corde et le bras.
        let cos_a = ((l1 * l1 + reach * reach - l2 * l2) / (2.0 * l1 * reach)).clamp(-1.0, 1.0);
        let sin_a = (1.0 - cos_a * cos_a).sqrt();
        let elbow = self.shoulder + dir * (l1 * cos_a) + pole * (l1 * sin_a);
        let wrist = self.shoulder + dir * reach;
        let up = Quat::from_rotation_arc(
            (self.elbow - self.shoulder).normalize_or_zero(),
            (elbow - self.shoulder).normalize_or_zero(),
        );
        let low_world = Quat::from_rotation_arc(
            (self.wrist - self.elbow).normalize_or_zero(),
            (wrist - elbow).normalize_or_zero(),
        );
        (up, up.inverse() * low_world)
    }

    /// Position du poignet pour des rotations données (repère du modèle) : sert aux tests et à la vérification.
    #[must_use]
    pub fn wrist_for(&self, pose: ArmPose) -> Vec3 {
        let elbow = self.shoulder + pose.0 * (self.elbow - self.shoulder);
        elbow + (pose.0 * pose.1) * (self.wrist - self.elbow)
    }
}

/// Le point du volant que tient la main, tourné de `angle` (rad, > 0 vers la gauche vue du conducteur) autour de l'axe de la
/// colonne : `centre` est un point de l'axe, `axis` sa direction vers le conducteur (unitaire).
#[must_use]
pub fn turned_grip(grip: Vec3, centre: Vec3, axis: Vec3, angle: f32) -> Vec3 {
    centre + Quat::from_axis_angle(axis, angle) * (grip - centre)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un bras plié, avec de la marge pour que les cibles des tests soient à portée.
    fn arm() -> ArmGeometry {
        ArmGeometry {
            shoulder: Vec3::new(0.702, 1.187, -0.014),
            elbow: Vec3::new(0.670, 1.050, 0.200),
            wrist: Vec3::new(0.637, 1.209, 0.380),
            pole: Vec3::new(0.0, -1.0, 0.0),
        }
    }

    #[test]
    fn au_repos_les_rotations_sont_nulles() {
        let a = arm();
        let (up, low) = a.solve(a.wrist);
        // Le coude du modèle n'est pas exactement du côté du pôle : quelques degrés de différence au plus.
        assert!(up.angle_between(Quat::IDENTITY) < 0.1, "{up:?}");
        assert!(low.angle_between(Quat::IDENTITY) < 0.1, "{low:?}");
        assert!(a.wrist_for((up, low)).distance(a.wrist) < 1e-3);
    }

    #[test]
    fn le_poignet_atteint_une_cible_a_portee() {
        let a = arm();
        for target in [
            a.wrist + Vec3::new(0.02, 0.04, 0.0),
            a.wrist + Vec3::new(-0.08, -0.05, -0.03),
            a.wrist + Vec3::new(0.0, 0.06, -0.05),
        ] {
            let reached = a.wrist_for(a.solve(target));
            assert!(reached.distance(target) < 1e-3, "{reached} pour {target}");
        }
    }

    #[test]
    fn une_cible_hors_de_portee_est_ramenee_au_bras_tendu_sans_nan() {
        let a = arm();
        let far = a.shoulder + Vec3::new(0.0, 0.0, 5.0);
        let pose = a.solve(far);
        let reached = a.wrist_for(pose);
        assert!(reached.is_finite() && pose.0.is_finite() && pose.1.is_finite());
        assert!(reached.distance(a.shoulder) <= a.upper() + a.lower() + 1e-3);
    }

    #[test]
    fn les_longueurs_des_segments_sont_conservees() {
        let a = arm();
        let pose = a.solve(a.wrist + Vec3::new(0.1, -0.1, 0.02));
        let elbow = a.shoulder + pose.0 * (a.elbow - a.shoulder);
        assert!((elbow.distance(a.shoulder) - a.upper()).abs() < 1e-4);
        assert!((a.wrist_for(pose).distance(elbow) - a.lower()).abs() < 1e-4);
    }

    #[test]
    fn le_coude_va_du_cote_du_pole() {
        let a = arm();
        for lift in [0.1_f32, -0.1] {
            let pose = a.solve(a.wrist + Vec3::new(0.0, lift, 0.0));
            let elbow = a.shoulder + pose.0 * (a.elbow - a.shoulder);
            let chord = (a.wrist + Vec3::new(0.0, lift, 0.0) - a.shoulder).normalize();
            let side = elbow - a.shoulder - chord * (elbow - a.shoulder).dot(chord);
            assert!(side.dot(a.pole) > 0.0, "coude {side} pour le pôle {}", a.pole);
        }
    }

    #[test]
    fn la_prise_tourne_autour_de_l_axe_et_garde_son_rayon() {
        let centre = Vec3::new(0.492, 0.997, 0.695);
        let axis = Vec3::new(0.0, 0.312, -0.951).normalize();
        let grip = Vec3::new(0.637, 1.209, 0.621);
        let turned = turned_grip(grip, centre, axis, 1.0);
        assert!((turned.distance(centre) - grip.distance(centre)).abs() < 1e-5);
        assert!(turned_grip(grip, centre, axis, 0.0).distance(grip) < 1e-6);
        assert!(turned.distance(grip) > 0.1);
    }
}
