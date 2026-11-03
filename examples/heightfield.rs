//! A planar terrain from a heightfield generated in code, no preprocessing step.
//!
//! [`TerrainConfig::from_heightfield`] tiles the grid in memory. Pass any `width * height` heights
//! in meters, or [`Heightfield::from_image`] for a loaded grayscale image.
//!
//! `cargo run --release --example heightfield --features atmosphere_example`

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
/// Texels per side of the generated heightfield.
const RESOLUTION: u32 = 2048;
/// Peak height of the generated terrain, in meters.
const MAX_HEIGHT: f32 = 1500.0;
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
    mut terrain_configs: ResMut<Assets<TerrainConfig>>,
    asset_server: Res<AssetServer>,
) {
    let heightfield = Heightfield::new(RESOLUTION, RESOLUTION, generate_heights(RESOLUTION));
    let config = TerrainConfig::from_heightfield(
        &heightfield,
        &HeightfieldSettings::new(MAP_SIDE_LENGTH as f64),
    );
    let min_height = config.min_height;
    let config = terrain_configs.add(config);

    let gradient = asset_server.load("textures/gradient1.png");

    let earth_medium = scattering_mediums.add(ScatteringMedium::earth(256, 256));
    let mut atmosphere = Atmosphere::earth(earth_medium);
    let settings = PlanarAtmosphereSettings::new(
        min_height,
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
        config,
        TerrainViewConfig::default(),
        PlanarMaterial {
            gradient: gradient.clone(),
            gradient_info: GradientInfo { mode: 1 },
        },
        view,
    );
}

/// Domain-warped value-noise fBm with a ridged high-frequency band.
fn generate_heights(resolution: u32) -> Vec<f32> {
    fn hash(x: i32, y: i32) -> f32 {
        let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841);
        h = (h ^ (h >> 13)).wrapping_mul(0x5bd1_e995);
        (h ^ (h >> 15)) as f32 / u32::MAX as f32
    }

    fn value_noise(p: Vec2) -> f32 {
        let i = p.floor();
        let f = p - i;
        let u = f * f * (3.0 - 2.0 * f);
        let (x, y) = (i.x as i32, i.y as i32);
        let a = hash(x, y).lerp(hash(x + 1, y), u.x);
        let b = hash(x, y + 1).lerp(hash(x + 1, y + 1), u.x);
        a.lerp(b, u.y)
    }

    fn fbm(mut p: Vec2, octaves: u32) -> f32 {
        let (mut sum, mut amplitude) = (0.0, 0.5);
        for _ in 0..octaves {
            sum += amplitude * value_noise(p);
            p = Mat2::from_angle(0.6) * p * 2.03;
            amplitude *= 0.5;
        }
        sum
    }

    (0..resolution * resolution)
        .map(|i| {
            let p = Vec2::new((i % resolution) as f32, (i / resolution) as f32) / resolution as f32
                * 6.0;
            let warp = Vec2::new(fbm(p + 3.1, 4), fbm(p + 7.7, 4));
            let base = fbm(p + 2.0 * warp, 8);
            let ridges = 1.0 - (2.0 * fbm(p * 2.0, 5) - 1.0).abs();
            (base * base * 1.6 + 0.25 * base * ridges * ridges).min(1.0) * MAX_HEIGHT
        })
        .collect()
}
