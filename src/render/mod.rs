//! View-dependent terrain mesh and the draw pipeline.
//!
//! A compute prepass refines a quadtree of tiles per view, morphs LODs so they meet, and draws
//! the result with one indirect call. The type to add to an app is [`TerrainMaterialPlugin`].

mod terrain_bind_group;
mod terrain_material;
mod terrain_pass;
mod terrain_shadow;
mod terrain_view_bind_group;
mod tiling_prepass;

pub use self::{
    terrain_bind_group::{GpuTerrain, TerrainUniform},
    terrain_material::TerrainMaterialPlugin,
    terrain_shadow::GpuTerrainShadow,
    terrain_view_bind_group::{GpuTerrainView, TerrainViewBindGroup},
};

pub(crate) use self::{
    terrain_bind_group::*, terrain_pass::*, terrain_shadow::*, terrain_view_bind_group::*,
    tiling_prepass::*,
};
