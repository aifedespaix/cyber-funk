//! Les formules de l'atmosphère, en Rust : le miroir exact des fonctions de `atmosphere.wesl`, pour les tester sans GPU
//! (transmittance contre intégration numérique, limite `u → 0`, bornes de la phase et de la coquille de kick).
//!
//! Toute modification ici se reporte dans le shader, et inversement : un test compare les constantes des deux textes.

use bevy::math::Vec3;

/// En dessous de ce `|u|`, `(1 − e^{−u})/u` est remplacé par son développement `1 − u/2` : la soustraction `1 − e^{−u}`
/// perd ses chiffres en `f32` quand `u` tend vers 0 (rayon horizontal).
pub const U_SERIES: f32 = 1.0e-3;
/// Borne de `|u|` : en dessous de `−U_MAX`, `e^{−u}` déborde un `f32` (rayon très plongeant vu de très haut) ; au-dessus
/// de `U_MAX`, il est nul.
pub const U_MAX: f32 = 30.0;
/// La caméra peut passer sous le sol (nappe sous la route) : la densité n'y croît pas sans fin.
pub const DEPTH_BELOW_MAX: f32 = 2.0;
/// Rayon minimal d'un tube (m) : la forme close diverge quand le rayon de vue traverse la source.
pub const TUBE_RADIUS_M: f32 = 1.5;
/// Part de la brume qui reste au creux de la coquille de kick, toutes ondes confondues.
pub const SHELL_FLOOR: f32 = 0.5;

/// `(1 − e^{−u})/u`, stable autour de 0.
#[must_use]
pub fn slab(u: f32) -> f32 {
    // Très plongeant : `e^{−u}` déborderait. Très montant : `e^{−u}` est nul, il reste `1/u`.
    let u = u.max(-U_MAX);
    if u > U_MAX {
        1.0 / u
    } else if u.abs() < U_SERIES {
        1.0 - 0.5 * u
    } else {
        (1.0 - (-u).exp()) / u
    }
}

/// Épaisseur optique `τ` d'un rayon de longueur `dist` parti de `origin` dans la direction unitaire `dir`, dans une
/// brume de densité `σ_t·ρ0·exp(−(y − sol)/H_f)` (P2 §1.3) : `τ = σρ·exp(−o_y/H)·D·(1 − e^{−u})/u`, `u = D·d_y/H`.
#[must_use]
pub fn optical_depth(sigma_rho: f32, height_m: f32, ground_y: f32, origin: Vec3, dir: Vec3, dist: f32) -> f32 {
    let h = height_m.max(1.0);
    let above = ((origin.y - ground_y) / h).max(-DEPTH_BELOW_MAX);
    sigma_rho * (-above).exp() * dist * slab(dist * dir.y / h)
}

/// Transmittance `T = e^{−τ}`.
#[must_use]
pub fn transmittance(tau: f32) -> f32 {
    (-tau.max(0.0)).exp()
}

/// Phase de Henyey-Greenstein **ramenée à un pic de 1** : `P_HG(cos θ, g)·4π·(1 − g)²/(1 + g)`.
///
/// La forme brute a un pic de `(1 + g)/(1 − g)²/(4π)`, soit ×11 entre `g = 0,55` et `g = 0,85` : moduler `g` par le son
/// serait un flash. Normalisée, monter `g` resserre le lobe sans relever son sommet (règle 7).
#[must_use]
pub fn phase_hg(cos_theta: f32, g: f32) -> f32 {
    let g = g.clamp(0.0, 0.95);
    let base = (1.0 + g * g - 2.0 * g * cos_theta.clamp(-1.0, 1.0)).max(1.0e-4);
    let one_minus = 1.0 - g;
    one_minus * one_minus * one_minus * one_minus / (base * base.sqrt())
}

