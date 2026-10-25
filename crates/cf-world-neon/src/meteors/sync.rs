//! De la logique au rendu : recopie la pose de chaque pièce du pool dans son entité, tourne les plans vers la caméra,
//! réécrit les matériaux et la gerbe de débris. Rien ici ne décide : [`Meteors`] a déjà tout calculé, et le limiteur de
//! flashs a déjà dit ce qui brille (règle 7).
//!
//! `CfSet::Present`, après le réalisateur : la caméra est celle de l'image. Les matériaux ne sont réécrits que si leurs
//! uniformes changent (Bevy ne re-prépare pas un asset qu'on n'a pas touché).

use bevy::camera::primitives::Aabb;
use bevy::math::{Mat3, Vec3, Vec4};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use cf_render::palette::{LinearRgb, SceneColor};
use cf_render::{AbsolutePosition, CameraPose, ScenePalette};

use super::PlanetTextures;
use super::blast::{ATTACK_S as BLAST_ATTACK_S, DEBRIS};
use super::flash::{GRANTED_GAIN, UNGRANTED_GAIN};
use super::material::{FxKind, FxParams, MeteorFxMaterial, MeteorSurfaceMaterial, SurfaceParams};
use super::rig::{MeteorMaterials, MeteorRig, Piece};
use super::systems::Meteors;
use super::types::{HEAT_COUNT, Meteor};

/// Marge de l'`Aabb` des débris (m) : un carré de 2,3 m de côté, diagonale comprise, déborde de son centre d'un peu moins.
const DEBRIS_MARGIN_M: f32 = 2.0;

/// Les teintes de chaleur (ambre, corail, glace, menthe, azur) de la palette de l'image.
fn heat(palette: &ScenePalette) -> [[f32; 3]; HEAT_COUNT] {
    let rgb = |c: SceneColor| {
        let LinearRgb { r, g, b } = palette.get(c);
        [r, g, b]
    };
    [
        rgb(SceneColor::Amber),
        rgb(SceneColor::Rose),
        rgb(SceneColor::Ice),
        rgb(SceneColor::Mint),
        rgb(SceneColor::Azure),
    ]
}

/// Écrit un matériau seulement s'il a changé.
fn write<M: Asset + PartialEq>(assets: &mut Assets<M>, handle: &Handle<M>, value: M) {
    if assets.get(handle).is_some_and(|m| *m == value) {
        return;
    }
    if let Some(mut material) = assets.get_mut(handle) {
        *material = value;
    }
}

/// La rotation d'un plan (face +Z) qui regarde la caméra : `from_eye` va de la caméra au plan.
#[must_use]
pub fn billboard(from_eye: Vec3) -> Quat {
    Transform::IDENTITY.looking_to(from_eye, Vec3::Y).rotation
}

/// Pose de la queue : un ruban face à la caméra, sa tête exactement sur l'astre. Le repère doit être orthonormé : l'axe
/// vers la caméra est donc débarrassé de sa part le long de la trajectoire (sinon le ruban se vrille et la lueur dérive).
/// Renvoie la rotation, l'échelle `(longueur, largeur, 1)` et le centre relatif à l'astre.
#[must_use]
pub fn trail_pose(heading: Vec3, to_camera: Vec3, radius: f32) -> (Quat, Vec3, Vec3) {
    let length = 18.0 + radius * 16.0;
    let axis = -heading;
    let mut across = to_camera - axis * to_camera.dot(axis);
    // Caméra dans l'axe du vol : Sillage prenait `+z`, qui peut être l'axe lui-même (base dégénérée, ruban NaN). Tout
    // vecteur perpendiculaire à l'axe convient.
    if across.length_squared() < 1e-6 {
        across = axis.any_orthonormal_vector();
    }
    let across = across.normalize();
    let rotation = Quat::from_mat3(&Mat3::from_cols(axis, across.cross(axis), across));
    (
        rotation,
        Vec3::new(length, radius * 2.6, 1.0),
        heading * (-length * 0.5),
    )
}

/// Opacité d'un astre qui vient de naître : sa lueur monte en [`BLAST_ATTACK_S`] (jamais un départ sec).
fn rise(age_s: f32) -> f32 {
    (age_s / BLAST_ATTACK_S).clamp(0.0, 1.0)
}

/// Direction de l'œil vers `at` (absolu, `f64` : la différence est précise, le résultat tient en `f32`).
fn from_eye(eye: bevy::math::DVec3, at: bevy::math::DVec3) -> Vec3 {
    (at - eye).as_vec3()
}

type PieceItem<'a> = (
    &'a Piece,
    &'a mut AbsolutePosition,
    &'a mut Transform,
    &'a mut Visibility,
    Option<&'a mut Aabb>,
);

