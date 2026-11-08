//! Le portique du DJ : un grand portique métallique planté dans la route, au nom du DJ en néon. La voiture passe
//! dessous à l'intro et à chaque changement de DJ.
//!
//! - [`pose`] : la géométrie pure (montée, taille du nom, pièces) ;
//! - ce module : le rendu, qui réutilise l'atlas de glyphes, le matériau des lettres et la machine à lettres de
//!   l'enseigne du DJ (`djsign/`).
//!
//! # Règles
//!
//! - Règle 8 : tout est créé une fois, à `PostStartup`. Absent, le portique n'est pas masqué (une entité jamais dessinée
//!   compilerait ses pipelines au premier passage) : il est enterré sous la route et réduit à une échelle invisible,
//!   comme la voiture éteinte.
//! - Règle 4 : aucun `spawn` en régime établi ; la mise en page du nom ne se refait que quand le nom change.
//! - Règle 7 : le nom s'allume en fondu (mode calme de la machine à lettres) et le glitch est purement géométrique
//!   (décalage et inclinaison des lettres) : aucune variation de lumière à limiter.
//! - Règle 6 : aucun `NoFrustumCulling`, les pièces portent l'`Aabb` de leur maillage.
//! - Les pieds descendent jusqu'à `-BASE_DEPTH_M` : leur base n'est jamais au-dessus de la route.

pub mod pose;

use std::collections::HashMap;

use bevy::math::{DVec3, Vec4};
use bevy::prelude::*;
use cf_render::{AbsolutePosition, CfSet, CurrentShow, SceneColor, ScenePalette, ShowSheet, StreamFocus, WorldLayer};

use crate::djsign::{
    DjSign, DjSignGlyphs, LetterMaterial, LetterParams, MAX_SIGN_CHARS, Measure, PAD_BOTTOM, PAD_TOP, PAD_X, SignInput,
};
use crate::run::RunStatus;
use pose::{AHEAD_M, BEAM_H_M, CLEARANCE_M, NAME_H_M, PYLON_W_M};

/// Échelle du portique absent : invisible, mais toujours dessiné (pipelines chauds).
const HIDDEN_SCALE: f32 = 1e-4;
/// Plafond de pas de temps (s) : après une pause on n'avance pas de plusieurs secondes.
const MAX_DT_S: f32 = 0.1;
/// Avance d'un glyphe inconnu, en hauteurs de capitale.
const UNKNOWN_ADVANCE: f32 = 0.7;

/// Marque la racine du portique (pour la séparer de la voiture dans les requêtes).
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct GantryMarker;

/// Les entités du portique, créées une fois.
#[derive(Resource, Debug)]
pub struct GantryRig {
    root: Entity,
    face: Entity,
    letters: Vec<(Entity, Handle<LetterMaterial>)>,
    metal: Handle<StandardMaterial>,
}

/// L'état du portique : le nom qui s'allume et l'endroit où il est posé.
#[derive(Resource, Debug)]
pub struct GantryState {
    sign: DjSign,
    /// Où le portique est posé (x, z absolus), `None` tant qu'il est absent.
    placed: Option<(f64, f64)>,
}

impl GantryState {
    /// Nombre de lettres du nom allumées.
    #[must_use]
    pub fn lit_letters(&self) -> usize {
        self.sign.poses().filter(|p| p.lit > 0.5).count()
    }

    /// Où le portique est posé, s'il est là.
    #[must_use]
    pub fn placed(&self) -> Option<(f64, f64)> {
        self.placed
    }
}

/// Installe le portique. À ajouter avec le séquenceur (`RunPlugin`) et l'enseigne du DJ (atlas de glyphes).
#[derive(Debug, Default, Clone, Copy)]
pub struct GantryPlugin;

impl Plugin for GantryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostStartup,
            spawn_gantry
                .run_if(resource_exists::<DjSignGlyphs>)
                .run_if(resource_exists::<Assets<Mesh>>)
                .run_if(resource_exists::<Assets<LetterMaterial>>),
        )
        .add_systems(
            Update,
            update_gantry
                .in_set(CfSet::Present)
                .run_if(resource_exists::<GantryRig>)
                .run_if(resource_exists::<DjSignGlyphs>)
                .run_if(resource_exists::<RunStatus>),
        );
    }
}

/// `PostStartup` : crée la racine, les trois pièces, la face et les lettres.
pub(crate) fn spawn_gantry(
    mut commands: Commands,
    glyphs: Res<DjSignGlyphs>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut letter_materials: ResMut<Assets<LetterMaterial>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
) {
    let root = commands
        .spawn((
            Name::new("portique du DJ"),
            GantryMarker,
            WorldLayer,
            AbsolutePosition(DVec3::new(0.0, f64::from(pose::buried_offset_m(0.0)), 0.0)),
            Transform::from_scale(Vec3::splat(HIDDEN_SCALE)),
            Visibility::Inherited,
        ))
        .id();
    let metal = standard.add(StandardMaterial {
        base_color: Color::linear_rgb(0.05, 0.05, 0.07),
        metallic: 0.95,
        perceptual_roughness: 0.36,
        ..StandardMaterial::default()
    });
    let cuboid = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    for (i, piece) in pose::pieces().into_iter().enumerate() {
        commands.spawn((
            Name::new(format!("portique du DJ : pièce {i}")),
            WorldLayer,
            Mesh3d(cuboid.clone()),
            MeshMaterial3d(metal.clone()),
            Transform::from_translation(piece.center).with_scale(piece.size),
            Visibility::Inherited,
            ChildOf(root),
        ));
    }
    let face = commands
        .spawn((
            Name::new("portique du DJ : face"),
            Transform::from_xyz(0.0, CLEARANCE_M + BEAM_H_M / 2.0, PYLON_W_M / 2.0 + 0.06),
            Visibility::Inherited,
            ChildOf(root),
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
                    Name::new(format!("portique du DJ : lettre {i}")),
                    WorldLayer,
                    Mesh3d(plane.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::default(),
                    Visibility::Hidden,
                    ChildOf(face),
                ))
                .id();
            (entity, material)
        })
        .collect();
    let advances: HashMap<char, f32> = glyphs.cells.iter().map(|(c, cell)| (*c, cell.advance)).collect();
    let measure: Measure = Box::new(move |ch| advances.get(&ch).copied().unwrap_or(UNKNOWN_ADVANCE));
    commands.insert_resource(GantryState {
        sign: DjSign::new(measure, true),
        placed: None,
    });
    commands.insert_resource(GantryRig {
        root,
        face,
        letters,
        metal,
    });
}

