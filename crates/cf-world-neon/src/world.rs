//! Le monde Neon City comme implémentation du trait [`World`] : géométrie du streaming et contrat avec le cœur de
//! `cf-render` (tronçons, portes, sujets).
//!
//! La route est un plan unique qui suit le point suivi (voir [`crate::road`]) ; les tronçons portent la ville : un
//! tronçon est une rangée d'immeubles de `scenery.ts` (voir [`crate::scenery`]). Les enseignes et les événements
//! déclareront leurs familles de pièces dans [`World::setup`] et les tireront dans [`World::plan_chunk`].

use bevy::ecs::world::World as EcsWorld;
use bevy::prelude::*;
use cf_render::streaming::ArchetypeId;
use cf_render::world::{SubjectContext, SubjectSink};
use cf_render::{ChunkInfo, ChunkPlan, PieceRegistry, Replan, StreamLayout, World, WorldError, WorldLayer};
use cf_show::model::ShowGates;

use crate::facade::FacadeMaterial;
use crate::scenery::arches::ArchPool;
use crate::scenery::{self, Clearing, MAX_PARTS, Row, SceneryAssets, SceneryConfig, SpotAnchor};
use crate::traffic::TrafficDirector;

/// Longueur d'un tronçon : l'écart entre deux rangées d'immeubles (`SPACING` de `scenery.ts`), en m.
pub const CHUNK_LENGTH_M: f32 = 26.0;
/// Rangées gardées derrière la voiture : 988 m (Sillage : 15 rangées, 390 m). Plus large que Sillage pour que les plans
/// qui regardent en arrière ou de haut voient encore la ville : la portée de culling (`FOG_END_M`) borne le coût.
pub const BEHIND: u32 = 38;
/// Rangées devant : 1 196 m (Sillage : 18 rangées). La caméra avance jusqu'à 190 m devant la voiture (`MAX_AHEAD`) et
/// voit jusqu'à `FOG_END_M` (988 m) au-delà.
pub const AHEAD: u32 = 46;
/// Graine du monde par défaut.
pub const DEFAULT_SEED: u64 = 0x0000_4E30_C17E;

/// La ville de Neon City.
#[derive(Resource, Debug, Clone)]
pub struct NeonWorld {
    seed: u64,
    scenery: SceneryConfig,
    /// Famille des pavés d'immeubles, connue après [`World::setup`].
    family: Option<ArchetypeId>,
    /// La ville (immeubles et arches) est affichée. Faux : la route seule, comme les captures de référence de la route.
    city: bool,
}

impl Default for NeonWorld {
    fn default() -> Self {
        Self::new(DEFAULT_SEED)
    }
}

impl NeonWorld {
    /// Un monde de graine `seed` : avec l'index d'un tronçon, elle détermine tout son contenu.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            scenery: SceneryConfig::default(),
            family: None,
            city: true,
        }
    }

    /// La graine du monde.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Le même monde sans ville : ni immeubles ni arches, la route seule (captures de référence de la route).
    #[must_use]
    pub fn without_city(mut self) -> Self {
        self.city = false;
        self
    }

    /// La ville est-elle affichée ?
    #[must_use]
    pub fn has_city(&self) -> bool {
        self.city
    }

    /// Les réglages de la ville (qualité, zones dégagées).
    #[must_use]
    pub fn scenery(&self) -> &SceneryConfig {
        &self.scenery
    }

    /// Niveau de qualité (0 = tout, 3 = le strict minimum) : retire d'abord les couronnes lointaines. Renvoie vrai si la
    /// ville change : l'appelant re-tire alors les tronçons vivants (`ChunkStream::replan_all`).
    pub fn set_quality(&mut self, level: usize) -> bool {
        self.scenery.set_quality(level)
    }

    /// Remplace les zones dégagées de la ville. Renvoie vrai si elles changent (à re-tirer comme [`Self::set_quality`]).
    pub fn set_clearings(&mut self, clearings: &[Clearing]) -> bool {
        self.scenery.set_clearings(clearings)
    }

    /// Zone dégagée de l'enseigne du DJ. Renvoie vrai si elle change (à re-tirer comme [`Self::set_quality`]).
    pub fn set_sign_clearing(&mut self, clearing: Option<Clearing>) -> bool {
        self.scenery.set_sign_clearing(clearing)
    }

    /// Une sphère (centre `at`, rayon `radius`, positions absolues) touche-t-elle un immeuble ? Pour la caméra et les
    /// météores : ce qu'on voit est ce qu'on percute.
    #[must_use]
    pub fn collides(&self, at: bevy::math::DVec3, radius: f64) -> bool {
        scenery::query::collides(&self.scenery, at, radius)
    }

    /// Hauteur du toit sous le point `(x, z)`, ou `None` s'il n'y a pas d'immeuble.
    #[must_use]
    pub fn roof_at(&self, x: f64, z: f64) -> Option<f32> {
        scenery::query::roof_at(&self.scenery, x, z)
    }

    /// Distance à l'axe de la route de la façade la plus proche d'un côté (−1 gauche, 1 droite) entre `z − depth` et
    /// `z + depth`, ou `None` s'il n'y a rien.
    #[must_use]
    pub fn facade(&self, side: i8, z: f64, depth: f64) -> Option<f32> {
        scenery::query::facade(&self.scenery, side, z, depth)
    }

    /// Les points d'attache des projecteurs de la rangée `row` (pour les faisceaux, J5-07).
    #[must_use]
    pub fn spots_in_row(&self, row: i64) -> Vec<SpotAnchor> {
        Row::generate(row, &self.scenery).spots().to_vec()
    }
}