/// Lumière diffusée par un tube (source ponctuelle, milieu homogène, sans phase) le long du rayon :
/// `(atan((D + b)/h) − atan(b/h))/h`, `q = o − P`, `b = d·q`, `h = √(q·q − b²)` (P2 §1.3). À multiplier par `σ_s·ρ·I`.
#[must_use]
pub fn tube(origin: Vec3, dir: Vec3, dist: f32, light: Vec3) -> f32 {
    let q = origin - light;
    let b = dir.dot(q);
    let h = (q.dot(q) - b * b).max(TUBE_RADIUS_M * TUBE_RADIUS_M).sqrt();
    (((dist + b) / h).atan() - (b / h).atan()) / h
}

/// Réglage de la coquille de vide qu'un kick pousse devant lui.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shell {
    /// Vitesse du front, m/s.
    pub speed: f32,
    /// Épaisseur du front, m.
    pub sigma: f32,
    /// Durée de vie, s.
    pub tau: f32,
    /// Profondeur du creux, 0..1 (0 : pas de coquille).
    pub depth: f32,
}

/// Dispersion `D(x, t)` à la distance `r` de la source : `1 − depth·Σ exp(−((r − c·âge)/σ)²)·exp(−âge/τ)`, bornée à
/// [`SHELL_FLOOR`] (quatre kicks rapprochés ne vident pas la brume).
#[must_use]
pub fn kick_dispersion(r: f32, ages: [f32; 4], shell: Shell) -> f32 {
    let mut sum = 0.0;
    for age in ages {
        let x = (r - shell.speed * age) / shell.sigma.max(0.1);
        sum += (-x * x).exp() * (-age / shell.tau.max(0.01)).exp();
    }
    (1.0 - shell.depth * sum).max(SHELL_FLOOR)
}

/// Distance utile de la brume, pour le culling (P2 §1.3) : `d = ln(50)/(σ_t·ρ0)`, là où `T < 2 %` à l'horizontale au sol.
#[must_use]
pub fn fog_reach(sigma_rho: f32) -> f32 {
    50.0_f32.ln() / sigma_rho.max(1.0e-6)
}

