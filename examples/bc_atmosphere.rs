//! Atmosphere example with British Columbia terrain.
//!
//! Preprocess first:
//! `cargo run --release -p bevy_terrain_preprocess --example preprocess_bc`
//!
//! Run:
//! `cargo run --release --example bc_atmosphere --features atmosphere_example`

use bevy::{
    asset::io::AssetSourceBuilder,
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
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};
use bevy_terrain::{math::Coordinate, prelude::*};
use big_space::prelude::Grid;
use std::f32::consts::PI;

const EARTH_MINOR_RADIUS: f32 = 6356752.314245;
const EARTH_ATMOSPHERE_SHELL: f32 = 100_000.0;

/// Geographic center of `source_data/bc_height.tif` (from gdalinfo).
const BC_CENTER_LAT: f64 = 50.505_253;
const BC_CENTER_LON: f64 = -122.272_600;
/// Elevation range from `assets/terrains/bc/config.tc.ron`.
const BC_MIN_HEIGHT: f32 = 65.059_395;
const BC_MAX_HEIGHT: f32 = 2848.676;
const BC_MEAN_HEIGHT: f32 = (BC_MIN_HEIGHT + BC_MAX_HEIGHT) * 0.5;
/// Highest lod in the preprocessed tile set.
const BC_LOD: u32 = 14;
/// Stay within the lod-14 load radius so tiles stream in.
const BC_CAMERA_ALTITUDE: f64 = 10_000.0;

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

fn bc_atmosphere_radii() -> (f32, f32) {
    let inner_radius = geocentric_radius_at_elevation(
        TerrainShape::WGS84,
        BC_CENTER_LAT,
        BC_CENTER_LON,
        BC_MIN_HEIGHT,
    );
    (inner_radius, inner_radius + EARTH_ATMOSPHERE_SHELL)
}

fn bc_coordinate() -> Coordinate {
    let unit = lat_lon_to_unit_position(BC_CENTER_LAT, BC_CENTER_LON);
    Coordinate::from_unit_position(unit, true)
}

fn bc_surface_normal() -> Dir3 {
    let unit = bc_coordinate().unit_position(true);
    Dir3::new_unchecked((TerrainShape::WGS84.scale() * unit).normalize().as_vec3())
}

fn bc_surface_point() -> DVec3 {
    bc_coordinate().local_position(TerrainShape::WGS84, BC_MEAN_HEIGHT)
}

#[derive(Resource, Default)]
struct GameState {
    paused: bool,
}

#[derive(Resource)]
struct AtmospherePresets {
    earth: Handle<ScatteringMedium>,
    mars: Handle<ScatteringMedium>,
}

#[derive(Clone, Copy, ShaderType)]
struct BcMaterialSettings {
    show_albedo: u32,
    show_normal: u32,
}

impl Default for BcMaterialSettings {
    fn default() -> Self {
        Self {
            show_albedo: 1,
            show_normal: 0,
        }
    }
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Default)]
struct BcMaterial {
    #[uniform(0)]
    settings: BcMaterialSettings,
}

impl Material for BcMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/bc_atmosphere.wgsl".into()
    }
}

fn main() {
    App::new()
        .register_asset_source(
            "bevy",
            AssetSourceBuilder::platform_default("../bevy/assets", None),
        )
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(GameState::default())
        .insert_resource(GlobalAmbientLight::NONE)
        .insert_resource(TerrainSettings::new(vec!["albedo", "normal"]))
        .insert_resource(TerrainShadowSettings {
            enabled: true,
            ..default()
        })
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            FreeCameraPlugin,
            TerrainPlugin,
            TerrainMaterialPlugin::<BcMaterial>::default(),
        ))
        .add_systems(Startup, (setup_scene, print_controls))
        .add_systems(
            Update,
            (dynamic_scene, atmosphere_controls, terrain_overlay_controls),
        )
        .run();
}

fn print_controls() {
    println!("Atmosphere + BC Terrain Controls:");
    println!("    1          - Switch to lookup texture rendering method");
    println!("    2          - Switch to raymarched rendering method");
    println!("    3          - Switch to Earth atmosphere");
    println!("    4          - Switch to Mars atmosphere");
    println!("    Enter      - Pause/Resume sun motion");
    println!("    T          - Toggle terrain shadows on/off");
    println!("    I          - Toggle color albedo overlay on/off");
    println!("    N          - Toggle normal map overlay on/off");
    println!("    Up/Down    - Increase/Decrease exposure");
    println!("    WASD       - Move camera (FreeCamera)");
}

