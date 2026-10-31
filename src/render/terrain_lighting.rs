use crate::{
    render::{
        TerrainDeferredTargets,
        terrain_material::{TerrainPipelineFlags, shadow_filter_shader_def},
    },
    shaders::TERRAIN_DEFERRED_LIGHTING_SHADER,
};
use bevy::{
    core_pipeline::{FullscreenShader, deferred::DEFERRED_PREPASS_FORMAT},
    ecs::entity::EntityHashMap,
    light::EnvironmentMapLight,
    pbr::{
        ExtractedAtmosphere, MeshPipeline, MeshPipelineKey, MeshPipelineViewLayoutKey,
        MeshPipelineViewLayouts, RenderViewLightProbes, ViewKeyCache,
    },
    prelude::*,
    render::{
        render_resource::{binding_types::*, *},
        view::ExtractedView,
    },
};

/// Fullscreen pipeline that lights the terrain's private G-buffer once per
/// pixel (see `terrain_deferred_lighting.wesl`), between the terrain draw and
/// the composite of [`terrain_deferred_pass`](super::terrain_deferred_pass).
#[derive(Resource)]
pub struct TerrainDeferredLightingPipeline {
    mesh_view_layouts: MeshPipelineViewLayouts,
    binding_arrays_are_usable: bool,
    pub(crate) input_layout: BindGroupLayoutDescriptor,
    fullscreen: FullscreenShader,
    shader: Handle<Shader>,
}

pub(crate) fn init_terrain_deferred_lighting_pipeline(
    mut commands: Commands,
    mesh_pipeline: Res<MeshPipeline>,
    fullscreen: Res<FullscreenShader>,
    asset_server: Res<AssetServer>,
) {
    let input_layout = BindGroupLayoutDescriptor::new(
        "terrain_deferred_lighting_input_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (texture_2d(TextureSampleType::Uint), texture_depth_2d()),
        ),
    );

    commands.insert_resource(TerrainDeferredLightingPipeline {
        mesh_view_layouts: mesh_pipeline.view_layouts.clone(),
        binding_arrays_are_usable: mesh_pipeline.binding_arrays_are_usable,
        input_layout,
        fullscreen: fullscreen.clone(),
        shader: asset_server.load(TERRAIN_DEFERRED_LIGHTING_SHADER),
    });
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TerrainDeferredLightingPipelineKey {
    /// The lighting related subset of the terrain draw's flags.
    flags: TerrainPipelineFlags,
    /// Matches the view's `mesh_view_bind_group`, like the terrain draw's.
    view_layout_key: MeshPipelineViewLayoutKey,
    shadow_filter_method: MeshPipelineKey,
}

impl SpecializedRenderPipeline for TerrainDeferredLightingPipeline {
    type Key = TerrainDeferredLightingPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let mut shader_defs = key.flags.shader_defs();
        if self.binding_arrays_are_usable {
            shader_defs.push("MULTIPLE_LIGHT_PROBES_IN_ARRAY".into());
        }
        shader_defs.extend(shadow_filter_shader_def(key.shadow_filter_method));

        let view_layout = self.mesh_view_layouts.get_view_layout(key.view_layout_key);

        RenderPipelineDescriptor {
            label: Some("terrain_deferred_lighting_pipeline".into()),
            layout: vec![
                view_layout.main_layout.clone(),
                view_layout.binding_array_layout.clone(),
                self.input_layout.clone(),
            ],
            immediate_size: 0,
            vertex: self.fullscreen.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                shader_defs,
                entry_point: Some("fragment".into()),
                targets: vec![Some(ColorTargetState {
                    format: DEFERRED_PREPASS_FORMAT,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
                constants: vec![],
            }),
            primitive: default(),
            depth_stencil: None,
            multisample: default(),
            zero_initialize_workgroup_memory: false,
        }
    }
}

/// The specialized lighting pipeline of every view the terrain renders deferred on.
#[derive(Resource, Default, Deref, DerefMut)]
pub struct TerrainDeferredLightingPipelines(EntityHashMap<CachedRenderPipelineId>);

pub(crate) fn queue_terrain_deferred_lighting(
    pipeline_cache: Res<PipelineCache>,
    lighting_pipeline: Option<Res<TerrainDeferredLightingPipeline>>,
    mut pipelines: ResMut<SpecializedRenderPipelines<TerrainDeferredLightingPipeline>>,
    mut lighting_pipelines: ResMut<TerrainDeferredLightingPipelines>,
    view_key_cache: Res<ViewKeyCache>,
    views: Query<
        (
            Entity,
            &ExtractedView,
            Has<ExtractedAtmosphere>,
            Has<RenderViewLightProbes<EnvironmentMapLight>>,
        ),
        With<TerrainDeferredTargets>,
    >,
) {
    lighting_pipelines.clear();

    let Some(lighting_pipeline) = lighting_pipeline else {
        return;
    };

    for (view, extracted_view, has_atmosphere, has_environment_maps) in &views {
        let Some(view_key) = view_key_cache.get(&extracted_view.retained_view_entity) else {
            continue;
        };

        // Same derivation as `queue_terrain`.
        let mut flags = TerrainPipelineFlags::NONE;
        if extracted_view.target_format != TextureFormat::Rgba8UnormSrgb
            && extracted_view.target_format != TextureFormat::Rgba8Unorm
        {
            flags |= TerrainPipelineFlags::HDR;
        }
        if has_atmosphere {
            flags |= TerrainPipelineFlags::ATMOSPHERE;
        }
        if has_environment_maps {
            flags |= TerrainPipelineFlags::ENVIRONMENT_MAP;
        }

        let key = TerrainDeferredLightingPipelineKey {
            flags,
            view_layout_key: MeshPipelineViewLayoutKey::from(*view_key),
            shadow_filter_method: view_key
                .intersection(MeshPipelineKey::SHADOW_FILTER_METHOD_RESERVED_BITS),
        };

        lighting_pipelines.insert(
            view,
            pipelines.specialize(&pipeline_cache, &lighting_pipeline, key),
        );
    }
}
