//! [`TerrainConfig`], the `*.tc.ron` asset that describes one preprocessed dataset.
//!

use crate::{
    math::{TerrainShape, TileCoordinate},
    terrain_data::{AttachmentConfig, AttachmentLabel},
};
use bevy::{ecs::entity::hash_map::EntityHashMap, platform::collections::HashMap, prelude::*};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

/// Resource that stores components that are associated to a terrain entity.
/// This is used to persist components in the render world.
#[derive(Deref, DerefMut, Resource)]
pub struct TerrainComponents<C>(EntityHashMap<C>);

impl<C> Default for TerrainComponents<C> {
    fn default() -> Self {
        Self(default())
    }
}

/// One preprocessed dataset. Load it with `asset_server.load("terrains/<name>/config.tc.ron")`.
///
/// The preprocess tool writes this file. You rarely build it by hand.
#[derive(Serialize, Deserialize, Asset, TypePath, Debug, Clone)]
pub struct TerrainConfig {
    /// Terrain folder inside `assets/`. Tile files are loaded relative to this.
    pub path: String,
    /// Plane, sphere, or spheroid.
    pub shape: TerrainShape,
    /// How many LOD levels the dataset contains.
    pub lod_count: u32,
    /// Lowest stored height, in the dataset's units (meters for GeoTIFF preprocess).
    pub min_height: f32,
    /// Highest stored height, in the same units as `min_height`.
    pub max_height: f32,
    /// Attachment label to its tile format. Height is required.
    pub attachments: HashMap<AttachmentLabel, AttachmentConfig>,
    /// Every tile the dataset contains. The atlas only loads the ones a view requests.
    pub tiles: Vec<TileCoordinate>,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            shape: TerrainShape::Plane { side_length: 1.0 },
            lod_count: 1,
            min_height: 0.0,
            max_height: 1.0,
            path: default(),
            tiles: default(),
            attachments: default(),
        }
    }
}

impl TerrainConfig {
    /// Insert or replace one attachment definition.
    pub fn add_attachment(
        &mut self,
        label: AttachmentLabel,
        attachment: AttachmentConfig,
    ) -> &mut Self {
        self.attachments.insert(label, attachment);
        self
    }

    /// Read a `config.tc.ron` from a filesystem path. Prefer the asset loader at runtime.
    pub fn load_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let encoded = fs::read_to_string(path)?;
        Ok(ron::from_str(&encoded)?)
    }

    /// Write this config as pretty RON. The preprocess tool uses this.
    pub fn save_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let encoded = ron::ser::to_string_pretty(self, default())?;
        Ok(fs::write(path, encoded)?)
    }
}
