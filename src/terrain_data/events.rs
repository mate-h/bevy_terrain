//! Lifecycle messages emitted by the [`TileAtlas`](super::TileAtlas) as tiles become
//! GPU-resident and are later evicted.
//!
//! These let downstream code react to the visual streaming state without reaching into
//! the atlas internals — e.g. debug overlays, or building/freeing per-tile raytracing
//! acceleration structures that follow tile residency. Gameplay/simulation streaming does
//! not need these; it drives its own tile requests through the [`TileTree`](super::TileTree).

use crate::math::TileCoordinate;
use bevy::prelude::*;

/// Emitted once when every attachment of a tile has finished loading and the tile has
/// become resident in its atlas slot — the same edge at which the atlas starts serving it
/// to tile trees. (The GPU texture upload follows on the next render frame.)
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainTileReady {
    /// The terrain entity that owns the tile atlas.
    pub terrain: Entity,
    /// The coordinate of the tile that became resident.
    pub coordinate: TileCoordinate,
    /// The atlas slot the tile now occupies.
    pub atlas_index: u32,
}

/// Emitted when a previously ready tile is evicted from the atlas and its slot is reused,
/// so its data is no longer available.
///
/// Only fired for tiles that had previously been reported ready via [`TerrainTileReady`];
/// a tile that is dropped before finishing loading produces no message.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainTileDropped {
    /// The terrain entity that owns the tile atlas.
    pub terrain: Entity,
    /// The coordinate of the tile that was evicted.
    pub coordinate: TileCoordinate,
    /// The atlas slot the tile used to occupy (now reused by another tile).
    pub atlas_index: u32,
}
