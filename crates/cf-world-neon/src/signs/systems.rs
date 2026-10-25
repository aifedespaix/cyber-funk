//! Les systèmes des enseignes : la route décide en `CfSet::Simulate`, les entités et le matériau suivent en `CfSet::Present`.
//!
//! Aucune allocation par image en régime établi : les tampons (obstructions, sujets) sont préalloués, les entités du
//! pool ne sont jamais créées ni détruites, les uniformes sont réécrits en place et seulement s'ils changent.

use bevy::math::{DVec3, Vec3, Vec4};
use bevy::prelude::*;
use cf_analysis::AudioFeatures as Features;
use cf_render::palette::LinearRgb;
use cf_render::{AbsolutePosition, AudioFeatures, CurrentShow, CurrentShowFx, SceneColor, ScenePalette, StreamFocus};

use super::ads::AdKind;
use super::atlas::{FLAG_VERDICT, Secondary};
use super::audio::{SignAudio, SignListener};
use super::bake::SignAtlas;
use super::flash::SignFlash;
use super::formats::{Look, Mount};
use super::lookup::{cell_key, strip_key};
use super::material::{LOOK_CAMERA, LOOK_GHOST, LOOK_HOLO, LOOK_METAL, LOOK_NEON, SignMaterial, SignParams};
use super::rig::{Resolved, SignRig, SignUnitIndex};
use super::road::{CamShot, POOL, RoadFrame, SignRoad, Unit};
use super::show_signs::SignShow;
use super::structure::{self, PartKind, SLAB_FRONT};
use crate::pilot::flight::Obstruction;
use crate::pilot::{CarAnimation, PilotBrain, to_kmh};
use crate::rng::hash;

/// La route d'enseignes : écrite par `update_signs` seul, lue par le rendu.
#[derive(Resource, Debug, Default)]
pub struct SignRoadRes(pub SignRoad);

/// Ce qui change d'une image à l'autre en dehors de la route : l'audio, les limiteurs.
#[derive(Resource, Debug, Default)]
pub struct SignFrame {
    /// L'audio de l'image.
    pub audio: SignAudio,
    /// Les limiteurs de flashs.
    pub flash: SignFlash,
    listener: SignListener,
}

/// Les structures à esquiver, mises à jour chaque image (tampon préalloué). Le pilote les lira avec celles des arches.
#[derive(Resource, Debug)]
pub struct SignObstructions(pub Vec<Obstruction>);

impl Default for SignObstructions {
    fn default() -> Self {
        Self(Vec::with_capacity(POOL))
    }
}

fn linear(palette: &ScenePalette, color: SceneColor) -> Vec3 {
    let LinearRgb { r, g, b } = palette.get(color);
    Vec3::new(r, g, b)
}

/// `CfSet::Simulate`, après les tronçons et le pilote : la route d'enseignes pour cette image.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub fn update_signs(
    time: Res<Time>,
    features: Option<Res<AudioFeatures>>,
    show: Option<Res<CurrentShow>>,
    fx: Option<Res<CurrentShowFx>>,
    brain: Option<Res<PilotBrain>>,
    animation: Option<Res<CarAnimation>>,
    atlas: Res<SignAtlas>,
    car: Query<&AbsolutePosition, With<StreamFocus>>,
    mut road: ResMut<SignRoadRes>,
    mut frame: ResMut<SignFrame>,
    mut sign_show: ResMut<SignShow>,
    mut obstructions: ResMut<SignObstructions>,
    mut resolved: ResMut<SignResolved>,
) {
    let Ok(car) = car.single() else { return };
    let (now, dt) = (time.elapsed_secs_f64(), time.delta_secs());
    sign_show.active = show.is_some();
    sign_show.slot = show.as_ref().map_or(0, |s| s.slot);
    let calm = fx.as_ref().is_some_and(|f| f.0.breathing);
    let speed = brain.as_deref().map_or(0.0, |b| b.0.frame().speed);
    let features = features.as_deref().map_or(Features::NEUTRAL, |f| f.0);
    let drop = show.as_ref().is_some_and(|s| s.drop_now);
    let SignFrame { audio, flash, listener } = &mut *frame;
    *audio = listener.listen(now, dt, &features, drop, calm, to_kmh(speed) as f32);
    let altitude = animation.as_deref().map_or(0.0, |a| a.altitude);
    road.0.update(
        &RoadFrame {
            car_z: car.0.z,
            altitude: f64::from(altitude),
            time: now,
            show: &sign_show,
        },
        crate::route::is_near_bridge,
    );
    let mut event = false;
    for (index, unit) in road.0.units.iter_mut().enumerate() {
        if unit.state.is_none() {
            continue;
        }
        if unit.fresh {
            let view = resolve(&atlas, unit, &sign_show);
            if let Some(strip) = view.strip {
                unit.ticker.set(strip.period);
            }
            resolved.0[index] = view;
        }
        unit.ticker.step(audio);
        if let Some(state) = &unit.state {
            event |= state.radar;
            event |= drop && unit.visible && state.ad.kind() == AdKind::Escape;
        }
    }
    let policy = fx.as_ref().map(|f| (f.0.flashes, f.0.breathing));
    flash.update(dt, now, policy, features.pulse, event);
    obstructions.0.clear();
    road.0.obstructions(car.0.z, &mut obstructions.0);
}

