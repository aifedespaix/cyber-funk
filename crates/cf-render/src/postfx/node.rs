//! Le nœud d'étalonnage dans le rendu de Bevy : pipeline, groupes de liaison et passe plein écran.
//!
//! Calqué sur `bevy::core_pipeline::fullscreen_material` (Bevy 0.20), à deux différences près, qui justifient de ne
//! pas l'utiliser tel quel :
//!
//! - **échantillonneur linéaire** : celui de `FullscreenMaterial` est au plus proche ; l'aberration chromatique, la
//!   réfraction des gouttes et le flou radial décalent la lecture d'une fraction de pixel, que Sillage (cibles
//!   three.js en `LinearFilter`) interpole ;
//! - **ordre explicite** : la passe tourne après le bloom de Bevy et avant le tonemapping, comme le `ShaderPass` de
//!   Sillage entre l'`UnrealBloomPass` et l'`OutputPass`.
//!
//! Les réglages sont un composant de la caméra ([`GradeUniforms`]), extrait vers le monde de rendu par
//! `ExtractComponentPlugin` et téléversé par `UniformComponentPlugin` : ils suivent la caméra qui les porte (le
//! compositeur) et elle seule, ce qu'une `ExtractResource` globale ne saurait pas faire avec deux compositeurs sur la
//! même fenêtre.

use bevy::asset::{Handle, uuid_handle};
use bevy::core_pipeline::tonemapping::tonemapping;
use bevy::core_pipeline::{Core3d, Core3dSystems, FullscreenShader};
use bevy::ecs::error::BevyError;
use bevy::post_process::bloom::bloom;
use bevy::prelude::*;
use bevy::render::camera::ExtractedCamera;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::extract_component::{ComponentUniforms, DynamicUniformIndex};
use bevy::render::render_resource::binding_types::{sampler, texture_2d, uniform_buffer};
use bevy::render::render_resource::{
    AddressMode, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, CachedRenderPipelineId,
    Canonical, ColorTargetState, ColorWrites, FilterMode, FragmentState, Operations, PipelineCache,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor, Sampler,
    SamplerBindingType, SamplerDescriptor, ShaderStages, Specializer, SpecializerKey, TextureFormat,
    TextureSampleType, TextureView, Variants,
};
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::view::{ExtractedView, PostProcessBindGroupCache, PostProcessBindGroupCacheBuilder, ViewTarget};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};
use bevy::shader::Shader;

use super::grade::GradeUniforms;

/// Le shader d'étalonnage, embarqué dans le binaire (aucun fichier lu pendant un show).
pub(crate) const GRADE_SHADER: Handle<Shader> = uuid_handle!("0b6f0c55-3c1e-4f0e-9f57-2c9a7e4d1a42");
/// Source WGSL du shader (`assets/shaders/cf-render/grade.wgsl`).
pub(crate) const GRADE_SOURCE: &str = include_str!("../../../../assets/shaders/cf-render/grade.wgsl");
/// Étiquette de la passe, reprise par les diagnostics de rendu (temps GPU par passe).
pub const GRADE_PASS_LABEL: &str = "cf_grade";

/// Enregistre le pipeline et la passe dans le monde de rendu. Sans monde de rendu (tests sans GPU), ne fait rien.
pub(crate) fn build_render(app: &mut App) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app
        .add_systems(RenderStartup, init_pipeline)
        .add_systems(
            Render,
            (
                prepare_pipelines.in_set(RenderSystems::Prepare),
                prepare_bind_groups.in_set(RenderSystems::PrepareBindGroups),
            ),
        )
        .add_systems(
            Core3d,
            grade_pass
                .in_set(Core3dSystems::PostProcess)
                .after(bloom)
                .before(tonemapping),
        );
}

#[derive(Resource)]
struct GradePipeline {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    variants: Variants<RenderPipeline, GradeSpecializer>,
}

struct GradeSpecializer;

#[derive(PartialEq, Eq, Hash, Clone, Copy, SpecializerKey)]
struct GradeKey {
    target_format: TextureFormat,
}

impl Specializer<RenderPipeline> for GradeSpecializer {
    type Key = GradeKey;

    fn specialize(
        &self,
        key: Self::Key,
        descriptor: &mut RenderPipelineDescriptor,
    ) -> Result<Canonical<Self::Key>, BevyError> {
        let fragment = descriptor.fragment_mut()?;
        fragment.set_target(
            0,
            ColorTargetState {
                format: key.target_format,
                blend: None,
                write_mask: ColorWrites::ALL,
            },
        );
        Ok(key)
    }
}

