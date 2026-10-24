//! Streaming tiled heightfield terrain for [Bevy](https://bevy.org).
//!
//! The crate streams height and attachment data for spherical, WGS84 ellipsoid, and planar
//! worlds, and builds view-dependent geometry that stays stable at planetary scale. Terrain
//! data is available to systems and shaders. The default `big_space` feature adds a floating
//! origin; custom [`Material`](bevy::prelude::Material)s can sample the same attachments.
//!
//! # Background
//! There are three critical questions that each terrain renderer has to solve:
//!
//! ## How to store, manage and access the terrain data?
//! Each terrain has different types of textures associated with it.
//! For example a simple one might only need height and albedo information.
//! Because terrains can be quite large the space required for all of these so called
//! attachments, can/should not be stored in RAM and VRAM all at once.
//! Thus they have to be streamed in and out depending on the positions of the
//! viewers (cameras, lights, etc.).
//! Therefore the terrain is subdivided into a giant tile tree, whose tiles store their
//! section of these attachments.
//! This crate uses the chunked clipmap data structure, which consists of two pieces working together.
//! The wrapping [`TileTree`](prelude::TileTree) views together with
//! the [`TileAtlas`](prelude::TileAtlas) (the data structure
//! that stores all of the currently loaded data) can be used to efficiently retrieve
//! the best currently available data at any position for terrains of any size.
//! See the [`terrain_data`] module for more information.
//!
//! ## How to best approximate the terrain geometry?
//! Even a small terrain with a height map of 1000x1000 pixels would require 1 million vertices
//! to be rendered each frame per view, with a naive approach without an lod strategy.
//! To better distribute the vertices over the screen there exist many different algorithms.
//! This crate comes with its own default terrain geometry algorithm, called the
//! Uniform Distance-Dependent Level of Detail (UDLOD), which was developed with performance and
//! quality scalability in mind.
//! See the [`render`] module for more information.
//! You can also implement a different algorithm yourself and only use the terrain
//! data structures to solve the first question.
//!
//! ## How to shade the terrain?
//! The third and most important challenge of terrain rendering is the shading. This is a very
//! project specific problem and thus there does not exist a one-size-fits-all solution.
//! You can define your own terrain [Material](bevy::prelude::Material) and shader with all the
//! detail textures tailored to your application.
//! In the future this plugin will provide modular shader functions to make techniques like splat
//! mapping, triplane mapping, etc. easier.
//! Additionally a virtual texturing solution might be integrated to achieve better performance.

pub mod debug;
mod floating_origin;
pub mod formats;
pub mod math;
mod metal_capture_env;
pub mod picking;
pub mod planar_atmosphere;
pub mod plugin;
pub mod preprocess;
pub mod render;
pub mod shaders;
pub mod spawn;
pub mod terrain;
pub mod terrain_data;
pub mod terrain_shadow;
pub mod terrain_view;
pub mod util;

pub use metal_capture_env::prepare_metal_capture;

#[doc(hidden)]
pub mod prelude {
    //! `use bevy_terrain::prelude::*;` to import common components, bundles, and plugins.

    #[cfg(feature = "big_space")]
    pub use crate::debug::{DebugCameraController, OrbitalCameraController};
    pub use crate::{
        debug::{
            DebugTerrainMaterial, GroundAlignedFreeCamera, GroundAlignedFreeCameraPlugin,
            LoadingImages, TerrainDebugPlugin,
        },
        math::{TerrainShape, TileCoordinate},
        picking::{PickingData, TerrainPickingPlugin},
        planar_atmosphere::{PlanarAtmospherePlugin, PlanarAtmosphereSettings},
        plugin::{TerrainPlugin, TerrainSettings},
        prepare_metal_capture,
        // preprocess::{PreprocessDataset, Preprocessor, SphericalDataset, TerrainPreprocessPlugin},
        render::TerrainMaterialPlugin,
        spawn::SpawnTerrainCommandsExt,
        terrain::TerrainConfig,
        terrain_data::{
            AttachmentConfig, AttachmentFormat, AttachmentLabel, GpuTileAtlas, TerrainTileDropped,
            TerrainTileReady, TileAtlas, TileTree,
        },
        terrain_shadow::TerrainShadowSettings,
        terrain_view::{TerrainViewComponents, TerrainViewConfig},
    };
    #[cfg(feature = "big_space")]
    pub use big_space::{
        commands::BigSpaceCommands,
        grid::Grid,
        prelude::{CellCoord, FloatingOrigin},
    };
}
