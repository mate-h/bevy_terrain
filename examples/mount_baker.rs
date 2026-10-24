//! Mount Baker terrain viewer — preprocessed from `source_data/mount_baker_full.tif`.
//!
//! Preprocess first:
//! `cargo run --release -p bevy_terrain_preprocess --example preprocess_mount_baker`
//!
//! Then run:
//! `cargo run --release --example mount_baker --features atmosphere_example`

use bevy::{
    camera::Exposure,
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin},
    input::keyboard::KeyCode,
    light::{
        Atmosphere, AtmosphereEnvironmentMapLight, SunDisk, VolumetricLight,
        atmosphere::ScatteringMedium, light_consts::lux,
    },
    math::DVec3,
    pbr::{AtmosphereMode, AtmosphereSettings},
    post_process::bloom::Bloom,
    prelude::*,
    reflect::TypePath,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    window::WindowResolution,
};
use bevy_terrain::{math::Coordinate, prelude::*};
use std::f32::consts::PI;

const EARTH_ATMOSPHERE_SHELL: f32 = 100_000.0;

/// Geographic center of `mount_baker_full.tif` (from gdalinfo).
const MOUNT_BAKER_CENTER_LAT: f64 = 48.777_292;
const MOUNT_BAKER_CENTER_LON: f64 = -121.813_147;
/// Elevation range from the source GeoTIFF statistics.
const MOUNT_BAKER_MIN_HEIGHT: f32 = 1194.905;
const MOUNT_BAKER_MAX_HEIGHT: f32 = 3251.913;
const MOUNT_BAKER_MEAN_HEIGHT: f32 = (MOUNT_BAKER_MIN_HEIGHT + MOUNT_BAKER_MAX_HEIGHT) * 0.5;
/// Highest lod in the preprocessed tile set.
const MOUNT_BAKER_LOD: u32 = 15;
/// Stay within the lod-15 load radius (~7 km) so tiles stream in.
const MOUNT_BAKER_CAMERA_ALTITUDE: f64 = 5_000.0;

#[derive(Resource, Default)]
struct GameState {
    paused: bool,
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Default)]
struct MountBakerMaterial {}

impl Material for MountBakerMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://bevy_terrain/shaders/render/fragment.wesl".into()
    }
}

fn lat_lon_to_unit_position(lat_deg: f64, lon_deg: f64) -> DVec3 {
    let lat = lat_deg.to_radians();
    let lon = lon_deg.to_radians();
    DVec3::new(-lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin())
}

fn geocentric_radius_at_elevation(
    shape: TerrainShape,
    lat_deg: f64,
    lon_deg: f64,
    height: f32,
) -> f32 {
    let unit = lat_lon_to_unit_position(lat_deg, lon_deg);
    let coordinate = Coordinate::from_unit_position(unit, true);
    coordinate.local_position(shape, height).length() as f32
}

fn mount_baker_atmosphere_radii() -> (f32, f32) {
    let inner_radius = geocentric_radius_at_elevation(
        TerrainShape::WGS84,
        MOUNT_BAKER_CENTER_LAT,
        MOUNT_BAKER_CENTER_LON,
        MOUNT_BAKER_MIN_HEIGHT,
    );
    (inner_radius, inner_radius + EARTH_ATMOSPHERE_SHELL)
}

fn mount_baker_coordinate() -> Coordinate {
    let unit = lat_lon_to_unit_position(MOUNT_BAKER_CENTER_LAT, MOUNT_BAKER_CENTER_LON);
    Coordinate::from_unit_position(unit, true)
}

fn mount_baker_surface_normal() -> Dir3 {
    let unit = mount_baker_coordinate().unit_position(true);
    Dir3::new_unchecked((TerrainShape::WGS84.scale() * unit).normalize().as_vec3())
}

fn mount_baker_surface_point() -> DVec3 {
    mount_baker_coordinate().local_position(TerrainShape::WGS84, MOUNT_BAKER_MEAN_HEIGHT)
}

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(GameState::default())
        .insert_resource(GlobalAmbientLight::NONE)
        .insert_resource(TerrainSettings::default())
        .insert_resource(TerrainShadowSettings {
            enabled: true,
            ..default()
        })
        .add_plugins((
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Mount Baker".into(),
                        resolution: WindowResolution::new(1920, 1080),
                        ..default()
                    }),
                    ..default()
                })
                .build()
                .disable::<TransformPlugin>(),
            FreeCameraPlugin,
            TerrainPlugin,
            TerrainMaterialPlugin::<MountBakerMaterial>::default(),
        ))
        .add_systems(Startup, (setup_scene, print_controls).chain())
        .add_systems(Update, (dynamic_scene, scene_controls))
        .run();
}

