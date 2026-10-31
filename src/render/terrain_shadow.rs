use crate::{
    render::TerrainTilingPrepassPipelines,
    shaders::SHADOW_MAP_SHADER,
    terrain::TerrainComponents,
    terrain_data::{GpuTileAtlas, TileTree},
    terrain_shadow::{TerrainShadowRefresh, TerrainShadowSettings, TerrainShadowUniform},
    terrain_view::TerrainViewComponents,
    util::GpuBuffer,
};
use bevy::shader::ShaderDefVal;
use bevy::{
    prelude::*,
    render::{
        Extract,
        render_asset::RenderAssets,
        render_resource::{binding_types::*, *},
        renderer::{RenderContext, RenderDevice, RenderQueue},
        storage::{GpuShaderBuffer, ShaderBuffer},
    },
};

const SHADOW_MAP_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TerrainShadowPipelineKey {
    pub spherical: bool,
}

impl TerrainShadowPipelineKey {
    pub fn shader_defs(&self) -> Vec<ShaderDefVal> {
        let mut shader_defs = vec!["SHADOW_COMPUTE".into()];

        if self.spherical {
            shader_defs.push("SPHERICAL".into());
        }

        shader_defs
    }
}

#[derive(Resource)]
pub struct TerrainShadowPipelines {
    params_layout: BindGroupLayoutDescriptor,
    terrain_layout: BindGroupLayoutDescriptor,
    view_layout: BindGroupLayoutDescriptor,
    output_layout: BindGroupLayoutDescriptor,
    shader: Handle<Shader>,
}

impl FromWorld for TerrainShadowPipelines {
    fn from_world(world: &mut World) -> Self {
        let terrain_layout = world
            .resource::<TerrainTilingPrepassPipelines>()
            .terrain_layout
            .clone();

        let params_layout = BindGroupLayoutDescriptor::new(
            "terrain_shadow_params_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (uniform_buffer::<TerrainShadowUniform>(false),),
            ),
        );
        let view_layout = BindGroupLayoutDescriptor::new(
            "terrain_shadow_view_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    storage_buffer_read_only_sized(false, None), // terrain_view
                    storage_buffer_read_only_sized(false, None), // tile_tree
                ),
            ),
        );
        let output_layout = BindGroupLayoutDescriptor::new(
            "terrain_shadow_output_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (texture_storage_2d(
                    SHADOW_MAP_FORMAT,
                    StorageTextureAccess::WriteOnly,
                ),),
            ),
        );

        let shader = world.load_asset(SHADOW_MAP_SHADER);

        Self {
            params_layout,
            terrain_layout,
            view_layout,
            output_layout,
            shader,
        }
    }
}

impl SpecializedComputePipeline for TerrainShadowPipelines {
    type Key = TerrainShadowPipelineKey;

    fn specialize(&self, key: Self::Key) -> ComputePipelineDescriptor {
        ComputePipelineDescriptor {
            label: Some("terrain_shadow_pipeline".into()),
            layout: vec![
                self.params_layout.clone(),
                self.terrain_layout.clone(),
                self.view_layout.clone(),
                self.output_layout.clone(),
            ],
            immediate_size: 0,
            shader: self.shader.clone(),
            shader_defs: key.shader_defs(),
            entry_point: Some("compute_shadow_map".into()),
            zero_initialize_workgroup_memory: false,
            constants: vec![],
        }
    }
}

/// The per-(terrain, view) GPU resources of the terrain shadow map.
pub struct GpuTerrainShadow {
    pub(crate) map_size: u32,
    pub(crate) texture_view: TextureView,
    pub(crate) sampler: Sampler,
    pub(crate) params_buffer: GpuBuffer<TerrainShadowUniform>,
    pub(crate) pipeline: Option<CachedComputePipelineId>,
    /// A regeneration was requested but has not run yet.
    pending: bool,
    /// `terrain_shadow_pass` regenerates the map this frame.
    dispatch: bool,
    params_bind_group: BindGroup,
    view_bind_group: Option<BindGroup>,
    output_bind_group: BindGroup,
    terrain_view_buffer: Handle<ShaderBuffer>,
    tile_tree_buffer: Handle<ShaderBuffer>,
}