/// Les résolutions des unités (cellule, bandeau), écrites quand une unité reçoit un panneau.
#[derive(Resource, Debug)]
pub struct SignResolved(pub [Resolved; POOL]);

impl Default for SignResolved {
    fn default() -> Self {
        Self([Resolved::default(); POOL])
    }
}

/// Cherche dans l'atlas la cellule et le bandeau d'un panneau qui vient d'être posé.
fn resolve(atlas: &SignAtlas, unit: &Unit, show: &SignShow) -> Resolved {
    let (Some(state), Some(format)) = (&unit.state, unit.format) else {
        return Resolved::default();
    };
    let look = format.format().look;
    let cell = cell_key(look, &state.ad, state.slogan, show).and_then(|k| atlas.cells.get(&k).copied());
    let strip = strip_key(look, &state.ad, state.slogan).and_then(|k| atlas.strips.get(&k).copied());
    if cell.is_none() {
        // Une enseigne sans image reste cachée : on prévient, une fois par pose, sans couper le show.
        tracing::warn!(?format, ad = ?state.ad, "enseigne : cellule absente de l'atlas, panneau caché");
    }
    Resolved {
        cell,
        strip,
        look: Some(look),
    }
}

/// Le rendu d'une unité pour cette image : tout ce que le shader lit.
#[derive(Debug, Clone, Copy)]
struct Draw<'a> {
    unit: &'a Unit,
    view: &'a Resolved,
    audio: &'a SignAudio,
    pulse: f32,
    burst: f32,
    shown: f32,
    radar: bool,
    time_s: f32,
    page_size: Vec4,
    strip_page_h: f32,
}

fn look_code(look: Look) -> f32 {
    match look {
        Look::Holo => LOOK_HOLO,
        Look::Metal => LOOK_METAL,
        Look::Ghost => LOOK_GHOST,
        Look::Neon => LOOK_NEON,
        Look::Camera => LOOK_CAMERA,
    }
}