/// `CfSet::Present` : pose, matériaux et débris de chaque pièce.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(super) fn sync_meteors(
    meteors: Res<Meteors>,
    pose: Res<CameraPose>,
    palette: Res<ScenePalette>,
    rig: Res<MeteorRig>,
    textures: Option<Res<PlanetTextures>>,
    mut materials: MeteorMaterials,
    mut meshes: ResMut<Assets<Mesh>>,
    mut pieces: Query<PieceItem<'_>>,
) {
    let heat = heat(&palette);
    let eye = pose.position;
    let flash_level = meteors.flash().level();
    for (piece, mut absolute, mut transform, mut visibility, aabb) in &mut pieces {
        let shown = match *piece {
            Piece::Body(i) => body(
                &meteors.field().meteors()[i],
                &rig,
                i,
                textures.as_deref(),
                &heat,
                &mut absolute,
                &mut transform,
                &mut materials,
            ),
            Piece::Trail(i) | Piece::Glow(i) => {
                let m = &meteors.field().meteors()[i];
                m.active
                    && trail_or_glow(
                        *piece,
                        m,
                        &rig,
                        &heat,
                        eye,
                        &mut absolute,
                        &mut transform,
                        &mut materials,
                    )
            }
            Piece::Splash(i) => {
                let s = &meteors.field().splashes()[i];
                if s.is_free() {
                    false
                } else {
                    absolute.set_if_neq(AbsolutePosition(s.position));
                    transform.rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
                    transform.scale = Vec3::splat((6.0 + s.radius * 14.0) * (0.25 + s.t()));
                    let gain = if s.flash_granted { GRANTED_GAIN } else { UNGRANTED_GAIN };
                    let level = rise(s.age_s) * (1.0 - s.t()) * (1.0 - s.t()) * gain;
                    let params = FxParams::new(FxKind::Splash, heat[s.tint], level, 0.0);
                    write(&mut materials.fx, &rig.splash[i], MeteorFxMaterial { params });
                    true
                }
            }
            Piece::Flash(i) => {
                let b = &meteors.blasts().slots()[i];
                // L'éclair n'existe que si le limiteur l'a accordé, au niveau qu'il rend.
                if b.shock_done() || !b.flash_granted || b.flash_t() >= 1.0 || flash_level <= 0.0 {
                    false
                } else {
                    absolute.set_if_neq(AbsolutePosition(b.at));
                    transform.rotation = billboard(from_eye(eye, b.at));
                    transform.scale = Vec3::splat(b.flash_scale());
                    let params = FxParams::new(FxKind::Flash, heat[b.tint], flash_level, 0.0);
                    write(&mut materials.fx, &rig.flash[i], MeteorFxMaterial { params });
                    true
                }
            }
            Piece::Shock(i) => {
                let b = &meteors.blasts().slots()[i];
                if b.shock_done() {
                    false
                } else {
                    absolute.set_if_neq(AbsolutePosition(b.at));
                    transform.rotation = billboard(from_eye(eye, b.at));
                    transform.scale = Vec3::splat(b.size_m() * 3.4);
                    let gain = if b.flash_granted { GRANTED_GAIN } else { UNGRANTED_GAIN };
                    let level = b.shock_fade() * b.attack() * gain;
                    let params = FxParams::new(FxKind::Shock, heat[b.tint], level, b.shock_radius());
                    write(&mut materials.fx, &rig.shock[i], MeteorFxMaterial { params });
                    true
                }
            }
            Piece::Debris(i) => {
                let b = &meteors.blasts().slots()[i];
                if b.debris_alive() {
                    absolute.set_if_neq(AbsolutePosition(b.at));
                    transform.rotation = Quat::IDENTITY;
                    transform.scale = Vec3::ONE;
                    let (min, max) = b.debris_bounds(DEBRIS_MARGIN_M);
                    if let Some(mut aabb) = aabb {
                        *aabb = Aabb::from_min_max(min, max);
                    }
                    if let Some(mut mesh) = meshes.get_mut(&rig.debris_mesh[i]) {
                        write_debris(&mut mesh, b, heat[b.tint]);
                    }
                    true
                } else {
                    false
                }
            }
        };
        visibility.set_if_neq(if shown { Visibility::Visible } else { Visibility::Hidden });
    }
}

/// Le corps de l'astre : pose, rotation sur lui-même, matériau (texture, teinte, direction du vol).
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
fn body(
    m: &Meteor,
    rig: &MeteorRig,
    i: usize,
    textures: Option<&PlanetTextures>,
    heat: &[[f32; 3]; HEAT_COUNT],
    absolute: &mut Mut<'_, AbsolutePosition>,
    transform: &mut Mut<'_, Transform>,
    materials: &mut MeteorMaterials,
) -> bool {
    if !m.active {
        return false;
    }
    absolute.set_if_neq(AbsolutePosition(m.position));
    transform.scale = Vec3::splat(m.radius);
    transform.rotation = Quat::from_euler(EulerRot::XYZ, m.angle_rad.1, m.angle_rad.0, 0.0);
    let tint = heat[m.tint];
    let dir = m.heading();
    let map = textures.and_then(|t| t.0.get(m.texture)).cloned();
    write(
        &mut materials.surface,
        &rig.body[i],
        MeteorSurfaceMaterial {
            params: SurfaceParams {
                tint: Vec4::new(tint[0], tint[1], tint[2], 1.0),
                dir: dir.extend(0.0),
                seed: Vec4::new(m.seed, 0.0, 0.0, 0.0),
            },
            map,
        },
    );
    true
}

