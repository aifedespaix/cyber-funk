//! La couche Bevy de l'enseigne du DJ : les entités préallouées à l'ouverture, et le système qui recopie la pose du chef.
//!
//! # Pool (règle 4)
//!
//! Tout est créé une fois, dans `PostStartup`, caché au repos, et **jamais détruit ni recréé** : une racine (position
//! absolue, origine flottante), vingt lettres (plans portant chacun son matériau), l'ossature (un maillage remplacé quand
//! le nom change) et les deux pavés de l'immeuble support (même maillage, matériau et étiquette que les gratte-ciel,
//! comme les arches). Aucun `spawn` en régime établi.
//!
//! # Culling
//!
//! Les lettres ont l'`Aabb` du plan unité ; l'ossature, celle de ses poutres (insérée à chaque remplacement du maillage,
//! Bevy ne la recalcule pas). Aucun `NoFrustumCulling`. Les lettres n'ont pas de plage de visibilité : le néon se devine
//! depuis l'horizon, c'est tout son intérêt ; l'immeuble, lui, disparaît dans la brume comme les façades.

use bevy::camera::primitives::Aabb;
use bevy::math::{DVec3, Vec4};
use bevy::mesh::MeshTag;
use bevy::pbr::MeshMaterial3d;
use bevy::prelude::*;
use cf_render::{AbsolutePosition, ScenePalette, WorldLayer};

use super::director::{DjSignDirector, SiteBlock};
use super::frame;
use super::glyphs::GlyphCell;
use super::layout::MAX_SIGN_CHARS;
use super::material::{LetterMaterial, LetterParams};
use super::mesh::frame_mesh;
use super::sign::{PAD_BOTTOM, PAD_TOP, PAD_X};
use crate::scenery::layout::Tier;
use crate::scenery::{SceneryAssets, fog_range};

/// Rang de la teinte violette dans le cycle des immeubles (`prismaticCycle` : cyan, azur, violet, cyan, rose, glace) :
/// l'immeuble support est violet, comme `palette.violet` dans Sillage.
const VIOLET_TINT: u32 = 2;
/// Part de la teinte du néon que le métal de l'ossature reflète, par niveau de lumière (`0.14` de Sillage).
const FRAME_GLOW: f32 = 0.14;

/// Toutes les entités de l'enseigne, créées à l'ouverture.
#[derive(Resource, Debug)]
pub struct DjSignRig {
    root: Entity,
    letters: Vec<(Entity, Handle<LetterMaterial>)>,
    frame: Entity,
    frame_mesh: Handle<Mesh>,
    frame_material: Handle<StandardMaterial>,
    site: [Entity; 2],
    /// Dernières mise en page et pose montrées : on ne retouche ni le maillage ni les pavés tant qu'elles n'ont pas changé.
    shown_layout: Option<u32>,
    shown_site: Option<u32>,
}

impl DjSignRig {
    /// Nombre d'entités préallouées (constant en régime établi).
    #[must_use]
    pub fn entity_count(&self) -> usize {
        1 + self.letters.len() + 1 + self.site.len()
    }

    /// La racine : position absolue de l'enseigne.
    #[must_use]
    pub fn root(&self) -> Entity {
        self.root
    }

    /// Les entités des lettres.
    pub fn letters(&self) -> impl Iterator<Item = Entity> + '_ {
        self.letters.iter().map(|(entity, _)| *entity)
    }

    /// L'entité de l'ossature.
    #[must_use]
    pub fn frame(&self) -> Entity {
        self.frame
    }

    /// Les deux pavés de l'immeuble support.
    #[must_use]
    pub fn site(&self) -> [Entity; 2] {
        self.site
    }
}

/// L'atlas des glyphes sur le GPU et l'index de ses cellules.
#[derive(Resource, Debug, Clone)]
pub struct DjSignGlyphs {
    /// La texture de l'atlas.
    pub image: Handle<Image>,
    /// Les cellules.
    pub cells: std::collections::HashMap<char, GlyphCell>,
}