/// `CfSet::Present` : pose le portique devant la voiture quand l'étape commence, le monte et le descend, écrit son nom.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
fn update_gantry(
    time: Res<Time>,
    status: Res<RunStatus>,
    show: Option<Res<CurrentShow>>,
    sheet: Option<Res<ShowSheet>>,
    palette: Option<Res<ScenePalette>>,
    glyphs: Res<DjSignGlyphs>,
    rig: Res<GantryRig>,
    mut state: ResMut<GantryState>,
    car: Query<&AbsolutePosition, (With<StreamFocus>, Without<GantryMarker>)>,
    mut roots: Query<(&mut AbsolutePosition, &mut Transform), With<GantryMarker>>,
    mut parts: Query<(&mut Transform, &mut Visibility), Without<GantryMarker>>,
    mut letter_materials: ResMut<Assets<LetterMaterial>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(car) = car.single() else { return };
    let out = status.0;
    let stage = out.stages.gantry;
    let rise = pose::rise(stage);

    // Posé devant la voiture au moment où l'étape quitte 0, rendu quand elle revient à 0 (ou à 1, replié).
    if stage > 0.0 && stage < 1.0 {
        if state.placed.is_none() {
            state.placed = Some((car.x, car.z - AHEAD_M));
            // Le nom visé : le DJ qui arrive pendant une passation, le DJ actuel sinon. Nouvelle apparition.
            let slot = out.handover_slot.or_else(|| show.as_deref().map(|s| s.0.slot));
            let name = slot
                .and_then(|i| sheet.as_deref()?.slot(i))
                .map_or("", |s| s.dj.as_str());
            state.sign.set_name(name);
            state.sign.rearm();
        }
    } else if state.placed.is_some() {
        state.placed = None;
    }

    let dt = time.delta_secs().min(MAX_DT_S);
    let now = time.elapsed_secs_f64();
    state.sign.update(
        &SignInput {
            dt,
            kick: false,
            pulse: 0.0,
            bpm: None,
            allowed: out.stages.sign,
            flashes: false,
        },
        now,
    );

    if let Ok((mut position, mut transform)) = roots.get_mut(rig.root) {
        let (x, z) = state.placed.unwrap_or((position.0.x, position.0.z));
        let target = DVec3::new(x, f64::from(pose::buried_offset_m(rise)), z);
        if position.0 != target {
            position.0 = target;
        }
        let scale = if rise <= 0.0 {
            Vec3::splat(HIDDEN_SCALE)
        } else {
            Vec3::ONE
        };
        if transform.scale != scale {
            transform.scale = scale;
        }
    }

    // La face : tout le nom, réduit pour tenir sur la poutre.
    let sign = &state.sign;
    let cap = sign.layout().height.max(f32::EPSILON);
    let fit = NAME_H_M / cap;
    let scale = fit * pose::name_fit(sign.layout().width * fit);
    if let Ok((mut face, _)) = parts.get_mut(rig.face) {
        let s = Vec3::splat(scale);
        if face.scale != s {
            face.scale = s;
        }
    }
    let hue = palette.as_deref().map_or(Vec4::ONE, |p| {
        let c = p.get(SceneColor::Mint);
        Vec4::new(c.r, c.g, c.b, 1.0)
    });
    let plane_h = (1.0 + PAD_TOP + PAD_BOTTOM) * cap;
    let mut poses = sign.poses();
    for (i, (entity, material)) in rig.letters.iter().enumerate() {
        let Ok((mut transform, mut visibility)) = parts.get_mut(*entity) else {
            continue;
        };
        let pose = poses.next();
        let cell = pose.and_then(|p| glyphs.cells.get(&p.ch));
        let (Some(pose), Some(cell)) = (pose, cell) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let (dx, tilt) = pose::glitch_offset(out.stages.name_glitch, i, now);
        let advance = pose.width / cap;
        transform.translation = Vec3::new(pose.x + dx / scale.max(f32::EPSILON), pose.y, 0.0);
        transform.rotation = Quat::from_rotation_z(pose.tilt + tilt);
        transform.scale = Vec3::new((advance + PAD_X * 2.0) * cap, plane_h, 1.0);
        visibility.set_if_neq(Visibility::Inherited);
        if let Some(mut m) = letter_materials.get_mut(material) {
            m.params = LetterParams {
                rect: Vec4::from_array(cell.rect),
                hue: hue.truncate().extend(pose.lit),
            };
        }
    }
    if let Some(mut metal) = standard.get_mut(&rig.metal) {
        let level = sign.lit().min(1.0) * 0.14;
        let glow = LinearRgba::rgb(hue.x * level, hue.y * level, hue.z * level);
        if metal.emissive != glow {
            metal.emissive = glow;
        }
    }
}