/// La queue ou le halo d'un astre en vol : un ruban ou un plan face à la caméra, tête sur l'astre.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
fn trail_or_glow(
    piece: Piece,
    m: &Meteor,
    rig: &MeteorRig,
    heat: &[[f32; 3]; HEAT_COUNT],
    eye: bevy::math::DVec3,
    absolute: &mut Mut<'_, AbsolutePosition>,
    transform: &mut Mut<'_, Transform>,
    materials: &mut MeteorMaterials,
) -> bool {
    let to_camera = (-from_eye(eye, m.position)).normalize_or(Vec3::Z);
    let level = rise(m.age_s);
    match piece {
        Piece::Trail(i) => {
            let (rotation, scale, offset) = trail_pose(m.heading(), to_camera, m.radius);
            absolute.set_if_neq(AbsolutePosition(m.position + offset.as_dvec3()));
            transform.rotation = rotation;
            transform.scale = scale;
            let params = FxParams::new(FxKind::Trail, heat[m.tint], level, 0.0);
            write(&mut materials.fx, &rig.trail[i], MeteorFxMaterial { params });
        }
        Piece::Glow(i) => {
            absolute.set_if_neq(AbsolutePosition(m.position));
            transform.rotation = billboard(-to_camera);
            transform.scale = Vec3::splat(m.radius * 6.5);
            let params = FxParams::new(FxKind::Glow, heat[m.tint], level, 0.0);
            write(&mut materials.fx, &rig.glow[i], MeteorFxMaterial { params });
        }
        _ => return false,
    }
    true
}

/// Réécrit positions et couleurs des carrés d'une gerbe, en place (aucune allocation : les attributs existent déjà).
fn write_debris(mesh: &mut Mesh, blast: &super::blast::Blast, tint: [f32; 3]) {
    if let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
        for (i, quad) in positions.as_chunks_mut::<4>().0.iter_mut().enumerate().take(DEBRIS) {
            quad.fill(blast.offset(i).to_array());
        }
    }
    if let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR) {
        for (i, quad) in colors.as_chunks_mut::<4>().0.iter_mut().enumerate().take(DEBRIS) {
            let level = blast.debris_level(i);
            quad.fill([tint[0] * level, tint[1] * level, tint[2] * level, 1.0]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_plan_billboard_regarde_la_camera() {
        for from in [
            Vec3::new(0.0, 0.0, -50.0),
            Vec3::new(30.0, 10.0, -80.0),
            Vec3::new(0.0, -40.0, 0.01),
        ] {
            let facing = billboard(from) * Vec3::Z;
            let to_eye = (-from).normalize();
            assert!(facing.dot(to_eye) > 0.999, "{from:?} : {facing:?}");
        }
    }

    #[test]
    fn la_queue_part_de_la_tete_et_reste_orthonormee() {
        let heading = Vec3::new(0.2, -0.7, -0.68).normalize();
        let to_camera = Vec3::new(-0.3, 0.5, 0.8).normalize();
        let (rotation, scale, offset) = trail_pose(heading, to_camera, 3.0);
        // Le bout +x du ruban est derrière l'astre, à une demi-longueur de son centre.
        let tail = rotation * Vec3::X;
        assert!(tail.dot(-heading) > 0.9999);
        assert!(((rotation * Vec3::Y).dot(rotation * Vec3::Z)).abs() < 1e-5);
        assert!((rotation.length() - 1.0).abs() < 1e-5);
        assert!(
            (offset.length() * 2.0 - scale.x).abs() < 1e-4,
            "centre à une demi-longueur de la tête"
        );
        assert!(offset.dot(heading) < 0.0, "le centre est derrière l'astre");
    }

    #[test]
    fn la_queue_ne_se_vrille_pas_quand_la_camera_est_dans_l_axe() {
        let heading = Vec3::NEG_Z;
        let (rotation, ..) = trail_pose(heading, Vec3::Z, 1.0);
        assert!(rotation.is_finite() && (rotation.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn la_lueur_monte_en_un_dixieme_de_seconde() {
        assert_eq!(rise(0.0), 0.0);
        assert!((rise(0.05) - 0.5).abs() < 1e-6);
        assert_eq!(rise(1.0), 1.0);
    }
}
