//! [`TileAtlas`] caches tiled attachments. [`TileTree`] decides which tiles each view needs.
//!
//! An attachment is one tiled image alongside height: albedo, landcover, a mask, an id map.
//! Resolution and format are per attachment. Shaders read the atlas through the bind group
//! built from [`crate::plugin::TerrainSettings`].

mod attachment;
mod events;
mod gpu_attachment;
mod gpu_tile_atlas;
mod tile_atlas;
mod tile_loader;
mod tile_tree;

pub use self::{
    attachment::{AttachmentConfig, AttachmentFormat, AttachmentLabel},
    events::{TerrainTileDropped, TerrainTileReady},
    gpu_tile_atlas::GpuTileAtlas,
    tile_atlas::TileAtlas,
    tile_tree::TileTree,
};

pub(crate) use self::{attachment::*, gpu_attachment::*, tile_loader::*, tile_tree::*};

pub const INVALID_ATLAS_INDEX: u32 = u32::MAX;
pub const INVALID_LOD: u32 = u32::MAX;
