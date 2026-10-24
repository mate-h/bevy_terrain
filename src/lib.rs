//! Streaming heightfield terrain for [Bevy](https://bevy.org).
//!
//! Tiles stream in around each camera. A quadtree builds the mesh, morphs neighboring LODs
//! together, and draws them in one indirect call. The same path covers a flat game world and a
//! WGS84 planet. Start with [`prelude`].
//!
//! # Setup
//!
//! Preprocess a heightmap with the `bevy_terrain_preprocess` tool in this repository. It writes
//! `assets/terrains/<name>/config.tc.ron` plus the tile files. That tool is not published with
//! this crate.
//!
//! Then, in the app:
//!
//! 1. Add [`TerrainPlugin`](plugin::TerrainPlugin) and a [`TerrainMaterialPlugin`](render::TerrainMaterialPlugin) for your material.
//! 2. Insert [`TerrainSettings`](plugin::TerrainSettings) before anything spawns. Height is always bound. Pass every other
//!    attachment the shader samples: `TerrainSettings::new(vec!["albedo", "landcover"])`.
//! 3. Spawn a camera, then [`spawn_terrain`](spawn::SpawnTerrainCommandsExt::spawn_terrain) with the
//!    config handle, a [`TerrainViewConfig`](terrain_view::TerrainViewConfig), the material, and that camera.
//!
//! `spawn_terrain` waits until the config asset is loaded.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_terrain::prelude::*;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins((
//!             DefaultPlugins,
//!             TerrainPlugin,
//!             TerrainMaterialPlugin::<bevy::pbr::StandardMaterial>::default(),
//!         ))
//!         .insert_resource(TerrainSettings::new(vec!["albedo"]))
//!         .add_systems(Startup, setup)
//!         .run();
//! }
//!
//! fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
//!     let view = commands.spawn(Camera3d::default()).id();
//!     commands.spawn_terrain(
//!         asset_server.load("terrains/california/config.tc.ron"),
//!         TerrainViewConfig::default(),
//!         bevy::pbr::StandardMaterial::default(),
//!         view,
//!     );
//! }
//! ```
//!
//! The default `big_space` feature keeps positions stable at planetary scale.
//! [`TerrainPlugin`](plugin::TerrainPlugin) registers `BigSpaceDefaultPlugins`. Disable Bevy's `TransformPlugin` and
//! parent the camera with `spawn_big_space`, as `examples/spherical.rs` does. Drop the feature
//! with `default-features = false` when a normal `Transform` is enough.
//!
//! # Shading
//!
//! A terrain material is a Bevy [`Material`](bevy::prelude::Material). Keep
//! [`DEFAULT_VERTEX_SHADER`](shaders::DEFAULT_VERTEX_SHADER) and replace the fragment shader. Material bindings go in bind group 4.
//!
//! The fragment entry point takes `FragmentInput` and returns `FragmentOutput`, both imported
//! from `bevy_terrain::shaders::render::fragment`. Call `fragment_info`, then `lookup_tile`, then
//! sample:
//!
//! - `sample_height(tile)` and `sample_attachment(...)` from `bevy_terrain::shaders::attachments`
//! - the attachment's config at `terrain_data.attachments.<label>`
//! - its texture at `<label>_attachment`
//!
//! `<label>` is the string passed to [`TerrainSettings::new`](plugin::TerrainSettings::new), and it has to match the dataset.
//! `sample_attachment` linearly filters. Read id and mask maps with `textureLoad` or
//! `textureGather`, or neighboring ids blend into nonsense. `examples/spherical.rs` and
//! `assets/shaders/spherical.wesl` are the working pattern. For deferred rendering, call
//! `fragment_output` so the G-buffer gets written.
//!
//! # What the pieces are
//!
//! Each terrain is one [`TileAtlas`](terrain_data::TileAtlas): a texture-array cache of height and other attachments.
//! Each camera has a [`TileTree`](terrain_data::TileTree) that requests tiles around that viewer. [`TerrainTileReady`](terrain_data::TerrainTileReady)
//! and [`TerrainTileDropped`](terrain_data::TerrainTileDropped) fire when a tile becomes resident or is evicted.
//! [`TerrainShape`](math::TerrainShape) is a plane, a sphere, or [`TerrainShape::WGS84`](math::TerrainShape::WGS84).
//!
//! [`PlanarAtmospherePlugin`](planar_atmosphere::PlanarAtmospherePlugin) slides a planetary atmosphere shell under a flat map.
//! Enable the raymarched heightfield shadow with `TerrainShadowSettings { enabled: true, ..default() }`.
//! [`TerrainPickingPlugin`](picking::TerrainPickingPlugin) plus [`PickingData`](picking::PickingData) on the camera reports the cursor hit.
//!
//! # Features
//!
//! - `big_space` (default) — floating origin
//! - `rand` — the `precision_demo` example
//! - `atmosphere_example`, `bluenoise_texture`, `metal_capture` — example and GPU-capture flags

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

pub mod prelude {
    //! `use bevy_terrain::prelude::*;` for the types in the [crate overview](crate).

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
