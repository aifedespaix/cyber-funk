//! Les arches (`Bridges` de `bridges.ts`) : un tablier fenêtré qui enjambe la route, posé sur deux piliers.
//!
//! Le plan (où sont les arches) est celui de [`crate::route::bridges`] ; ici seulement l'affichage : six entités
//! préallouées (deux arches au plus, trois pavés chacune), posées d'après [`RoutePlan`] quand il change, c'est-à-dire d'une
//! arche à l'autre. Mêmes mesh, matériau et tag que les immeubles : elles se regroupent avec eux dans le même lot.

use bevy::mesh::MeshTag;
use bevy::prelude::*;
use cf_render::{AbsolutePosition, WorldLayer};

use super::layout::BASE_DEPTH_M;
use super::{fog_range, mesh_tag};
use crate::route::bridges::{DECK_BOTTOM, DECK_HEIGHT, DEPTH, PILLAR_WIDTH, SLOTS, SPAN_HALF};
use crate::route::{Bridge, RoutePlan};
use crate::scenery::layout::Tier;

/// Pavés d'une arche : pilier gauche, pilier droit, tablier.
const PARTS: usize = 3;

/// Les entités préallouées des arches, créées dans `World::setup`.
#[derive(Resource, Debug, Clone)]
pub struct ArchPool {
    entities: [Entity; SLOTS * PARTS],
}

impl ArchPool {
    /// Crée les six pavés, cachés. Démarrage seulement.
    pub(crate) fn spawn(
        ecs: &mut World,
        mesh: &Handle<Mesh>,
        material: &Handle<crate::facade::FacadeMaterial>,
    ) -> Self {
        let entities = std::array::from_fn(|i| {
            ecs.spawn((
                Name::new(format!("arche {}.{}", i / PARTS, i % PARTS)),
                WorldLayer,
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                mesh_tag(0),
                AbsolutePosition::default(),
                Transform::default(),
                Visibility::Hidden,
                fog_range(),
            ))
            .id()
        });
        Self { entities }
    }

    /// Les entités, par arche puis par pavé.
    #[must_use]
    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }
}

/// Un pavé d'arche : position absolue du pied, taille, graine.
struct Block {
    x: f64,
    y: f64,
    size: Vec3,
    seed: u32,
}

/// Les trois pavés d'une arche (`Bridges.update` de Sillage) : piliers du vide jusqu'au dessous du tablier, tablier d'un
/// pilier à l'autre.
fn blocks(bridge: &Bridge) -> [Block; PARTS] {
    let base = f64::from(BASE_DEPTH_M);
    let deck_top = DECK_BOTTOM + DECK_HEIGHT;
    let pillar_x = SPAN_HALF - PILLAR_WIDTH / 2.0;
    let pillar = |x: f64, seed: u32| Block {
        x,
        y: -base,
        size: Vec3::new(PILLAR_WIDTH as f32, (base + DECK_BOTTOM) as f32, DEPTH as f32),
        seed,
    };
    [
        pillar(-pillar_x, bridge.seed),
        pillar(pillar_x, bridge.seed + 7),
        Block {
            x: 0.0,
            y: DECK_BOTTOM,
            size: Vec3::new((SPAN_HALF * 2.0) as f32, (deck_top - DECK_BOTTOM) as f32, DEPTH as f32),
            seed: bridge.seed + 13,
        },
    ]
}

/// `CfSet::Simulate`, après les tronçons : pose les pavés quand le plan de route change.
pub(crate) fn update_arches(
    plan: Res<RoutePlan>,
    world: Res<crate::NeonWorld>,
    pool: Res<ArchPool>,
    mut blocks_q: Query<(&mut AbsolutePosition, &mut Transform, &mut MeshTag, &mut Visibility)>,
) {
    if !plan.is_changed() && !world.is_changed() {
        return;
    }
    for (slot, bridge) in plan.bridges.slots().iter().enumerate() {
        let placed = bridge.as_ref().filter(|_| world.has_city()).map(|b| (b, blocks(b)));
        for part in 0..PARTS {
            let Some(&entity) = pool.entities.get(slot * PARTS + part) else {
                continue;
            };
            let Ok((mut position, mut transform, mut tag, mut visibility)) = blocks_q.get_mut(entity) else {
                continue;
            };
            match &placed {
                Some((bridge, blocks)) => {
                    let block = &blocks[part];
                    position.0 = bevy::math::DVec3::new(block.x, block.y, bridge.z);
                    transform.scale = block.size;
                    // Teinte du pont, graine du pavé, aucun projecteur : tier 0, comme `info.set([seed, -1, 0, offset])`.
                    tag.value = (block.seed & 0x3FF) | (bridge.tint as u32 & 0xF) << 10 | (Tier::Body as u32) << 14;
                    visibility.set_if_neq(Visibility::Inherited);
                }
                None => {
                    visibility.set_if_neq(Visibility::Hidden);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arch_tag_has_temper_zero() {
        let mut world = World::new();
        let pool = ArchPool::spawn(&mut world, &Handle::default(), &Handle::default());
        for &entity in pool.entities() {
            let tag = world.get::<MeshTag>(entity).map(|t| t.value);
            assert_eq!(tag.map(|t| (t >> 24) & 0x7), Some(0));
        }
    }

    #[test]
    fn les_piliers_et_le_tablier_sont_ceux_de_sillage() {
        let bridge = Bridge::of_cell(2);
        let [left, right, deck] = blocks(&bridge);
        assert!(
            (left.x + 15.25).abs() < 1e-9 && (right.x - 15.25).abs() < 1e-9,
            "pilier à 19,5 − 8,5/2"
        );
        // Du vide (−384 m) jusqu'au dessous du tablier (24 m).
        assert!((left.y + 384.0).abs() < 1e-9 && (left.size.y - 408.0).abs() < 1e-3);
        assert!((deck.y - 24.0).abs() < 1e-9 && (deck.size.y - 11.0).abs() < 1e-3 && (deck.size.x - 39.0).abs() < 1e-3);
        assert_eq!(
            [left.seed, right.seed, deck.seed],
            [bridge.seed, bridge.seed + 7, bridge.seed + 13]
        );
        // Le tablier est dans l'emprise verticale que le pilote lit.
        let span = crate::route::bridge_span();
        assert!((f64::from(deck.size.y) - (span.hi - span.lo)).abs() < 1e-3);
    }
}