fn init_pipeline(mut commands: Commands, render_device: Res<RenderDevice>, fullscreen: Res<FullscreenShader>) {
    let layout = BindGroupLayoutDescriptor::new(
        "cf_grade_bind_group_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<GradeUniforms>(true),
            ),
        ),
    );
    let sampler = render_device.create_sampler(&SamplerDescriptor {
        label: Some("cf_grade_sampler"),
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    let descriptor = RenderPipelineDescriptor {
        label: Some("cf_grade_pipeline".into()),
        layout: vec![layout.clone()],
        vertex: fullscreen.to_vertex_state(),
        fragment: Some(FragmentState {
            shader: GRADE_SHADER,
            // Remplacé par le format de la vue à la spécialisation.
            targets: vec![Some(ColorTargetState {
                format: TextureFormat::Rgba16Float,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            ..default()
        }),
        ..default()
    };
    commands.insert_resource(GradePipeline {
        layout,
        sampler,
        variants: Variants::new(GradeSpecializer, descriptor),
    });
}

#[derive(Component)]
struct GradePipelineId(CachedRenderPipelineId);

/// Les groupes de liaison des deux textures principales (la source alterne d'une passe à l'autre).
#[derive(Component)]
struct GradeBindGroups(PostProcessBindGroupCache);

fn prepare_pipelines(
    mut commands: Commands,
    pipeline_cache: Res<PipelineCache>,
    pipeline: Option<ResMut<GradePipeline>>,
    views: Query<(Entity, &ExtractedView, Has<GradeUniforms>), With<ExtractedCamera>>,
) -> Result<(), BevyError> {
    let Some(mut pipeline) = pipeline else {
        return Ok(());
    };
    for (entity, view, graded) in &views {
        if !graded {
            commands.entity(entity).remove::<GradePipelineId>();
            continue;
        }
        let key = GradeKey {
            target_format: view.target_format,
        };
        let id = pipeline.variants.specialize(&pipeline_cache, key)?;
        commands.entity(entity).insert(GradePipelineId(id));
    }
    Ok(())
}

fn prepare_bind_groups(
    mut commands: Commands,
    mut views: Query<(Entity, &ViewTarget, Option<&mut GradeBindGroups>, Has<GradeUniforms>)>,
    pipeline: Option<Res<GradePipeline>>,
    pipeline_cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<GradeUniforms>>,
    render_device: Res<RenderDevice>,
) {
    let Some(pipeline) = pipeline else { return };
    let Some(binding) = uniforms.uniforms().binding() else {
        return;
    };
    for (entity, view_target, bind_groups, graded) in &mut views {
        if !graded {
            commands.entity(entity).remove::<GradeBindGroups>();
            continue;
        }
        let builder = PostProcessBindGroupCacheBuilder::new(|texture: &TextureView| {
            (
                texture.id(),
                render_device.create_bind_group(
                    "cf_grade_bind_group",
                    &pipeline_cache.get_bind_group_layout(&pipeline.layout),
                    &BindGroupEntries::sequential((texture, &pipeline.sampler, binding.clone())),
                ),
            )
        });
        if let Some(mut bind_groups) = bind_groups {
            if bind_groups.0.should_update(view_target) {
                bind_groups.0.update(view_target, builder);
            }
        } else {
            commands
                .entity(entity)
                .insert(GradeBindGroups(builder.generate_bind_groups(view_target)));
        }
    }
}

/// La passe : un triangle plein écran, de la texture principale courante vers l'autre.
fn grade_pass(
    view: ViewQuery<(
        &ViewTarget,
        &DynamicUniformIndex<GradeUniforms>,
        &GradeBindGroups,
        &GradePipelineId,
    )>,
    pipeline_cache: Res<PipelineCache>,
    mut ctx: RenderContext,
) {
    let (view_target, uniform_index, bind_groups, pipeline_id) = view.into_inner();
    // Pipeline pas encore compilé (premières images) : l'image passe sans étalonnage plutôt que de bloquer.
    let Some(pipeline) = pipeline_cache.get_render_pipeline(pipeline_id.0) else {
        return;
    };
    let post_process = view_target.post_process_write();
    let bind_group = bind_groups.0.get_current_bind_group(post_process.source);
    let descriptor = RenderPassDescriptor {
        label: Some(GRADE_PASS_LABEL),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: post_process.destination,
            depth_slice: None,
            resolve_target: None,
            ops: Operations::default(),
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    };
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let mut pass = ctx.command_encoder().begin_render_pass(&descriptor);
    let span = diagnostics.pass_span(&mut pass, GRADE_PASS_LABEL);
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[uniform_index.index()]);
    pass.draw(0..3, 0..1);
    span.end(&mut pass);
}