/// `PostStartup` : crée la racine, les lettres, l'ossature et les pavés, tout caché.
pub(crate) fn spawn_rig(
    mut commands: Commands,
    glyphs: Res<DjSignGlyphs>,
    scenery: Res<SceneryAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut letter_materials: ResMut<Assets<LetterMaterial>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
) {
    let root = commands
        .spawn((
            Name::new("enseigne du DJ"),
            AbsolutePosition::default(),
            Transform::default(),
            Visibility::Hidden,
        ))
        .id();
    let plane = meshes.add(Rectangle::new(1.0, 1.0));
    let letters = (0..MAX_SIGN_CHARS)
        .map(|i| {
            let material = letter_materials.add(LetterMaterial {
                params: LetterParams::default(),
                atlas: Some(glyphs.image.clone()),
            });
            let entity = commands
                .spawn((
                    Name::new(format!("enseigne du DJ : lettre {i}")),
                    WorldLayer,
                    Mesh3d(plane.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::default(),
                    Visibility::Hidden,
                    ChildOf(root),
                ))
                .id();
            (entity, material)
        })
        .collect();
    let frame_mesh = meshes.add(placeholder_mesh());
    let frame_material = standard.add(StandardMaterial {
        metallic: 0.95,
        perceptual_roughness: 0.36,
        ..StandardMaterial::default()
    });
    let frame = commands
        .spawn((
            Name::new("enseigne du DJ : ossature"),
            WorldLayer,
            Mesh3d(frame_mesh.clone()),
            MeshMaterial3d(frame_material.clone()),
            Aabb::default(),
            Transform::default(),
            Visibility::Hidden,
            ChildOf(root),
        ))
        .id();
    let site = std::array::from_fn(|i| {
        commands
            .spawn((
                Name::new(format!("enseigne du DJ : immeuble {i}")),
                WorldLayer,
                Mesh3d(scenery.mesh.clone()),
                MeshMaterial3d(scenery.material.clone()),
                MeshTag::new(0),
                AbsolutePosition::default(),
                Transform::default(),
                Visibility::Hidden,
                fog_range(),
            ))
            .id()
    });
    commands.insert_resource(DjSignRig {
        root,
        letters,
        frame,
        frame_mesh,
        frame_material,
        site,
        shown_layout: None,
        shown_site: None,
    });
}

/// L'étiquette de façade d'un pavé de l'immeuble support : graine, teinte violette, aucun projecteur (tier du corps).
fn site_tag(seed: u32, block: &SiteBlock) -> u32 {
    ((seed + block.seed_offset) & 0x3FF) | VIOLET_TINT << 10 | (Tier::Body as u32) << 14
}

/// Ce que le système touche : une seule requête, parce que chaque entité du pool a sa propre combinaison de composants.
type PoolQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Transform,
        &'static mut Visibility,
        Option<&'static mut AbsolutePosition>,
        Option<&'static mut MeshTag>,
    ),
>;