/// Perspective atmosphérique d'une couleur (DA §1.2 règle 3) : saturation `×(0,4 + 0,6·T)`, puis `C·T + C_brume·(1 − T)`.
#[must_use]
pub fn aerial(color: Vec3, fog: Vec3, t: f32) -> Vec3 {
    let luma = color.dot(Vec3::new(0.2126, 0.7152, 0.0722));
    let muted = Vec3::splat(luma).lerp(color, 0.4 + 0.6 * t);
    muted * t + fog * (1.0 - t)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const SIGMA_RHO: f32 = 0.0037;
    const HEIGHT: f32 = 25.0;

    /// Intégration numérique de la densité le long du rayon (point milieu, `f64`).
    fn numeric_depth(origin: Vec3, dir: Vec3, dist: f32) -> f64 {
        let n = 20_000;
        let ds = f64::from(dist) / f64::from(n);
        let mut sum = 0.0;
        for k in 0..n {
            let s = (f64::from(k) + 0.5) * ds;
            let y = f64::from(origin.y) + s * f64::from(dir.y);
            sum += f64::from(SIGMA_RHO) * (-y / f64::from(HEIGHT)).exp() * ds;
        }
        sum
    }

    proptest! {
        #[test]
        fn la_transmittance_analytique_suit_l_integration_numerique(
            cam_y in 0.5_f32..140.0,
            pitch in -0.6_f32..1.2,
            dist in 1.0_f32..1500.0,
        ) {
            let dir = Vec3::new(0.0, pitch.sin(), -pitch.cos());
            // Le rayon ne descend pas sous le sol : il s'arrêterait sur la route.
            let dist = if dir.y < 0.0 { dist.min(cam_y / -dir.y) } else { dist };
            let origin = Vec3::new(3.0, cam_y, 0.0);
            let t = transmittance(optical_depth(SIGMA_RHO, HEIGHT, 0.0, origin, dir, dist));
            let reference = (-numeric_depth(origin, dir, dist)).exp() as f32;
            prop_assert!((t - reference).abs() < 0.01 * reference.max(0.01), "T = {t}, référence = {reference}");
        }

        #[test]
        fn la_coquille_de_kick_reste_dans_ses_bornes(r in 0.0_f32..400.0, a in 0.0_f32..3.0, b in 0.0_f32..3.0) {
            let shell = Shell { speed: 40.0, sigma: 5.0, tau: 0.6, depth: 0.5 };
            let d = kick_dispersion(r, [a, b, a + b, 60.0], shell);
            prop_assert!((SHELL_FLOOR..=1.0).contains(&d));
        }

        #[test]
        fn la_phase_normalisee_ne_depasse_jamais_un(cos_theta in -1.0_f32..1.0, g in 0.0_f32..0.95) {
            let p = phase_hg(cos_theta, g);
            prop_assert!(p > 0.0 && p <= 1.0 + 1e-4, "phase = {p}");
        }
    }

    #[test]
    fn la_limite_u_nul_est_stable_et_continue() {
        assert!((slab(0.0) - 1.0).abs() < 1e-7);
        // De part et d'autre du seuil de bascule, les deux branches se rejoignent.
        for sign in [-1.0_f32, 1.0] {
            let below = slab(sign * U_SERIES * 0.999);
            let above = slab(sign * U_SERIES * 1.001);
            assert!((below - above).abs() < 2e-4, "saut au seuil : {below} contre {above}");
        }
        // Rayon parfaitement horizontal : la loi de Beer-Lambert d'un milieu homogène.
        let tau = optical_depth(SIGMA_RHO, HEIGHT, 0.0, Vec3::ZERO, Vec3::NEG_Z, 600.0);
        assert!((tau - SIGMA_RHO * 600.0).abs() < 1e-5);
        // Valeurs extrêmes : jamais de NaN ni d'infini.
        for u in [-1.0e6_f32, -30.0, 30.0, 1.0e6] {
            assert!(slab(u).is_finite());
        }
    }

    #[test]
    fn la_brume_tient_les_plans_de_la_direction_artistique() {
        // DA §1.2 : 1 − T ≤ 0,2 à 60 m (scène), 0,2 à 0,7 entre 60 et 600 m, 0,7 à 0,95 au-delà de 600 m (horizon).
        let fogged = |d: f32| {
            1.0 - transmittance(optical_depth(
                SIGMA_RHO,
                HEIGHT,
                0.0,
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::NEG_Z,
                d,
            ))
        };
        assert!(fogged(60.0) <= 0.2 + 1e-3, "{}", fogged(60.0));
        assert!((0.2..=0.7).contains(&fogged(200.0)));
        assert!((0.7..=0.95).contains(&fogged(650.0)), "{}", fogged(650.0));
        assert!((fog_reach(SIGMA_RHO) - 1057.3).abs() < 1.0);
    }

    #[test]
    fn le_tube_en_forme_close_suit_l_integration_numerique() {
        let origin = Vec3::new(0.0, 1.5, 0.0);
        let dir = Vec3::new(0.2, 0.05, -1.0).normalize();
        let light = Vec3::new(4.0, 6.0, -30.0);
        let dist = 120.0_f32;
        let n = 200_000;
        let ds = f64::from(dist) / f64::from(n);
        let mut sum = 0.0_f64;
        for k in 0..n {
            let s = ((f64::from(k) + 0.5) * ds) as f32;
            let p = origin + dir * s;
            sum += ds / f64::from(p.distance_squared(light));
        }
        let closed = f64::from(tube(origin, dir, dist, light));
        assert!(
            (closed - sum).abs() < 0.01 * sum,
            "forme close {closed}, numérique {sum}"
        );
    }

    #[test]
    fn la_perspective_atmospherique_desature_et_eclaircit_au_loin() {
        let tower = Vec3::new(0.02, 0.0, 0.05);
        let fog = Vec3::new(0.05, 0.04, 0.09);
        let near = aerial(tower, fog, 0.95);
        let far = aerial(tower, fog, 0.1);
        let luma = |c: Vec3| c.dot(Vec3::new(0.2126, 0.7152, 0.0722));
        // Règle 2 : la brume lointaine est plus claire que la tour qu'elle recouvre.
        assert!(luma(far) > luma(near));
        // Sans brume, la couleur est intacte.
        assert!(aerial(tower, fog, 1.0).distance(tower) < 1e-6);
    }
}