impl GpuTerrainShadow {
    /// The shadow map. It lives as long as the terrain view and the raymarch
    /// rewrites it in place, so external bind groups can hold onto it.
    pub fn texture_view(&self) -> &TextureView {
        &self.texture_view
    }

    pub fn sampler(&self) -> &Sampler {
        &self.sampler
    }

    /// The `TerrainShadowUniform` buffer, rewritten every frame. `enabled`
    /// is 0 until the raymarch can run.
    pub fn params_buffer(&self) -> &Buffer {
        &self.params_buffer
    }

    fn new(
        device: &RenderDevice,
        pipeline_cache: &PipelineCache,
        pipelines: &TerrainShadowPipelines,
        tile_tree: &TileTree,
        settings: &TerrainShadowSettings,
    ) -> Self {
        let map_size = settings.map_size;

        let texture = device.create_texture(&TextureDescriptor {
            label: Some("terrain_shadow_map"),
            size: Extent3d {
                width: map_size,
                height: map_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: SHADOW_MAP_FORMAT,
            usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&TextureViewDescriptor::default());

        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("terrain_shadow_sampler"),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            ..default()
        });

        let params_buffer = GpuBuffer::create_labeled(
            "terrain_shadow_params",
            device,
            &TerrainShadowUniform::default(),
            BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        );

        let params_bind_group = device.create_bind_group(
            "terrain_shadow_params_bind_group",
            &pipeline_cache.get_bind_group_layout(&pipelines.params_layout),
            &BindGroupEntries::sequential((&params_buffer,)),
        );
        let output_bind_group = device.create_bind_group(
            "terrain_shadow_output_bind_group",
            &pipeline_cache.get_bind_group_layout(&pipelines.output_layout),
            &BindGroupEntries::sequential((&texture_view,)),
        );

        Self {
            map_size,
            texture_view,
            sampler,
            params_buffer,
            pipeline: None,
            pending: true,
            dispatch: false,
            params_bind_group,
            view_bind_group: None,
            output_bind_group,
            terrain_view_buffer: tile_tree.terrain_view_buffer.clone(),
            tile_tree_buffer: tile_tree.tile_tree_buffer.clone(),
        }
    }

    pub(crate) fn initialize(
        device: Res<RenderDevice>,
        pipeline_cache: Res<PipelineCache>,
        pipelines: Res<TerrainShadowPipelines>,
        settings: Res<TerrainShadowSettings>,
        mut gpu_terrain_shadows: ResMut<TerrainViewComponents<GpuTerrainShadow>>,
        tile_trees: Extract<Res<TerrainViewComponents<TileTree>>>,
    ) {
        for (&(terrain, view), tile_tree) in tile_trees.iter() {
            if gpu_terrain_shadows.contains_key(&(terrain, view)) {
                continue;
            }

            gpu_terrain_shadows.insert(
                (terrain, view),
                GpuTerrainShadow::new(&device, &pipeline_cache, &pipelines, tile_tree, &settings),
            );
        }
    }