/// `CfSet::Present` : recopie la pose du chef sur les entités.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(crate) fn apply_sign(
    mut commands: Commands,
    director: Res<super::DjSignRes>,
    glyphs: Res<DjSignGlyphs>,
    palette: Option<Res<ScenePalette>>,
    mut rig: ResMut<DjSignRig>,
    mut pool: PoolQuery,
    mut meshes: ResMut<Assets<Mesh>>,
    mut letter_materials: ResMut<Assets<LetterMaterial>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
) {
    let director: &DjSignDirector = &director.0;
    let sign = director.sign();

    // Rien de posé : tout dort.
    let Some(site) = director.site().filter(|_| director.visible()) else {
        hide_all(&rig, &mut pool);
        rig.shown_site = None;
        return;
    };

    // L'immeuble support : à chaque pose.
    if rig.shown_site != Some(director.site_seq()) {
        rig.shown_site = Some(director.site_seq());
        for (entity, block) in rig.site.into_iter().zip([site.body, site.parapet]) {
            if let Ok((mut transform, mut visibility, position, tag)) = pool.get_mut(entity) {
                if let Some(mut position) = position {
                    position.0 = block.base;
                }
                transform.scale = block.size;
                if let Some(mut tag) = tag {
                    tag.value = site_tag(site.seed, &block);
                }
                visibility.set_if_neq(Visibility::Inherited);
            }
        }
        if let Ok((_, _, Some(mut position), _)) = pool.get_mut(rig.root) {
            position.0 = DVec3::new(f64::from(site.x), f64::from(site.roof_y), site.sign_z);
        }
    }
    if let Ok((_, mut visibility, _, _)) = pool.get_mut(rig.root) {
        visibility.set_if_neq(Visibility::Inherited);
    }

    // L'ossature : à chaque reconstruction des lettres.
    if rig.shown_layout != Some(sign.layout_seq()) {
        rig.shown_layout = Some(sign.layout_seq());
        match frame_mesh(&frame::build(sign.layout())) {
            Some((mesh, aabb)) => {
                meshes.insert(rig.frame_mesh.id(), mesh).ok();
                commands.entity(rig.frame).insert(aabb);
            }
            None => {
                meshes.insert(rig.frame_mesh.id(), placeholder_mesh()).ok();
            }
        }
    }
    if let Ok((_, mut visibility, _, _)) = pool.get_mut(rig.frame) {
        visibility.set_if_neq(Visibility::Inherited);
    }

    // Les lettres.
    let hue = palette.as_deref().map_or(Vec4::ONE, |p| {
        let c = p.get(director.hue());
        Vec4::new(c.r, c.g, c.b, 1.0)
    });
    let cap = sign.layout().height;
    let plane_h = (1.0 + PAD_TOP + PAD_BOTTOM) * cap;
    let mut poses = sign.poses();
    for (entity, material) in &rig.letters {
        let Ok((mut transform, mut visibility, _, _)) = pool.get_mut(*entity) else {
            continue;
        };
        let pose = poses.next();
        let cell = pose.and_then(|p| glyphs.cells.get(&p.ch));
        let (Some(pose), Some(cell)) = (pose, cell) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let advance = pose.width / cap;
        transform.translation = Vec3::new(pose.x, pose.y, 0.0);
        transform.rotation = Quat::from_rotation_z(pose.tilt);
        transform.scale = Vec3::new((advance + PAD_X * 2.0) * cap, plane_h, 1.0);
        visibility.set_if_neq(Visibility::Inherited);
        if let Some(mut material) = letter_materials.get_mut(material) {
            material.params = LetterParams {
                rect: Vec4::from_array(cell.rect),
                hue: hue.truncate().extend(pose.lit),
            };
        }
    }

    // Le reflet du néon sur le métal.
    if let Some(mut material) = standard.get_mut(&rig.frame_material) {
        let level = sign.lit().min(1.0) * FRAME_GLOW;
        let glow = LinearRgba::rgb(hue.x * level, hue.y * level, hue.z * level);
        if material.emissive != glow {
            material.emissive = glow;
        }
        if let Some(p) = palette.as_deref() {
            let (ink, ice) = (p.get(cf_render::SceneColor::Ink), p.get(cf_render::SceneColor::Ice));
            let metal = Color::linear_rgb(
                ink.r + (ice.r - ink.r) * 0.22,
                ink.g + (ice.g - ink.g) * 0.22,
                ink.b + (ice.b - ink.b) * 0.22,
            );
            if material.base_color != metal {
                material.base_color = metal;
            }
        }
    }
}

fn hide_all(rig: &DjSignRig, pool: &mut PoolQuery) {
    let all = std::iter::once(rig.root)
        .chain(rig.letters())
        .chain(std::iter::once(rig.frame))
        .chain(rig.site);
    for entity in all {
        if let Ok((_, mut visibility, _, _)) = pool.get_mut(entity) {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
}

/// Maillage de remplacement quand l'ossature n'a aucune pièce : un triangle dégénéré (trois sommets confondus), qui ne
/// dessine rien. Un maillage sans aucun attribut n'a pas de position : le prépasse et les ombres de `StandardMaterial`
/// exigent `Vertex_Position` et la validation du pipeline fait quitter l'application.
fn placeholder_mesh() -> Mesh {
    Mesh::new(
        bevy::mesh::PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0_f32, 0.0, 1.0]; 3])
}
