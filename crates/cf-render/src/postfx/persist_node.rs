//! Les deux passes de la persistance dans le rendu de Bevy (formules : [`super::persistence`]).
//!
//! L'historique `H` vit dans deux textures demi-résolution (`Rgba16Float`) créées à l'ouverture de la vue, et
//! seulement recréées si la fenêtre change de taille : aucune texture n'est allouée en régime établi. Chaque image,
//! `update` lit `H_{t−1}` et écrit `H_t` dans l'autre texture (lecture et écriture alternent), puis `composite` ajoute
//! la traînée à l'image. La passe tourne avant le bloom : les traînées rayonnent comme leur source.

use bevy::asset::{Handle, uuid_handle};
use bevy::core_pipeline::{Core3d, Core3dSystems, FullscreenShader};
use bevy::post_process::bloom::bloom;
use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::camera::ExtractedCamera;
use bevy::render::extract_component::{ComponentUniforms, DynamicUniformIndex, ExtractComponent};
use bevy::render::render_resource::binding_types::{sampler, texture_2d, uniform_buffer};
use bevy::render::render_resource::{
    AddressMode, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, CachedRenderPipelineId,
    ColorTargetState, ColorWrites, Extent3d, FilterMode, FragmentState, Operations, PipelineCache,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor, Sampler, SamplerBindingType,
    SamplerDescriptor, ShaderStages, ShaderType, Texture, TextureDescriptor, TextureDimension, TextureFormat,
    TextureSampleType, TextureUsages, TextureView, TextureViewDescriptor,
};
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::view::ViewTarget;
use bevy::render::{Render, RenderStartup, RenderSystems};
use bevy::shader::Shader;

use super::persistence::{THRESHOLD, TRAIL_GAIN, VANISHING_POINT};

pub(crate) const PERSIST_SHADER: Handle<Shader> = uuid_handle!("a3c8e710-52d9-4b1f-9e64-0d7f3b8c1a25");
pub(crate) const PERSIST_SOURCE: &str = include_str!("../../../../assets/shaders/cf-render/persist.wgsl");
pub const PERSIST_PASS_LABEL: &str = "cf_persist";
const HISTORY_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

/// Uniformes de la persistance, portés par la caméra du compositeur.
#[derive(Component, ExtractComponent, ShaderType, Debug, Clone, Copy, PartialEq)]
#[extract_app(RenderApp)]
pub struct PersistUniforms {
    pub decay: f32,
    pub zeta: f32,
    pub reset: f32,
    /// 1 : passes actives ; 0 : l'image passe sans toucher à l'historique.
    pub enabled: f32,
    pub vanishing: Vec2,
    pub threshold: f32,
    pub gain: f32,
    /// Texture lue cette image (0 ou 1) ; l'autre est écrite. Alterne à chaque image.
    pub parity: u32,
}

impl Default for PersistUniforms {
    fn default() -> Self {
        Self {
            decay: 0.0,
            zeta: 0.0,
            reset: 1.0,
            enabled: 0.0,
            vanishing: Vec2::from(VANISHING_POINT),
            threshold: THRESHOLD,
            gain: TRAIL_GAIN,
            parity: 0,
        }
    }
}

pub(crate) fn build_render(app: &mut App) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app
        .add_systems(RenderStartup, init_pipelines)
        .add_systems(Render, prepare_history.in_set(RenderSystems::PrepareResources))
        .add_systems(Core3d, run_persistence.in_set(Core3dSystems::PostProcess).before(bloom));
}

#[derive(Resource)]
struct PersistPipelines {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    update: CachedRenderPipelineId,
    composite: CachedRenderPipelineId,
}

fn init_pipelines(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    fullscreen: Res<FullscreenShader>,
    pipeline_cache: Res<PipelineCache>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        PERSIST_PASS_LABEL,
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<PersistUniforms>(true),
                texture_2d(TextureSampleType::Float { filterable: true }),
            ),
        ),
    );
    let sampler = render_device.create_sampler(&SamplerDescriptor {
        label: Some(PERSIST_PASS_LABEL),
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    let queue = |entry: &'static str| {
        pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some(format!("{PERSIST_PASS_LABEL}_{entry}").into()),
            layout: vec![layout.clone()],
            vertex: fullscreen.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: PERSIST_SHADER,
                entry_point: Some(entry.into()),
                targets: vec![Some(ColorTargetState {
                    format: HISTORY_FORMAT,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
                ..default()
            }),
            ..default()
        })
    };
    commands.insert_resource(PersistPipelines {
        update: queue("update"),
        composite: queue("composite"),
        layout,
        sampler,
    });
}

/// Les deux textures d'historique d'une vue, avec la taille pour laquelle elles ont été créées.
#[derive(Component)]
struct History {
    _textures: [Texture; 2],
    views: [TextureView; 2],
    size: UVec2,
}

/// Crée l'historique de chaque vue qui en porte les uniformes ; ne recrée que si la taille de la vue change.
fn prepare_history(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    views: Query<(Entity, &ViewTarget, Option<&History>), With<PersistUniforms>>,
) {
    for (entity, target, history) in &views {
        let size = ({
            let e = target.main_texture().size();
            UVec2::new(e.width, e.height) / 2
        })
        .max(UVec2::ONE);
        if history.is_some_and(|h| h.size == size) {
            continue;
        }
        let make = |label: &'static str| {
            render_device.create_texture(&TextureDescriptor {
                label: Some(label),
                size: Extent3d {
                    width: size.x,
                    height: size.y,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: HISTORY_FORMAT,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let textures = [make("cf_persist_history_a"), make("cf_persist_history_b")];
        let views = [
            textures[0].create_view(&TextureViewDescriptor::default()),
            textures[1].create_view(&TextureViewDescriptor::default()),
        ];
        commands.entity(entity).insert(History {
            _textures: textures,
            views,
            size,
        });
    }
}

fn run_persistence(
    view: ViewQuery<
        (
            &ViewTarget,
            &DynamicUniformIndex<PersistUniforms>,
            &PersistUniforms,
            &History,
        ),
        With<ExtractedCamera>,
    >,
    pipelines: Option<Res<PersistPipelines>>,
    uniforms: Res<ComponentUniforms<PersistUniforms>>,
    pipeline_cache: Res<PipelineCache>,
    mut ctx: RenderContext,
) {
    let (view_target, index, settings, history) = view.into_inner();
    let (Some(pipelines), Some(binding)) = (pipelines, uniforms.uniforms().binding()) else {
        return;
    };
    if settings.enabled < 0.5 {
        return;
    }
    let (Some(update), Some(composite)) = (
        pipeline_cache.get_render_pipeline(pipelines.update),
        pipeline_cache.get_render_pipeline(pipelines.composite),
    ) else {
        return;
    };
    let read = (settings.parity & 1) as usize;
    let write = 1 - read;
    let post_process = view_target.post_process_write();
    let layout = pipeline_cache.get_bind_group_layout(&pipelines.layout);
    let device = ctx.render_device().clone();
    let bind = |history_view: &TextureView| {
        device.create_bind_group(
            PERSIST_PASS_LABEL,
            &layout,
            &BindGroupEntries::sequential((post_process.source, &pipelines.sampler, binding.clone(), history_view)),
        )
    };
    let update_group = bind(&history.views[read]);
    let composite_group = bind(&history.views[write]);
    for (label, target, pipeline, group) in [
        ("cf_persist_update", &history.views[write], update, &update_group),
        (
            "cf_persist_composite",
            post_process.destination,
            composite,
            &composite_group,
        ),
    ] {
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: Operations::default(),
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[index.index()]);
        pass.draw(0..3, 0..1);
    }
}