/// Les uniformes d'une enseigne (hors couleurs, que la palette fournit).
fn params(d: &Draw<'_>, palette: &ScenePalette) -> SignParams {
    let (Some(state), Some(cell), Some(look)) = (&d.unit.state, d.view.cell, d.view.look) else {
        return SignParams::default();
    };
    let accent = linear(palette, state.ad.accent());
    let second = match cell.secondary {
        Secondary::Role(role) => linear(palette, role),
        Secondary::Accent => accent,
        Secondary::None => Vec3::ZERO,
    };
    let verdict = cell.flags & FLAG_VERDICT != 0;
    let strip = d.view.strip;
    // Le flash de ce panneau : le niveau du limiteur d'événements, pour les panneaux concernés seulement.
    let flash = if verdict || (look == Look::Camera && d.radar) {
        d.burst
    } else {
        0.0
    };
    let pos = d.unit.ticker.position(d.audio, f64::from(state.seed));
    SignParams {
        cell: Vec4::new(f32::from(cell.x), f32::from(cell.y), d.page_size.x, d.page_size.y),
        strip: strip.map_or(Vec4::ZERO, |s| {
            Vec4::new(f32::from(s.y), super::atlas::PAGE_W as f32, s.period, pos)
        }),
        layout: Vec4::new(
            d.strip_page_h,
            f32::from(u8::from(strip.is_some())),
            f32::from(u8::from(matches!(look, Look::Ghost))),
            look_code(look),
        ),
        accent: accent.extend(d.unit.opacity * d.shown),
        ice: linear(palette, SceneColor::Ice).extend(1.0),
        second: second.extend(cell.flags as f32),
        ink: linear(palette, SceneColor::Ink).extend(1.0),
        velvet: linear(palette, SceneColor::Velvet).extend(state.seed),
        state: Vec4::new(d.pulse, flash, d.pulse, d.time_s),
        feed: Vec4::new(
            d.audio.kmh,
            10.0 + (hash(f64::from(state.seed), 9.0) * 90.0).floor() as f32,
            match state.shot {
                CamShot::Wide => 0.0,
                CamShot::Front => 1.0,
                CamShot::Side => 2.0,
            },
            f32::from(u8::from(state.live)),
        ),
        misc: Vec4::new(
            f32::from(u8::from(d.unit.format.is_some_and(|f| f.format().mount == Mount::None))),
            match (look, verdict) {
                (_, true) => 2.2,
                (Look::Camera, _) => 1.5,
                _ => 0.0,
            },
            f32::from(u8::from(d.audio.calm)),
            0.0,
        ),
    }
}

/// Réécrit `value` dans l'actif seulement s'il diffère : Bevy ne re-prépare pas un matériau qui n'a pas changé.
fn set_color(materials: &mut Assets<StandardMaterial>, handle: &Handle<StandardMaterial>, rgb: Vec3) {
    let color = Color::linear_rgb(rgb.x, rgb.y, rgb.z);
    if materials.get(handle).is_some_and(|m| m.base_color != color)
        && let Some(mut m) = materials.get_mut(handle)
    {
        m.base_color = color;
    }
}