    pub(crate) fn prepare(
        device: Res<RenderDevice>,
        queue: Res<RenderQueue>,
        pipeline_cache: Res<PipelineCache>,
        pipelines: Res<TerrainShadowPipelines>,
        buffers: Res<RenderAssets<GpuShaderBuffer>>,
        gpu_terrains: Res<TerrainComponents<crate::render::GpuTerrain>>,
        settings: Res<TerrainShadowSettings>,
        uniforms: Res<TerrainViewComponents<TerrainShadowUniform>>,
        refreshes: Res<TerrainViewComponents<TerrainShadowRefresh>>,
        mut gpu_terrain_shadows: ResMut<TerrainViewComponents<GpuTerrainShadow>>,
    ) {
        for (&(terrain, view), gpu_terrain_shadow) in gpu_terrain_shadows.iter_mut() {
            if gpu_terrain_shadow.view_bind_group.is_none()
                && let (Some(terrain_view_buffer), Some(tile_tree_buffer)) = (
                    buffers.get(&gpu_terrain_shadow.terrain_view_buffer),
                    buffers.get(&gpu_terrain_shadow.tile_tree_buffer),
                )
            {
                gpu_terrain_shadow.view_bind_group = Some(device.create_bind_group(
                    "terrain_shadow_view_bind_group",
                    &pipeline_cache.get_bind_group_layout(&pipelines.view_layout),
                    &BindGroupEntries::sequential((
                        terrain_view_buffer.buffer.as_entire_binding(),
                        tile_tree_buffer.buffer.as_entire_binding(),
                    )),
                ));
            }

            // Shaders skip the map until `terrain_shadow_pass` can write it.
            // Until then its texels are zero, which reads as a shadow at
            // elevation 0.
            let ready = gpu_terrain_shadow
                .pipeline
                .and_then(|id| pipeline_cache.get_compute_pipeline(id))
                .is_some()
                && gpu_terrain_shadow.view_bind_group.is_some()
                && gpu_terrains
                    .get(&terrain)
                    .is_some_and(|gpu_terrain| gpu_terrain.terrain_bind_group.is_some());

            // Keep a request pending until the pass can actually run it.
            gpu_terrain_shadow.pending |= refreshes
                .get(&(terrain, view))
                .is_some_and(|refresh| refresh.frames > 0);
            gpu_terrain_shadow.dispatch = gpu_terrain_shadow.pending && ready && settings.enabled;
            gpu_terrain_shadow.pending &= !gpu_terrain_shadow.dispatch;

            if let Some(uniform) = uniforms.get(&(terrain, view)) {
                let mut uniform = uniform.clone();
                uniform.enabled &= ready as u32;
                gpu_terrain_shadow.params_buffer.set_value(uniform);
                gpu_terrain_shadow.params_buffer.update(&queue);
            }
        }
    }

    pub(crate) fn queue(
        pipeline_cache: Res<PipelineCache>,
        shadow_pipelines: Res<TerrainShadowPipelines>,
        mut pipelines: ResMut<SpecializedComputePipelines<TerrainShadowPipelines>>,
        gpu_tile_atlases: Res<TerrainComponents<GpuTileAtlas>>,
        mut gpu_terrain_shadows: ResMut<TerrainViewComponents<GpuTerrainShadow>>,
    ) {
        for (&(terrain, _view), gpu_terrain_shadow) in gpu_terrain_shadows.iter_mut() {
            let key = TerrainShadowPipelineKey {
                spherical: gpu_tile_atlases[&terrain].is_spherical,
            };

            gpu_terrain_shadow.pipeline =
                Some(pipelines.specialize(&pipeline_cache, &shadow_pipelines, key));
        }
    }
}

/// Regenerates the terrain shadow maps by raymarching the terrain height map towards the sun.
pub fn terrain_shadow_pass(
    mut ctx: RenderContext,
    settings: Res<TerrainShadowSettings>,
    pipeline_cache: Res<PipelineCache>,
    gpu_terrains: Res<TerrainComponents<crate::render::GpuTerrain>>,
    gpu_terrain_shadows: Res<TerrainViewComponents<GpuTerrainShadow>>,
) {
    if !settings.enabled {
        return;
    }

    let mut pass = ctx
        .command_encoder()
        .begin_compute_pass(&ComputePassDescriptor {
            label: Some("terrain_shadow_pass"),
            ..default()
        });

    for (&(terrain, _view), gpu_terrain_shadow) in gpu_terrain_shadows.iter() {
        if !gpu_terrain_shadow.dispatch {
            continue;
        }
        let Some(pipeline) = gpu_terrain_shadow
            .pipeline
            .and_then(|id| pipeline_cache.get_compute_pipeline(id))
        else {
            continue;
        };
        let Some(view_bind_group) = &gpu_terrain_shadow.view_bind_group else {
            continue;
        };
        let Some(terrain_bind_group) = gpu_terrains
            .get(&terrain)
            .and_then(|gpu_terrain| gpu_terrain.terrain_bind_group.as_ref())
        else {
            continue;
        };

        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &gpu_terrain_shadow.params_bind_group, &[]);
        pass.set_bind_group(1, terrain_bind_group, &[]);
        pass.set_bind_group(2, view_bind_group, &[]);
        pass.set_bind_group(3, &gpu_terrain_shadow.output_bind_group, &[]);

        let workgroups = gpu_terrain_shadow.map_size.div_ceil(8);
        pass.dispatch_workgroups(workgroups, workgroups, 1);
    }
}
