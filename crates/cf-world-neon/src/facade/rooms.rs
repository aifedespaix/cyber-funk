//! Fenêtres habitées (chantier C4 ; DA §5, idée de rupture n° 1) : le miroir Rust de `facade_rooms.wesl`, pour tester
//! sans GPU la géométrie de la fausse profondeur et les bornes de la lumière d'une pièce.
//!
//! La cellule d'une fenêtre (une travée sur un étage) est la face avant d'une boîte unité. Le rayon de vue, en espace
//! tangent (x le long de la façade, y vers le haut, z vers l'intérieur), entre en `p = (f.x, f.y, 0)` et sort à
//! `t = min_axe((step(0, d) − p)/d)` : la paroi touchée donne la lumière. Deux plans intermédiaires portent des
//! silhouettes (mobilier, habitant), et une pièce sur cinq a un store.
//!
//! Rien ne suit la musique : la pièce module la lumière de la vitre, qui reste plafonnée par `window_cap`. Toute
//! modification ici se reporte dans le shader : un test compare les constantes des deux textes.

use bevy::math::{Vec2, Vec3};

/// Profondeur d'une pièce, m : de la plus courte à la plus longue.
pub const DEPTH_M: (f32, f32) = (2.2, 4.5);
/// Lumière du mur du fond (la vitre en aplat vaut 1).
pub const BACK: f32 = 0.9;
/// Lumière du plafond, hors rampe.
pub const CEILING: f32 = 1.0;
/// Ce que la rampe du plafond ajoute dans son axe.
pub const CEILING_LAMP: f32 = 0.7;
/// Demi-largeur de la rampe, en part de la travée.
pub const LAMP_HALF: f32 = 0.09;
/// Lumière du sol.
pub const FLOOR: f32 = 0.35;
/// Lumière moyenne des murs de côté (l'un est à 0,8, l'autre à 1,2 de cette valeur).
pub const SIDE: f32 = 0.55;
/// Ce qu'il reste de lumière au fond de la pièce.
pub const DEPTH_FALL: f32 = 0.5;
/// Ce que la porte du mur du fond garde de la lumière du mur.
pub const DOOR: f32 = 0.5;
/// Demi-largeur du montant central de la vitre, en part de la travée.
pub const MULLION_HALF: f32 = 0.018;
/// Ce qu'une silhouette (mobilier, habitant, lame de store) laisse passer.
pub const SHADOW: f32 = 0.22;
/// Plan du mobilier, en part de la profondeur.
pub const FURNITURE_Z: f32 = 0.6;
/// Plan de l'habitant, en part de la profondeur.
pub const PERSON_Z: f32 = 0.45;
/// Part des pièces habitées.
pub const PERSON_SHARE: f32 = 0.25;
/// Part des pièces à store.
pub const BLIND_SHARE: f32 = 0.2;
/// Période de l'aller-retour d'un habitant, s. Elle divise la durée de rebouclage du temps des shaders (2 400 s).
pub const PACE_S: f32 = 48.0;
/// La pièce se fond en aplat entre ces deux distances, m (DA §5 : aplat au-delà de 150 m).
pub const FADE_M: (f32, f32) = (90.0, 150.0);

/// La paroi que touche le rayon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wall {
    /// Mur du fond (`z = 1`).
    Back,
    /// Plafond (`y = 1`).
    Ceiling,
    /// Sol (`y = 0`).
    Floor,
    /// Mur de côté (`x = 0` ou `x = 1`).
    Side,
}

/// Où le rayon `d` (unités de boîte), entré en `p` sur la face avant, touche la boîte unité : le paramètre `t`, le point
/// et la paroi. `d.z` est ramené à un minimum positif (on regarde toujours vers l'intérieur).
#[must_use]
pub fn room_hit(p: Vec2, d: Vec3) -> (f32, Vec3, Wall) {
    let safe = |v: f32| if v.abs() < 1.0e-5 { 1.0e-5 } else { v };
    let d = Vec3::new(safe(d.x), safe(d.y), d.z.max(1.0e-4));
    let p3 = p.extend(0.0);
    let exit = |p: f32, d: f32| (if d >= 0.0 { 1.0 } else { 0.0 } - p) / d;
    let t3 = Vec3::new(exit(p3.x, d.x), exit(p3.y, d.y), exit(p3.z, d.z));
    let t = t3.min_element();
    let wall = if t3.y <= t3.x && t3.y <= t3.z {
        if d.y > 0.0 { Wall::Ceiling } else { Wall::Floor }
    } else if t3.x < t3.z {
        Wall::Side
    } else {
        Wall::Back
    };
    (t, p3 + d * t, wall)
}

