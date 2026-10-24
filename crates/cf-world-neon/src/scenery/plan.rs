//! La rangée devenue plan de tronçon.

use bevy::prelude::*;
use cf_render::ChunkPlan;
use cf_render::streaming::ArchetypeId;

use super::layout::{Row, SceneryConfig};

/// Remplit `plan` avec la ville de la rangée `index` : chaque pavé est un `Transform` (position en repère de tronçon,
/// échelle = taille) et son tag.
///
/// Le tronçon d'index `i` est ancré en `z = −i × 26` : c'est la rangée `i` de Sillage, dont les tours ne s'écartent que du
/// décalage aléatoire (±3 m) de leur position nominale. Aucune allocation.
pub(crate) fn plan_row(index: i64, config: &SceneryConfig, family: ArchetypeId, plan: &mut ChunkPlan) {
    let row = Row::generate(index, config);
    for part in row.parts() {
        let [w, h, d] = part.size;
        plan.push(
            family,
            Transform {
                translation: Vec3::new(part.x, part.y, part.z),
                scale: Vec3::new(w, h, d),
                ..Transform::IDENTITY
            },
            part.tag(),
        );
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::Component;
    use cf_render::PieceRegistry;

    use super::super::layout::MAX_PARTS;
    use super::*;

    #[derive(Component, Clone)]
    struct Marker;

    #[test]
    fn la_capacite_d_un_tronçon_suffit_a_toute_rangee() {
        let mut registry = PieceRegistry::default();
        let family = registry.add("immeubles", MAX_PARTS as u16, Marker);
        let mut plan = ChunkPlan::new(&registry);
        let config = SceneryConfig::default();
        let mut most = 0;
        for index in -50..2_000 {
            plan.clear();
            plan_row(index, &config, family, &mut plan);
            assert_eq!(plan.refused(), 0, "rangée {index} : famille pleine");
            most = most.max(plan.len());
        }
        assert!(most > 10, "une rangée a au moins quelques tours ({most})");
    }

    #[test]
    fn meme_rangee_meme_plan() {
        let mut registry = PieceRegistry::default();
        let family = registry.add("immeubles", MAX_PARTS as u16, Marker);
        let config = SceneryConfig::default();
        let (mut a, mut b) = (ChunkPlan::new(&registry), ChunkPlan::new(&registry));
        plan_row(1234, &config, family, &mut a);
        plan_row(1234, &config, family, &mut b);
        assert_eq!(a, b);
        assert!(!a.is_empty());
    }
}