fn atmosphere_controls(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut planet_atmosphere: Query<(&mut Atmosphere, &mut GlobalTransform)>,
    mut camera_settings: Query<&mut AtmosphereSettings, With<Camera3d>>,
    mut sun_disks: Query<&mut SunDisk, With<DirectionalLight>>,
    atmosphere_presets: Res<AtmospherePresets>,
    mut game_state: ResMut<GameState>,
    mut terrain_shadows: ResMut<TerrainShadowSettings>,
    mut camera_exposure: Query<&mut Exposure, With<Camera3d>>,
    time: Res<Time>,
) {
    if keyboard_input.just_pressed(KeyCode::Digit3) {
        let (inner_radius, outer_radius) = bc_atmosphere_radii();
        for (mut atmosphere, mut transform) in &mut planet_atmosphere {
            *atmosphere = Atmosphere::earth(atmosphere_presets.earth.clone());
            atmosphere.inner_radius = inner_radius;
            atmosphere.outer_radius = outer_radius;
            *transform = GlobalTransform::from_translation(-Vec3::Y * atmosphere.inner_radius);
        }
        for mut sun_disk in &mut sun_disks {
            sun_disk.angular_size = SunDisk::EARTH.angular_size;
        }
        println!("Switched to Earth atmosphere");
    }

    if keyboard_input.just_pressed(KeyCode::Digit4) {
        for (mut atmosphere, mut transform) in &mut planet_atmosphere {
            *atmosphere = Atmosphere::mars(atmosphere_presets.mars.clone());
            *transform = GlobalTransform::from_translation(-Vec3::Y * atmosphere.inner_radius);
        }
        for mut sun_disk in &mut sun_disks {
            sun_disk.angular_size = SunDisk::MARS.angular_size;
        }
        println!("Switched to Mars atmosphere");
    }

    if keyboard_input.just_pressed(KeyCode::Digit1) {
        for mut settings in &mut camera_settings {
            settings.rendering_method = AtmosphereMode::LookupTexture;
            println!("Switched to lookup texture rendering method");
        }
    }

    if keyboard_input.just_pressed(KeyCode::Digit2) {
        for mut settings in &mut camera_settings {
            settings.rendering_method = AtmosphereMode::Raymarched;
            println!("Switched to raymarched rendering method");
        }
    }

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
    let mars_phase = asset_server.load("bevy://textures/mars_mie_phase.ktx2");
    let mars_medium = scattering_mediums.add(ScatteringMedium::mars(256, 256, mars_phase));

    commands.insert_resource(AtmospherePresets {
        earth: earth_medium.clone(),
        mars: mars_medium.clone(),
    });

    let mut atmosphere = Atmosphere::earth(earth_medium);
    let (inner_radius, outer_radius) = bc_atmosphere_radii();
    atmosphere.inner_radius = inner_radius;
    atmosphere.outer_radius = outer_radius;
    println!(
        "BC terrain bounds from config.tc.ron: \
         {BC_MIN_HEIGHT:.1}–{BC_MAX_HEIGHT:.1} m at ({BC_CENTER_LAT:.4}°, {BC_CENTER_LON:.4}°)"
    );
    println!(
        "Atmosphere inner radius at lowest terrain point: {inner_radius:.3} m \
         (WGS84 minor axis reference: {EARTH_MINOR_RADIUS:.3} m)"
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

    let mut view = Entity::PLACEHOLDER;

    commands.spawn_big_space(Grid::default(), |root| {
        root.spawn((
            Transform::IDENTITY,
            CellCoord::default(),
            GlobalTransform::from_translation(Vec3::Z),
            atmosphere,
        ));

        let surface = bc_surface_point();
        let up = bc_surface_normal();
        let camera_position = surface + up.as_vec3().as_dvec3() * BC_CAMERA_ALTITUDE;
        let look_dir = (surface.as_vec3() - camera_position.as_vec3()).normalize();
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
                    walk_speed: 2500.0,
                    run_speed: 10000.0,
                    ..default()
                },
                FloatingOrigin,
                AtmosphereEnvironmentMapLight::default(),
            ))
            .id();
    });

    commands.spawn_terrain(
        asset_server.load("terrains/bc/config.tc.ron"),
        TerrainViewConfig {
            view_lod: BC_LOD,
            ..default()
        },
        BcMaterial::default(),
        view,
    );
}

fn terrain_overlay_controls(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut materials: ResMut<Assets<BcMaterial>>,
    terrain_materials: Query<&MeshMaterial3d<BcMaterial>, With<TileAtlas>>,
) {
    let toggle_albedo = keyboard_input.just_pressed(KeyCode::KeyI);
    let toggle_normal = keyboard_input.just_pressed(KeyCode::KeyN);

    if !toggle_albedo && !toggle_normal {
        return;
    }

    for terrain_material in &terrain_materials {
        let Some(mut material) = materials.get_mut(terrain_material.id()) else {
            continue;
        };

        if toggle_albedo {
            material.settings.show_albedo = 1 - material.settings.show_albedo;
            if material.settings.show_albedo != 0 {
                material.settings.show_normal = 0;
            }
            println!(
                "Color albedo overlay: {}",
                if material.settings.show_albedo != 0 {
                    "on"
                } else {
                    "off"
                }
            );
        }

        if toggle_normal {
            material.settings.show_normal = 1 - material.settings.show_normal;
            if material.settings.show_normal != 0 {
                material.settings.show_albedo = 0;
            }
            println!(
                "Normal map overlay: {}",
                if material.settings.show_normal != 0 {
                    "on"
                } else {
                    "off"
                }
            );
        }
    }
}

fn dynamic_scene(
    mut suns: Query<&mut Transform, With<DirectionalLight>>,
    time: Res<Time>,
    sun_motion_state: Res<GameState>,
) {
    if !sun_motion_state.paused {
        suns.iter_mut()
            .for_each(|mut tf| tf.rotate_x(-time.delta_secs() * PI / 10.0));
    }
}