/// Lumière de la paroi touchée en `q`, avant les silhouettes. `door_x` : axe de la porte du mur du fond (0,2 ou 0,8).
#[must_use]
pub fn wall_light(q: Vec3, wall: Wall, door_x: f32) -> f32 {
    let base = match wall {
        Wall::Back => {
            let door = (q.x - door_x).abs() <= 0.13 && q.y <= 0.78;
            BACK * (0.75 + 0.25 * q.y) * if door { DOOR } else { 1.0 }
        }
        Wall::Ceiling => {
            let t = (((q.x - 0.5).abs() - LAMP_HALF) / LAMP_HALF).clamp(0.0, 1.0);
            CEILING + CEILING_LAMP * (1.0 - t * t * (3.0 - 2.0 * t))
        }
        Wall::Floor => FLOOR,
        Wall::Side => SIDE * if q.x >= 0.5 { 1.2 } else { 0.8 },
    };
    base * (1.0 + (DEPTH_FALL - 1.0) * q.z)
}

/// Borne haute de la lumière d'une pièce : le plafonnier, au ras de la vitre.
pub const LIGHT_MAX: f32 = CEILING + CEILING_LAMP;
/// Borne basse d'une paroi nue : le sol au fond de la pièce.
pub const LIGHT_MIN: f32 = FLOOR * DEPTH_FALL;

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::facade::material::FACADE_SOURCE;

    proptest! {
        /// Le point touché est sur une paroi de la boîte unité, jamais dehors, et la paroi annoncée est la bonne.
        #[test]
        fn le_rayon_touche_une_paroi_de_la_boite(
            px in 0.0_f32..1.0, py in 0.0_f32..1.0,
            dx in -3.0_f32..3.0, dy in -3.0_f32..3.0, dz in 0.01_f32..3.0,
        ) {
            let (t, q, wall) = room_hit(Vec2::new(px, py), Vec3::new(dx, dy, dz));
            prop_assert!(t >= 0.0 && t.is_finite());
            let eps = 1.0e-3;
            prop_assert!(q.min_element() >= -eps && q.max_element() <= 1.0 + eps, "{q:?}");
            let on = |v: f32, target: f32| (v - target).abs() < eps;
            match wall {
                Wall::Back => prop_assert!(on(q.z, 1.0)),
                Wall::Ceiling => prop_assert!(on(q.y, 1.0)),
                Wall::Floor => prop_assert!(on(q.y, 0.0)),
                Wall::Side => prop_assert!(on(q.x, 0.0) || on(q.x, 1.0)),
            }
            // La lumière de la paroi reste dans ses bornes : la pièce ne crée ni flash ni trou noir.
            for door_x in [0.2, 0.8] {
                let light = wall_light(q, wall, door_x);
                prop_assert!((LIGHT_MIN - 1e-4..=LIGHT_MAX + 1e-4).contains(&light), "{light}");
            }
        }
    }

    #[test]
    fn de_face_on_voit_le_mur_du_fond_et_de_biais_un_mur_de_cote() {
        let (_, q, wall) = room_hit(Vec2::new(0.5, 0.5), Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(wall, Wall::Back);
        assert!((q - Vec3::new(0.5, 0.5, 1.0)).length() < 1e-3);
        // Regard rasant vers la droite : le mur de droite, près de la vitre.
        let (_, q, wall) = room_hit(Vec2::new(0.5, 0.5), Vec3::new(4.0, 0.0, 1.0));
        assert_eq!(wall, Wall::Side);
        assert!((q.x - 1.0).abs() < 1e-3 && q.z < 0.2);
        // Depuis la rue, on regarde vers le haut : c'est le plafond qu'on voit, pas le sol.
        let (_, _, wall) = room_hit(Vec2::new(0.5, 0.8), Vec3::new(0.0, 2.0, 1.0));
        assert_eq!(wall, Wall::Ceiling);
        let (_, _, wall) = room_hit(Vec2::new(0.5, 0.2), Vec3::new(0.0, -2.0, 1.0));
        assert_eq!(wall, Wall::Floor);
    }

    #[test]
    fn la_piece_a_du_contraste_plafond_clair_sol_sombre() {
        // Sous la rampe, le plafond est la paroi la plus claire ; le sol, la plus sombre ; la porte assombrit le fond.
        let ceiling = wall_light(Vec3::new(0.5, 1.0, 0.1), Wall::Ceiling, 0.2);
        let floor = wall_light(Vec3::new(0.5, 0.0, 0.1), Wall::Floor, 0.2);
        let back = wall_light(Vec3::new(0.5, 0.5, 1.0), Wall::Back, 0.2);
        let door = wall_light(Vec3::new(0.2, 0.5, 1.0), Wall::Back, 0.2);
        assert!(ceiling > 3.0 * floor && back > door && door > 0.0);
        assert!((ceiling - LIGHT_MAX * (1.0 + (DEPTH_FALL - 1.0) * 0.1)).abs() < 1e-5);
        // Hors de la rampe, le plafond retombe à sa lumière de base.
        assert!((wall_light(Vec3::new(0.9, 1.0, 0.0), Wall::Ceiling, 0.2) - CEILING).abs() < 1e-6);
    }

    #[test]
    fn la_parallaxe_suit_le_regard() {
        // Le même point de la vitre, vu de plus en plus de biais : le point touché sur le mur du fond glisse dans le sens
        // du regard, puis passe sur le mur de côté. C'est ce glissement qui donne la profondeur.
        let hit = |dx: f32| room_hit(Vec2::new(0.3, 0.5), Vec3::new(dx, 0.0, 1.0)).1;
        assert!(hit(0.2).x > hit(0.0).x && hit(0.5).x > hit(0.2).x);
        assert!(hit(-0.2).x < hit(0.0).x);
    }

    #[test]
    fn l_habitant_reboucle_sans_saut() {
        // Le temps des shaders reboucle à 2 400 s : la période de l'aller-retour le divise.
        assert!((2400.0 / PACE_S).fract().abs() < 1e-6);
    }

    /// Lit `const NAME: f32 = valeur;` dans le shader.
    fn wesl_const(name: &str) -> f32 {
        let key = format!("const {name}: f32 = ");
        let at = FACADE_SOURCE.find(&key).map_or(0, |i| i + key.len());
        let rest = &FACADE_SOURCE[at..];
        rest[..rest.find(';').unwrap_or(0)].trim().parse().unwrap_or(f32::NAN)
    }

    #[test]
    fn les_constantes_du_shader_sont_celles_du_miroir_rust() {
        for (name, rust) in [
            ("ROOM_DEPTH_MIN_M", DEPTH_M.0),
            ("ROOM_DEPTH_MAX_M", DEPTH_M.1),
            ("ROOM_BACK", BACK),
            ("ROOM_CEILING", CEILING),
            ("ROOM_FLOOR", FLOOR),
            ("ROOM_SIDE", SIDE),
            ("ROOM_SHADOW", SHADOW),
            ("ROOM_LAMP", CEILING_LAMP),
            ("ROOM_LAMP_HALF", LAMP_HALF),
            ("ROOM_DEPTH_FALL", DEPTH_FALL),
            ("ROOM_DOOR", DOOR),
            ("ROOM_MULLION_HALF", MULLION_HALF),
            ("ROOM_FURNITURE_Z", FURNITURE_Z),
            ("ROOM_PERSON_Z", PERSON_Z),
            ("ROOM_PERSON_SHARE", PERSON_SHARE),
            ("ROOM_BLIND_SHARE", BLIND_SHARE),
            ("ROOM_PACE_S", PACE_S),
            ("ROOM_FADE_NEAR_M", FADE_M.0),
            ("ROOM_FADE_FAR_M", FADE_M.1),
        ] {
            assert!(
                (wesl_const(name) - rust).abs() < 1e-6,
                "{name} : shader {}, Rust {rust}",
                wesl_const(name)
            );
        }
        // La pièce module la vitre **avant** le plafond des fenêtres : elle ne peut pas le faire sauter.
        let pane = FACADE_SOURCE
            .find("let pane = window_color * lit * mask * room")
            .unwrap_or(usize::MAX);
        let cap = FACADE_SOURCE.find("window_cap(pane * window_gain(pane_n)").unwrap_or(0);
        assert!(
            pane < cap,
            "la pièce doit entrer dans `pane`, que `window_cap` plafonne"
        );
    }
}
