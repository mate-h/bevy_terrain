use crate::{
    shaders::MIP_SHADER,
    terrain::TerrainComponents,
    terrain_data::{AttachmentFormat, GpuTileAtlas},
};
use bevy::shader::ShaderDefVal;
use bevy::{
    asset::{AssetServer, Handle},
    platform::collections::HashMap,
    prelude::*,
    render::{
        render_resource::{binding_types::*, *},
        renderer::{RenderContext, RenderDevice},
    },
};
use strum::IntoEnumIterator;

pub(crate) fn create_mip_layout_descriptor(format: AttachmentFormat) -> BindGroupLayoutDescriptor {
    BindGroupLayoutDescriptor::new(
        "mip_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                uniform_buffer::<u32>(false), // atlas_index
                texture_2d_array(TextureSampleType::Float { filterable: true }), // parent
                texture_storage_2d_array(
                    format.processing_format(),
                    StorageTextureAccess::WriteOnly,
                ), // child
            ),
        ),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MipPipelineKey {
    pub(crate) format: AttachmentFormat,
}

impl MipPipelineKey {
    pub fn shader_defs(&self) -> Vec<ShaderDefVal> {
        let mut shader_defs = Vec::new();

        let format = match self.format {
            AttachmentFormat::Rgb8U => "RGB8U",
            AttachmentFormat::Rgba8U => "RGBA8U",
            AttachmentFormat::Rgba8ULinear => "RGBA8U",
            AttachmentFormat::R8U => "R8U",
            AttachmentFormat::R16U => "R16U",
            AttachmentFormat::R16I => "R16I",
            AttachmentFormat::Rg16U => "RG16U",
            AttachmentFormat::R32F => "R32F",
        };

        shader_defs.push(format.into());

        shader_defs
    }
}

#[derive(Resource)]
pub struct MipPipelines {
    pub(crate) mip_layouts: HashMap<AttachmentFormat, BindGroupLayoutDescriptor>,
    mip_shader: Handle<Shader>,
}

impl FromWorld for MipPipelines {
    fn from_world(world: &mut World) -> Self {
        let _device = world.resource::<RenderDevice>();
        let asset_server = world.resource::<AssetServer>();

        let mip_layouts = AttachmentFormat::iter()
            .map(|format| (format, create_mip_layout_descriptor(format)))
            .collect();
        let mip_shader = asset_server.load(MIP_SHADER);

        Self {
            mip_layouts,
            mip_shader,
        }
    }
}

impl SpecializedComputePipeline for MipPipelines {
    type Key = MipPipelineKey;

    fn specialize(&self, key: Self::Key) -> ComputePipelineDescriptor {
        ComputePipelineDescriptor {
            label: Some("mip_pipeline".into()),
            layout: vec![self.mip_layouts[&key.format].clone()],
            immediate_size: 0,
            shader: self.mip_shader.clone(),
            shader_defs: key.shader_defs(),
            entry_point: Some("main".into()),
            zero_initialize_workgroup_memory: false,
            constants: vec![],
        }
    }
}

pub fn mip_prepass(
    mut ctx: RenderContext,
    pipeline_cache: Res<PipelineCache>,
    gpu_tile_atlases: Res<TerrainComponents<GpuTileAtlas>>,
) {
    let mut pass = ctx
        .command_encoder()
        .begin_compute_pass(&ComputePassDescriptor::default());

    for gpu_tile_atlas in gpu_tile_atlases.values() {
        gpu_tile_atlas.generate_mip(&mut pass, &pipeline_cache);
    }
}
