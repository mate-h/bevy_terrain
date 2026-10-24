//! [`TerrainViewConfig`] is the per-camera quality knob. [`TerrainViewComponents`] maps
//! `(terrain entity, view entity)` to the state for that pair.

use bevy::{platform::collections::HashMap, prelude::*};

/// Resource that stores components that are associated to a terrain entity and a view entity.
#[derive(Deref, DerefMut, Resource)]
pub struct TerrainViewComponents<C>(HashMap<(Entity, Entity), C>);

impl<C> Default for TerrainViewComponents<C> {
    fn default() -> Self {
        Self(default())
    }
}

/// Quality settings for one camera looking at a terrain.
///
/// [`Default`](TerrainViewConfig::default) is what the examples start from. Distances are in
/// multiples of a tile, not meters.
#[derive(Clone)]
pub struct TerrainViewConfig {
    /// The count of tiles in x and y direction per tile tree layer.
    pub tree_size: u32,
    /// The size of the tile buffer.
    pub geometry_tile_count: u32,
    /// The upper bound on the amount of steps the tile list will be refined.
    /// The effective count is derived from the subdivision distance (one step
    /// per reachable quadtree level) and only capped by this value.
    pub refinement_count: u32,
    /// The number of rows and columns of the tile grid.
    pub grid_size: u32,
    /// The morph percentage of the mesh.
    pub morph_range: f32,
    /// The blend percentage in the vertex and fragment shader.
    pub blend_range: f32,
    /// Distance, in tile sizes, between adjacent LOD layers.
    ///
    /// Keep this above about 6. Tiles only morph to the neighboring LOD, so a smaller gap
    /// pops the transition when the next LOD arrives. `I`/`O` adjust it when [`TerrainDebugPlugin`](crate::debug::TerrainDebugPlugin) is installed.
    pub morph_distance: f64,
    /// Distance, in tile sizes, over which fragment colors blend into the next LOD.
    pub blend_distance: f64,
    /// Extra fraction of the subdivision distance, covering cube-sphere distortion near face corners.
    ///
    /// Use `0.0` on a plane and about `0.1` on a sphere or spheroid.
    pub subdivision_tolerance: f64,
    /// Extra fraction of `blend_distance` added when streaming, so tiles arrive before they are drawn.
    pub load_tolerance: f64,
    /// Fraction of the shape's scale. Inside this distance, vertices take the high-precision path.
    pub precision_distance: f64,
    /// LOD of the reference tile used by that high-precision path. `0` is the finest.
    ///
    /// Pick a LOD the dataset actually contains.
    pub view_lod: u32,
    /// Overlap order when several terrains share one view. Higher values win the stencil test.
    pub order: u32,
}

impl Default for TerrainViewConfig {
    fn default() -> Self {
        Self {
            tree_size: 16,
            geometry_tile_count: 1000000,
            refinement_count: 30,
            grid_size: 16,
            morph_range: 0.2,
            blend_range: 0.2,
            morph_distance: 40.0,
            blend_distance: 5.0,
            subdivision_tolerance: 0.1,
            load_tolerance: 0.2,
            precision_distance: 0.001,
            view_lod: 10,
            order: 0,
        }
    }
}