impl World for NeonWorld {
    const ID: &'static str = "neon";
    type SubjectParam = (
        Option<Res<'static, TrafficDirector>>,
        Option<Res<'static, crate::signs::SignRoadRes>>,
        Option<Res<'static, crate::meteors::Meteors>>,
        Option<Res<'static, crate::djsign::DjSignRes>>,
    );

    fn layout(&self) -> StreamLayout {
        StreamLayout {
            chunk_length_m: CHUNK_LENGTH_M,
            behind: BEHIND,
            ahead: AHEAD,
            seed: self.seed,
        }
    }

    fn setup(&mut self, ecs: &mut EcsWorld, pieces: &mut PieceRegistry) -> Result<(), WorldError> {
        let mesh = ecs
            .get_resource_mut::<Assets<Mesh>>()
            .ok_or(WorldError::MissingResource("Assets<Mesh>"))?
            .add(scenery::unit_block());
        let material = ecs
            .get_resource_mut::<Assets<FacadeMaterial>>()
            .ok_or(WorldError::MissingResource("Assets<FacadeMaterial>"))?
            .add(FacadeMaterial::default());
        // `WorldLayer` dans le lot : Bevy ne propage pas les `RenderLayers` des tronçons à leurs pièces.
        let family = pieces.add(
            "immeubles",
            MAX_PARTS as u16,
            (
                WorldLayer,
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                scenery::fog_range(),
            ),
        );
        self.family = Some(family);
        let arches = ArchPool::spawn(ecs, &mesh, &material);
        ecs.insert_resource(arches);
        ecs.insert_resource(SceneryAssets {
            mesh,
            material,
            pieces: family,
        });
        Ok(())
    }

    fn plan_chunk(&self, chunk: &ChunkInfo, plan: &mut ChunkPlan) {
        if let (true, Some(family)) = (self.city, self.family) {
            scenery::plan_row(chunk.index, &self.scenery, family, plan);
        }
    }

    fn apply_gates(&mut self, _gates: &ShowGates) -> Replan {
        Replan::Keep
    }

    /// Les sujets du trafic (véhicules de la flotte, structures d'événements), s'il y a un trafic.
    fn subjects(
        &self,
        _ctx: &SubjectContext<'_>,
        (traffic, signs, meteors, dj_sign): &(
            Option<Res<'_, TrafficDirector>>,
            Option<Res<'_, crate::signs::SignRoadRes>>,
            Option<Res<'_, crate::meteors::Meteors>>,
            Option<Res<'_, crate::djsign::DjSignRes>>,
        ),
        out: &mut SubjectSink<'_>,
    ) {
        if let Some(traffic) = traffic {
            traffic.collect_subjects(|subject| {
                out.push(subject);
            });
        }
        // Les panneaux lisibles (J7-01) : les plans « enseigne » du réalisateur les filment.
        if let Some(signs) = signs {
            signs.0.collect_signs(|id, position, facing, width| {
                out.push(cf_render::world::Subject {
                    id,
                    kind: cf_render::world::SubjectKind::Sign,
                    position,
                    velocity: Vec3::ZERO,
                    size: width,
                    facing: Some(facing),
                });
            });
        }
        // L'enseigne géante du DJ (J7-02) : le plan `dj-sign` la filme. `id` change à chaque pose, le plan qui suivait
        // l'ancienne enseigne ne la retrouve donc pas quand une neuve prend sa place.
        if let Some(dj_sign) = dj_sign
            && let Some((position, facing, width)) = dj_sign.0.focus()
        {
            out.push(cf_render::world::Subject {
                id: u64::from(dj_sign.0.site_seq()),
                kind: cf_render::world::SubjectKind::DjSign,
                position,
                velocity: Vec3::ZERO,
                size: width,
                facing: Some(facing),
            });
        }
        // Les astres en vol (J7-10) : `meteor-ride` et `meteor-impact-watch` les filment.
        if let Some(meteors) = meteors {
            meteors.collect_subjects(|subject| {
                out.push(subject);
            });
        }
    }
}
