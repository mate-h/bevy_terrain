use crate::{
    render::{
        GpuTerrainShadow, GpuTerrainView, TerrainDeferredTargets,
        terrain_material::{TerrainPipelineFlags, shadow_filter_shader_def},
    },
    shaders::TERRAIN_DEFERRED_LIGHTING_SHADER,
    terrain_shadow::{TerrainShadowSettings, TerrainShadowUniform},
    terrain_view::TerrainViewComponents,
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
        renderer::RenderDevice,
        sync_world::MainEntity,
        view::ExtractedView,
    },
};

/// The terrain whose shadow map the deferred lighting pass applies on a view.
///
/// When a single terrain renders on the view, the lighting pass looks up its shadow once per
/// pixel and the terrain draw skips the lookup. With several terrains, the pass can't tell them
/// apart, so each draw looks up its own shadow and passes it through the G-buffer's diffuse
/// occlusion.
pub(crate) fn lighting_pass_shadow_terrain(
    settings: &TerrainShadowSettings,
    terrain_views: &TerrainViewComponents<GpuTerrainView>,
    view: Entity,
) -> Option<Entity> {
    if !settings.enabled {
        return None;
    }
    let mut terrains = terrain_views
        .keys()
        .filter(|(_, terrain_view)| *terrain_view == view)
        .map(|(terrain, _)| *terrain);
    let terrain = terrains.next()?;
    terrains.next().is_none().then_some(terrain)
}

/// Fullscreen pipeline that lights the terrain's private G-buffer once per
/// pixel (see `terrain_deferred_lighting.wesl`), between the terrain draw and
/// the composite of [`terrain_deferred_pass`](super::terrain_deferred_pass).
#[derive(Resource)]
pub struct TerrainDeferredLightingPipeline {
    mesh_view_layouts: MeshPipelineViewLayouts,
    binding_arrays_are_usable: bool,
    pub(crate) input_layout: BindGroupLayoutDescriptor,
    shadow_layout: BindGroupLayoutDescriptor,
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

    let shadow_layout = BindGroupLayoutDescriptor::new(
        "terrain_deferred_lighting_shadow_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<TerrainShadowUniform>(false),
            ),
        ),
    );

    commands.insert_resource(TerrainDeferredLightingPipeline {
        mesh_view_layouts: mesh_pipeline.view_layouts.clone(),
        binding_arrays_are_usable: mesh_pipeline.binding_arrays_are_usable,
        input_layout,
        shadow_layout,
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
        let mut layout = vec![
            view_layout.main_layout.clone(),
            view_layout.binding_array_layout.clone(),
            self.input_layout.clone(),
        ];
        if key.flags.contains(TerrainPipelineFlags::TERRAIN_SHADOW) {
            layout.push(self.shadow_layout.clone());
        }

        RenderPipelineDescriptor {
            label: Some("terrain_deferred_lighting_pipeline".into()),
            layout,
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

/// The lighting pipeline of one view, with the terrain shadow bindings when the pass applies
/// the terrain shadow there.
pub struct TerrainDeferredLighting {
    pub(crate) pipeline: CachedRenderPipelineId,
    pub(crate) shadow_bind_group: Option<BindGroup>,
}

/// The specialized lighting pipeline of every view the terrain renders deferred on.
#[derive(Resource, Default, Deref, DerefMut)]
pub struct TerrainDeferredLightingPipelines(EntityHashMap<TerrainDeferredLighting>);

pub(crate) fn queue_terrain_deferred_lighting(
    pipeline_cache: Res<PipelineCache>,
    lighting_pipeline: Option<Res<TerrainDeferredLightingPipeline>>,
    mut pipelines: ResMut<SpecializedRenderPipelines<TerrainDeferredLightingPipeline>>,
    mut lighting_pipelines: ResMut<TerrainDeferredLightingPipelines>,
    view_key_cache: Res<ViewKeyCache>,
    render_device: Res<RenderDevice>,
    shadow_settings: Res<TerrainShadowSettings>,
    gpu_terrain_views: Res<TerrainViewComponents<GpuTerrainView>>,
    gpu_terrain_shadows: Res<TerrainViewComponents<GpuTerrainShadow>>,
    views: Query<
        (
            Entity,
            &MainEntity,
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

    for (view, main_view, extracted_view, has_atmosphere, has_environment_maps) in &views {
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

        let shadow_bind_group =
            lighting_pass_shadow_terrain(&shadow_settings, &gpu_terrain_views, main_view.id())
                .and_then(|terrain| gpu_terrain_shadows.get(&(terrain, main_view.id())))
                .map(|shadow| {
                    render_device.create_bind_group(
                        "terrain_deferred_lighting_shadow_bind_group",
                        &pipeline_cache.get_bind_group_layout(&lighting_pipeline.shadow_layout),
                        &BindGroupEntries::sequential((
                            shadow.texture_view(),
                            shadow.sampler(),
                            shadow.params_buffer().as_entire_binding(),
                        )),
                    )
                });
        if shadow_bind_group.is_some() {
            flags |= TerrainPipelineFlags::TERRAIN_SHADOW;
        }

        let key = TerrainDeferredLightingPipelineKey {
            flags,
            view_layout_key: MeshPipelineViewLayoutKey::from(*view_key),
            shadow_filter_method: view_key
                .intersection(MeshPipelineKey::SHADOW_FILTER_METHOD_RESERVED_BITS),
        };

        lighting_pipelines.insert(
            view,
            TerrainDeferredLighting {
                pipeline: pipelines.specialize(&pipeline_cache, &lighting_pipeline, key),
                shadow_bind_group,
            },
        );
    }
}
