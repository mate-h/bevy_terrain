use crate::terrain::TerrainConfig;
use bevy::asset::{AssetLoader, LoadContext, io::Reader};
use bevy::prelude::*;
use std::io;

/// Loads `*.tc.ron` files as [`TerrainConfig`](crate::terrain::TerrainConfig).
#[derive(Default, TypePath)]
pub struct TerrainConfigLoader;

impl AssetLoader for TerrainConfigLoader {
    type Asset = TerrainConfig;
    type Settings = ();
    type Error = io::Error;

    fn extensions(&self) -> &[&str] {
        &["tc.ron"]
    }

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<TerrainConfig, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        ron::from_str(
            std::str::from_utf8(&bytes)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?,
        )
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }
}
