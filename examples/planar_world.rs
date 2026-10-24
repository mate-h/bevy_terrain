//! Flat game-world terrain on a planetary atmosphere shell.
//!
//! Preprocess first:
//! `cargo run --release -p bevy_terrain_preprocess --example preprocess_planar`
//!
//! Then run (works with or without the default `big_space` feature):
//! `cargo run --release --example planar_world --features atmosphere_example`

use bevy::{
    camera::Exposure,
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin},
    light::{
        Atmosphere, AtmosphereEnvironmentMapLight, SunDisk, VolumetricLight,
        atmosphere::ScatteringMedium, light_consts::lux,
    },
    pbr::{AtmosphereMode, AtmosphereSettings},
    post_process::bloom::Bloom,
    prelude::*,
    reflect::TypePath,
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};
use bevy_terrain::prelude::*;
#[cfg(feature = "big_space")]
use big_space::prelude::{FloatingOrigin, Grid};

const EARTH_MINOR_RADIUS: f64 = 6356752.314245;
const EARTH_ATMOSPHERE_SHELL: f32 = 100_000.0;
const MAP_SIDE_LENGTH: f32 = 8192.0;
/// From `assets/terrains/planar_world/config.tc.ron`.
const PLANAR_MIN_HEIGHT: f32 = 0.0;
/// When false, the planet center stays fixed at the map origin instead of following the camera.
const PLANAR_ATMOSPHERE_FOLLOW_CAMERA: bool = true;

#[derive(ShaderType, Clone)]
struct GradientInfo {
    mode: u32,
}

#[derive(Asset, AsBindGroup, TypePath, Clone)]
struct PlanarMaterial {
    #[texture(0)]
    #[sampler(1)]
    gradient: Handle<Image>,
    #[uniform(2)]
    gradient_info: GradientInfo,
}

impl Material for PlanarMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/planar.wesl".into()
    }
}

fn main() {
    let mut app = App::new();

    #[cfg(feature = "big_space")]
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<TransformPlugin>()
            .add(FreeCameraPlugin)
            .add(TerrainPlugin)
            .add(TerrainMaterialPlugin::<PlanarMaterial>::default())
            .add(PlanarAtmospherePlugin),
    );

    #[cfg(not(feature = "big_space"))]
    app.add_plugins((
        DefaultPlugins,
        FreeCameraPlugin,
        TerrainPlugin,
        TerrainMaterialPlugin::<PlanarMaterial>::default(),
        PlanarAtmospherePlugin,
    ));

    app.insert_resource(TerrainSettings::default())
        .insert_resource(GlobalAmbientLight::NONE)
        .add_systems(Startup, setup_scene)
        .run();
}

fn setup_scene(
    mut commands: Commands,
    mut scattering_mediums: ResMut<Assets<ScatteringMedium>>,
    asset_server: Res<AssetServer>,
) {
    let gradient = asset_server.load("textures/gradient1.png");

    let earth_medium = scattering_mediums.add(ScatteringMedium::earth(256, 256));
    let mut atmosphere = Atmosphere::earth(earth_medium);
    let settings = PlanarAtmosphereSettings::new(
        PLANAR_MIN_HEIGHT,
        EARTH_MINOR_RADIUS,
        EARTH_ATMOSPHERE_SHELL,
        PLANAR_ATMOSPHERE_FOLLOW_CAMERA,
    );
    atmosphere.inner_radius = settings.inner_radius;
    atmosphere.outer_radius = settings.outer_radius;
    commands.insert_resource(settings);

    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            contact_shadows_enabled: true,
            illuminance: lux::RAW_SUNLIGHT,
            ..default()
        },
        Transform::from_xyz(1.0, 0.4, 0.0).looking_at(Vec3::ZERO, Vec3::Y),
        VolumetricLight,
        SunDisk::EARTH,
    ));

    let camera_distance = MAP_SIDE_LENGTH;
    let camera_bundle = (
        Transform::from_xyz(0.0, camera_distance * 0.6, camera_distance)
            .looking_at(Vec3::ZERO, Vec3::Y),
        Camera3d::default(),
        Exposure { ev100: 13.0 },
        Bloom::NATURAL,
        AtmosphereSettings {
            rendering_method: AtmosphereMode::Raymarched,
            ..default()
        },
        FreeCamera {
            walk_speed: 500.0,
            run_speed: 2000.0,
            ..default()
        },
        AtmosphereEnvironmentMapLight::default(),
    );

    let view = {
        #[cfg(feature = "big_space")]
        {
            let mut view = Entity::PLACEHOLDER;
            let grid = Grid::default();
            let (atmosphere_cell, atmosphere_local) = settings.initial_big_space_position(&grid);
            commands.spawn_big_space(grid, |root| {
                root.spawn((
                    Transform::from_translation(atmosphere_local),
                    atmosphere_cell,
                    atmosphere,
                ));
                view = root.spawn_spatial((camera_bundle, FloatingOrigin)).id();
            });
            view
        }

        #[cfg(not(feature = "big_space"))]
        {
            commands.spawn((settings.initial_transform(), atmosphere));
            commands.spawn(camera_bundle).id()
        }
    };

    commands.spawn_terrain(
        asset_server.load("terrains/planar_world/config.tc.ron"),
        TerrainViewConfig::default(),
        PlanarMaterial {
            gradient: gradient.clone(),
            gradient_info: GradientInfo { mode: 1 },
        },
        view,
    );
}
