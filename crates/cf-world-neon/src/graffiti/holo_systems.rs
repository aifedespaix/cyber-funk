//! Ce qui anime les projecteurs : le plugin du matériau, et `drive_holo`, qui écrit chaque image les uniformes des vingt-quatre
//! pièces du pool (teinte de la palette, apparition, niveau d'instabilité).
//!
//! **Règle 7 de `CLAUDE.md`** : le niveau d'instabilité d'un hologramme est exactement celui que le limiteur de flashs des
//! enseignes ([`SignFlash`], un seul limiteur pour toutes les enseignes et les hologrammes) accorde à l'image : au plus trois
//! départs par seconde, montée et descente d'au moins `MIN_FADE`, plafond de la voie. Si les flashs sont coupés, en mode
//! respiration, ou si le limiteur est absent (panne, enseignes non installées), le niveau est 0 : l'hologramme reste stable
//! et allumé, jamais éteint d'un coup.

use bevy::math::Vec4;
use bevy::prelude::*;
use bevy::shader::Shader;
use cf_render::{CfSet, SceneColor, ScenePalette, WarmupAppExt};

use super::bake::GraffitiAtlas;
use super::holo_material::{HoloMaterial, HoloParams, embed_shader};
use super::projector::{MAX_PROJECTORS, ProjectorPool, ProjectorState, layout};
use super::quality::GraffitiQuality;
use crate::scenery::graffiti_plan::Projector;
use crate::signs::flash::SignFlash;
use crate::signs::systems::SignFrame;

/// Durée de l'apparition d'un projecteur (balayage de bas en haut), en s : au plus 1,5 s.
pub const APPEAR_S: f32 = 1.2;
/// Force du halo flou autour des lettres.
const HALO_GAIN: f32 = 0.9;
/// Force du faisceau entre la barre et les lettres.
const BEAM_GAIN: f32 = 0.5;
/// Les teintes de la palette qu'un projecteur peut prendre, choisies par le rang de son mot. La palette est déjà passée par
/// l'étalonnage du quartier : la teinte suit les deux.
pub const TINT_ROLES: [SceneColor; 4] = [SceneColor::Mint, SceneColor::Rose, SceneColor::Violet, SceneColor::Ice];

/// Installe le matériau des projecteurs et son système.
///
/// À ajouter après les plugins de rendu de Bevy : le shader est embarqué dans `Assets<Shader>` à la construction. Sans
/// rendu (tests), seul le matériau est déclaré et la logique tourne.
#[derive(Debug, Default, Clone, Copy)]
pub struct HoloPlugin;

impl Plugin for HoloPlugin {
    fn build(&self, app: &mut App) {
        if app.is_plugin_added::<bevy::pbr::PbrPlugin>() {
            app.add_plugins(MaterialPlugin::<HoloMaterial>::default())
                .warm_material::<HoloMaterial>("projecteurs holographiques");
            if let Some(mut shaders) = app.world_mut().get_resource_mut::<Assets<Shader>>() {
                embed_shader(&mut shaders);
            }
        } else {
            app.init_asset::<HoloMaterial>();
        }
        app.add_systems(
            Update,
            drive_holo
                .in_set(CfSet::Present)
                .run_if(resource_exists::<ProjectorPool>)
                .run_if(resource_exists::<ScenePalette>),
        );
    }
}

/// Le niveau d'instabilité accordé par le limiteur, de 0 à 1 : celui du pouls des enseignes quand les flashs sont permis, 0
/// sinon (flashs coupés, respiration, limiteur absent).
#[must_use]
pub fn level_from(flash: Option<&SignFlash>) -> f32 {
    flash
        .filter(|f| f.permitted())
        .map_or(0.0, |f| f.pulse().clamp(0.0, 1.0))
}

/// Avance l'apparition d'un projecteur d'une image : de 0 à 1 en [`APPEAR_S`].
#[must_use]
pub fn advance_appear(appear: f32, dt_s: f32) -> f32 {
    (appear + dt_s.max(0.0) / APPEAR_S).min(1.0)
}

/// La teinte (lumière linéaire) du projecteur dont le mot a le rang `word`.
#[must_use]
pub fn tint_for(palette: &ScenePalette, word: u16) -> Vec4 {
    let role = TINT_ROLES[usize::from(word) % TINT_ROLES.len()];
    let c = palette.get(role);
    Vec4::new(c.r, c.g, c.b, 0.0)
}

/// L'état entre deux images : qui est affiché dans chaque emplacement et où en est son apparition.
#[derive(Debug, Default)]
pub(crate) struct HoloDrive {
    shown: [Option<Projector>; MAX_PROJECTORS],
    appear: [f32; MAX_PROJECTORS],
}

/// `CfSet::Present` : écrit les uniformes des matériaux des projecteurs en place (aucune allocation).
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(crate) fn drive_holo(
    time: Res<Time>,
    palette: Res<ScenePalette>,
    frame: Option<Res<SignFrame>>,
    atlas: Option<Res<GraffitiAtlas>>,
    quality: Res<GraffitiQuality>,
    pool: Res<ProjectorPool>,
    state: Res<ProjectorState>,
    mut materials: ResMut<Assets<HoloMaterial>>,
    mut drive: Local<HoloDrive>,
) {
    let level = level_from(frame.as_deref().map(|f| &f.flash));
    let (t, dt) = (time.elapsed_secs_wrapped(), time.delta_secs());
    for (rank, slot) in pool.slots().iter().enumerate() {
        let Some(p) = state.active()[rank] else {
            drive.shown[rank] = None;
            continue;
        };
        if drive.shown[rank] != Some(p) {
            drive.shown[rank] = Some(p);
            drive.appear[rank] = 0.0;
        }
        drive.appear[rank] = advance_appear(drive.appear[rank], dt);
        let Some(atlas) = atlas.as_deref() else { continue };
        let Some(cell) = atlas.holo_cell(p.word) else { continue };
        let lay = layout(&p, &cell);
        let size = atlas.holo_size.as_vec3();
        let rect = |r: super::atlas::CellRect| {
            Vec4::new(
                f32::from(r.x) / size.x,
                f32::from(r.y) / size.y,
                f32::from(r.w) / size.x,
                f32::from(r.h) / size.y,
            )
            .with_w(f32::from(r.layer))
        };
        let tint = tint_for(&palette, p.word);
        let seed = (p.z.abs() * 0.0137).fract() as f32;
        let state_v = Vec4::new(drive.appear[rank], level, t, seed);
        let bar = HoloParams {
            tint: tint.with_w(1.0),
            state: state_v,
            ..HoloParams::default()
        };
        let text = HoloParams {
            tint,
            state: state_v,
            shape: Vec4::new(
                lay.cell_share,
                lay.bar_share,
                if quality.halo { HALO_GAIN } else { 0.0 },
                BEAM_GAIN,
            ),
            sharp: rect(cell.sharp),
            halo: rect(cell.halo),
        };
        for (handle, params) in [(&slot.bar_material, bar), (&slot.text_material, text)] {
            if materials
                .get(handle)
                .is_some_and(|m| m.params == params && m.atlas.is_some())
            {
                continue;
            }
            if let Some(mut material) = materials.get_mut(handle) {
                material.params = params;
                if material.atlas.is_none() {
                    material.atlas = Some(atlas.holo.clone());
                }
            }
        }
    }
}