/// `CfSet::Present` : pose les entités et le matériau de chaque unité d'après la route.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub fn render_signs(
    mut road: ResMut<SignRoadRes>,
    frame: Res<SignFrame>,
    resolved: Res<SignResolved>,
    mut rig: ResMut<SignRig>,
    atlas: Res<SignAtlas>,
    palette: Res<ScenePalette>,
    time: Res<Time>,
    mut signs: ResMut<Assets<SignMaterial>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut roots: Query<(&mut AbsolutePosition, &mut Transform, &mut Visibility), With<SignUnitIndex>>,
    mut parts: Query<(&mut Transform, &mut Visibility), Without<SignUnitIndex>>,
) {
    let (now, dt) = (time.elapsed_secs_f64(), time.delta_secs());
    let permitted = frame.flash.permitted();
    let iron = linear(&palette, SceneColor::Velvet);
    set_color(&mut standard, &rig.iron.clone(), iron * 0.35);
    for (index, unit) in road.0.units.iter_mut().enumerate() {
        let unit_rig = &mut rig.units[index];
        let Ok((mut position, mut transform, mut visibility)) = roots.get_mut(unit_rig.root) else {
            continue;
        };
        let (Some(state), Some(format)) = (unit.state, unit.format) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        if unit.fresh {
            unit_rig.gate.reset();
            unit_rig.resolved = resolved.0[index];
            unit_rig.radar_hold_s = 0.0;
            fit_structure(unit_rig, unit, &mut parts);
            unit.fresh = false;
        }
        if state.radar {
            unit_rig.radar_hold_s = 0.7;
        }
        unit_rig.radar_hold_s = (unit_rig.radar_hold_s - dt).max(0.0);
        let shown = unit_rig
            .gate
            .step(dt, now, unit.display.on, unit.display.passing, permitted);
        position.0 = unit.position();
        transform.rotation = Quat::from_rotation_y(unit.layout.yaw);
        let visible = unit.visible && unit_rig.resolved.cell.is_some();
        visibility.set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if !visible {
            continue;
        }
        // Le plan : décalage et étirement du bug, posé devant le caisson (ou au centre d'une projection).
        let floating = format.format().mount == Mount::None;
        let panel_z = if floating { 0.0 } else { SLAB_FRONT };
        if let Ok((mut t, mut v)) = parts.get_mut(unit_rig.panel) {
            t.translation = Vec3::new(unit.display.dx, 0.0, panel_z);
            t.scale = Vec3::new(unit.width * unit.display.sx, unit.height * unit.display.sy, 1.0);
            v.set_if_neq(if shown > 0.01 {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
        let display_on = shown > 0.5;
        if let Some(fit) = fit_of(unit) {
            for kind in PartKind::ALL {
                if let Ok((_, mut v)) = parts.get_mut(unit_rig.parts[kind.index()]) {
                    let on = fit.shown(kind, display_on);
                    v.set_if_neq(if on { Visibility::Inherited } else { Visibility::Hidden });
                }
            }
        }
        // Liseré à la couleur de la pub, balise rouge au tempo : le pouls est celui du limiteur.
        let fade = unit.opacity;
        let accent = linear(&palette, state.ad.accent());
        let glow = 1.0 + frame.flash.burst() * if look_of(format) == Look::Camera { 1.5 } else { 0.0 };
        let level = (0.9 + frame.flash.pulse() * 0.8) * fade * glow * 1.4 * shown;
        set_color(&mut standard, &unit_rig.light.clone(), accent * level);
        let beacon = (0.3 + frame.flash.pulse() * 0.7) * fade;
        set_color(
            &mut standard,
            &unit_rig.beacon.clone(),
            Vec3::new(beacon, beacon * 0.08, beacon * 0.05),
        );
        // Le matériau du plan.
        let Some(cell) = unit_rig.resolved.cell else { continue };
        let view = unit_rig.resolved;
        let draw = Draw {
            unit,
            view: &view,
            audio: &frame.audio,
            pulse: frame.flash.pulse(),
            burst: frame.flash.burst(),
            shown,
            radar: unit_rig.radar_hold_s > 0.0,
            time_s: (now % 3600.0) as f32,
            page_size: atlas.page_size(usize::from(cell.page)),
            strip_page_h: view.strip.map_or(1.0, |s| atlas.page_size(usize::from(s.page)).y),
        };
        let new = params(&draw, &palette);
        let art = atlas.page_handle(usize::from(cell.page));
        let strip = view.strip.and_then(|s| atlas.page_handle(usize::from(s.page)));
        if let Some(mut material) = signs.get_mut(&unit_rig.material)
            && (material.params != new || material.art != art || (strip.is_some() && material.strip != strip))
        {
            material.params = new;
            material.art = art;
            if strip.is_some() {
                material.strip = strip;
            }
        }
    }
}

fn look_of(format: super::formats::FormatId) -> Look {
    format.format().look
}

/// La structure du format actuel de l'unité (recalculée à la demande : une poignée de produits, sans allocation).
fn fit_of(unit: &Unit) -> Option<structure::Fit> {
    let format = unit.format?;
    let f = format.format();
    Some(structure::fit(
        f.mount,
        unit.width,
        unit.height,
        unit.side,
        unit.layout.y,
        f.mount != Mount::None,
        f.look != Look::Neon,
        unit.round,
    ))
}

/// Pose les pièces de structure d'une unité qui vient de recevoir un panneau.
fn fit_structure(
    rig: &super::rig::UnitRig,
    unit: &Unit,
    parts: &mut Query<(&mut Transform, &mut Visibility), Without<SignUnitIndex>>,
) {
    let Some(fit) = fit_of(unit) else { return };
    for kind in PartKind::ALL {
        let Ok((mut transform, _)) = parts.get_mut(rig.parts[kind.index()]) else {
            continue;
        };
        if let Some(part) = fit.parts[kind.index()] {
            transform.translation = part.pos;
            transform.scale = part.scale;
        }
    }
}

/// Les sujets filmables (panneaux lisibles) : pour que `NeonWorld::subjects` les dépose dans le `SubjectSink`.
///
/// `collect` reçoit (identifiant, centre de la face, direction de la face lisible, largeur).
pub fn collect_subjects(road: &SignRoad, collect: impl FnMut(u64, DVec3, Vec3, f32)) {
    road.collect_signs(collect);
}
