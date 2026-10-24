//! Loaders for `*.tc.ron` configs ([`TerrainConfigLoader`]) and terrain TIFF tiles ([`TiffLoader`]).
//! [`TerrainPlugin`](crate::plugin::TerrainPlugin) registers both.

mod ron;
mod tiff;

pub use self::ron::TerrainConfigLoader;
pub use self::tiff::TiffLoader;