fn print_controls() {
    println!("Mount Baker Controls:");
    println!("    Enter      - Pause/Resume sun motion");
    println!("    T          - Toggle terrain shadows on/off");
    println!("    Up/Down    - Increase/Decrease exposure");
    println!("    WASD       - Move camera (FreeCamera)");
}

fn scene_controls(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut game_state: ResMut<GameState>,
    mut terrain_shadows: ResMut<TerrainShadowSettings>,
    mut camera_exposure: Query<&mut Exposure, With<Camera3d>>,
    time: Res<Time>,
) {
    if keyboard_input.just_pressed(KeyCode::Enter) {
        game_state.paused = !game_state.paused;
    }

    if keyboard_input.just_pressed(KeyCode::KeyT) {
        terrain_shadows.enabled = !terrain_shadows.enabled;
        println!(
            "Terrain shadows: {}",
            if terrain_shadows.enabled { "on" } else { "off" }
        );
    }

    if keyboard_input.pressed(KeyCode::ArrowUp) {
        for mut exposure in &mut camera_exposure {
            exposure.ev100 -= time.delta_secs() * 2.0;
        }
    }

    if keyboard_input.pressed(KeyCode::ArrowDown) {
        for mut exposure in &mut camera_exposure {
            exposure.ev100 += time.delta_secs() * 2.0;
        }
    }
}

fn setup_scene(
    mut commands: Commands,
    mut scattering_mediums: ResMut<Assets<ScatteringMedium>>,
    asset_server: Res<AssetServer>,
) {
    let earth_medium = scattering_mediums.add(ScatteringMedium::earth(256, 256));
    let mut atmosphere = Atmosphere::earth(earth_medium);
    let (inner_radius, outer_radius) = mount_baker_atmosphere_radii();
    atmosphere.inner_radius = inner_radius;
    atmosphere.outer_radius = outer_radius;

    println!(
        "Mount Baker bounds: {MOUNT_BAKER_MIN_HEIGHT:.1}–{MOUNT_BAKER_MAX_HEIGHT:.1} m \
         at ({MOUNT_BAKER_CENTER_LAT:.4}°, {MOUNT_BAKER_CENTER_LON:.4}°)"
    );

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

    let surface = mount_baker_surface_point();
    let up = mount_baker_surface_normal();
    let camera_position = surface + up.as_vec3().as_dvec3() * MOUNT_BAKER_CAMERA_ALTITUDE;
    let look_dir = (surface.as_vec3() - camera_position.as_vec3()).normalize();

    let mut view = Entity::PLACEHOLDER;

    commands.spawn_big_space(Grid::default(), |root| {
        root.spawn((
            Transform::IDENTITY,
            CellCoord::default(),
            GlobalTransform::from_translation(Vec3::Z),
            atmosphere,
        ));

        let grid = Grid::default();
        let (cell, local_translation) =
            grid.imprecise_translation_to_grid(camera_position.as_vec3());

        view = root
            .spawn_spatial((
                cell,
                Transform::from_translation(local_translation).looking_to(look_dir, up.as_vec3()),
                Camera3d::default(),
                Exposure { ev100: 13.0 },
                Bloom::NATURAL,
                AtmosphereSettings {
                    rendering_method: AtmosphereMode::Raymarched,
                    ..default()
                },
                FreeCamera {
                    walk_speed: 250.0,
                    run_speed: 1000.0,
                    ..default()
                },
                FloatingOrigin,
                AtmosphereEnvironmentMapLight::default(),
            ))
            .id();
    });

    commands.spawn_terrain(
        asset_server.load("terrains/mount_baker/config.tc.ron"),
        TerrainViewConfig {
            view_lod: MOUNT_BAKER_LOD,
            ..default()
        },
        MountBakerMaterial::default(),
        view,
    );
}

fn dynamic_scene(
    mut suns: Query<&mut Transform, With<DirectionalLight>>,
    time: Res<Time>,
    game_state: Res<GameState>,
) {
    if !game_state.paused {
        suns.iter_mut()
            .for_each(|mut tf| tf.rotate_x(-time.delta_secs() * PI / 10.0));
    }
}
