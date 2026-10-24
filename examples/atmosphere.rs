//! Atmosphere example with California terrain, based on `bevy/examples/3d/atmosphere.rs`.
//!
//! Run from this crate root:
//! `cargo run --example atmosphere --features atmosphere_example`

use bevy::{
    anti_alias::taa::TemporalAntiAliasing,
    asset::io::AssetSourceBuilder,
    camera::Exposure,
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin},
    core_pipeline::prepass::{DepthPrepass, MotionVectorPrepass},
    input::keyboard::KeyCode,
    light::{
        atmosphere::ScatteringMedium, light_consts::lux, Atmosphere, AtmosphereEnvironmentMapLight,
        CascadeShadowConfigBuilder, SunDisk, VolumetricLight,
    },
    math::{DVec2, DVec3},
    pbr::{AtmosphereMode, AtmosphereSettings},
    post_process::bloom::Bloom,
    prelude::*,
    reflect::TypePath,
    render::{
        camera::{MipBias, TemporalJitter},
        render_resource::{AsBindGroup, ShaderType},
    },
    shader::ShaderRef,
};
use bevy_terrain::{math::Coordinate, prelude::*};
use big_space::prelude::Grid;
use std::f32::consts::PI;

const EARTH_MINOR_RADIUS: f32 = 6356752.314245;
const EARTH_ATMOSPHERE_SHELL: f32 = 100_000.0;

/// Lowest elevation in `source_data/california.tif` (near Death Valley).
const CALIFORNIA_MIN_HEIGHT: f32 = -8.381_328_6;
const CALIFORNIA_MIN_LAT: f64 = 36.637_500;
const CALIFORNIA_MIN_LON: f64 = -117.159_259;
/// Highest elevation in the preprocessed california height tiles.
const CALIFORNIA_MAX_HEIGHT: f32 = 4416.048_340;

/// California tiles in `config.tc.ron` live on cube face 4 (−Z normal) at lod 11.
const CALIFORNIA_FACE: u32 = 4;
const CALIFORNIA_LOD: u32 = 11;
const CALIFORNIA_TILE_XY: IVec2 = IVec2::new(91, 1695);
const CALIFORNIA_MEAN_HEIGHT: f32 = (CALIFORNIA_MIN_HEIGHT + CALIFORNIA_MAX_HEIGHT) * 0.5;
/// Lod-11 load radius is ~29 km; stay within that so tiles stream in.
const CALIFORNIA_CAMERA_ALTITUDE: f64 = 15_000.0;

/// Shadow-test spheres: grid over the California tile center.
const SHADOW_SPHERE_RADIUS: f32 = 80.0;
const SHADOW_SPHERE_HEIGHT: f64 = 500.0;
const SHADOW_SPHERE_SPACING: f64 = 500.0;
const SHADOW_SPHERE_GRID: i32 = 5;

type TaaComponents = (
    TemporalAntiAliasing,
    TemporalJitter,
    MipBias,
    DepthPrepass,
    MotionVectorPrepass,
);

/// Matches the lon/lat convention in `preprocess/src/transformers.rs`.
fn lat_lon_to_unit_position(lat_deg: f64, lon_deg: f64) -> DVec3 {
    let lat = lat_deg.to_radians();
    let lon = lon_deg.to_radians();
    DVec3::new(-lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin())
}

/// Geocentric distance from the planet center at a surface point with elevation `height`.
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

fn california_atmosphere_radii() -> (f32, f32) {
    let inner_radius = geocentric_radius_at_elevation(
        TerrainShape::WGS84,
        CALIFORNIA_MIN_LAT,
        CALIFORNIA_MIN_LON,
        CALIFORNIA_MIN_HEIGHT,
    );
    (inner_radius, inner_radius + EARTH_ATMOSPHERE_SHELL)
}

fn california_coordinate() -> Coordinate {
    let tile_count = 2f64.powi(CALIFORNIA_LOD as i32);
    let uv = (CALIFORNIA_TILE_XY.as_dvec2() + DVec2::splat(0.5)) / tile_count;
    Coordinate::new(CALIFORNIA_FACE, uv)
}

fn california_surface_normal() -> Dir3 {
    let unit = california_coordinate().unit_position(true);
    Dir3::new_unchecked((TerrainShape::WGS84.scale() * unit).normalize().as_vec3())
}

fn california_surface_point() -> DVec3 {
    california_coordinate().local_position(TerrainShape::WGS84, CALIFORNIA_MEAN_HEIGHT)
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

#[derive(Clone, Copy, Default, ShaderType)]
struct CaliforniaMaterialSettings {
    show_landcover: u32,
    show_albedo: u32,
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Default)]
struct CaliforniaMaterial {
    #[uniform(0)]
    settings: CaliforniaMaterialSettings,
}

impl Material for CaliforniaMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/atmosphere.wesl".into()
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
        .insert_resource(TerrainSettings::new(vec!["landcover", "albedo"]))
        .insert_resource(TerrainShadowSettings {
            enabled: true,
            ..default()
        })
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            FreeCameraPlugin,
            GroundAlignedFreeCameraPlugin,
            TerrainPlugin,
            TerrainMaterialPlugin::<CaliforniaMaterial>::default(),
        ))
        .add_systems(Startup, (setup_scene, print_controls))
        .add_systems(
            Update,
            (
                dynamic_scene,
                atmosphere_controls,
                terrain_overlay_controls,
                antialias_controls,
            ),
        )
        .run();
}

fn print_controls() {
    println!("Atmosphere + California Terrain Controls:");
    println!("    1          - Switch to lookup texture rendering method");
    println!("    2          - Switch to raymarched rendering method");
    println!("    3          - Switch to Earth atmosphere");
    println!("    4          - Switch to Mars atmosphere");
    println!("    Enter      - Pause/Resume sun motion");
    println!("    T          - Toggle terrain shadows on/off");
    println!("    L          - Toggle WorldCover landcover overlay on/off");
    println!("    I          - Toggle satellite albedo overlay on/off");
    println!("    M          - Toggle MSAA 4x / TAA (MSAA off)");
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
        let (inner_radius, outer_radius) = california_atmosphere_radii();
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

fn antialias_controls(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut camera: Query<(Entity, Option<&mut Msaa>, Option<&TemporalAntiAliasing>), With<Camera3d>>,
    mut commands: Commands,
) {
    if !keyboard_input.just_pressed(KeyCode::KeyM) {
        return;
    }

    let Ok((entity, msaa, taa)) = camera.single_mut() else {
        return;
    };

    if taa.is_some() {
        if let Some(mut msaa) = msaa {
            *msaa = Msaa::Sample4;
        } else {
            commands.entity(entity).insert(Msaa::Sample4);
        }
        commands.entity(entity).remove::<TaaComponents>();
        println!("Anti-aliasing: MSAA 4x");
    } else {
        if let Some(mut msaa) = msaa {
            *msaa = Msaa::Off;
        } else {
            commands.entity(entity).insert(Msaa::Off);
        }
        commands
            .entity(entity)
            .insert(TemporalAntiAliasing::default());
        println!("Anti-aliasing: TAA");
    }
}

fn setup_scene(
    mut commands: Commands,
    mut scattering_mediums: ResMut<Assets<ScatteringMedium>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
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
    let (inner_radius, outer_radius) = california_atmosphere_radii();
    atmosphere.inner_radius = inner_radius;
    atmosphere.outer_radius = outer_radius;
    println!(
        "California terrain bounds from source_data/california.tif: \
         min={CALIFORNIA_MIN_HEIGHT:.3} m at ({CALIFORNIA_MIN_LAT:.6}°, {CALIFORNIA_MIN_LON:.6}°), \
         max={CALIFORNIA_MAX_HEIGHT:.3} m"
    );
    println!(
        "Atmosphere inner radius at lowest terrain point: {inner_radius:.3} m \
         (WGS84 minor axis reference: {EARTH_MINOR_RADIUS:.3} m)"
    );

    // Default cascade max distance is 150 m — far too short for planetary scale.
    let cascade_shadow_config = CascadeShadowConfigBuilder {
        maximum_distance: 20_000.0,
        first_cascade_far_bound: 500.0,
        ..default()
    }
    .build();

    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            contact_shadows_enabled: true,
            illuminance: lux::RAW_SUNLIGHT,
            ..default()
        },
        Transform::from_xyz(1.0, 0.4, 0.0).looking_at(Vec3::ZERO, Vec3::Y),
        cascade_shadow_config,
        VolumetricLight,
        SunDisk::EARTH,
    ));

    let sphere_mesh = meshes.add(Sphere::new(SHADOW_SPHERE_RADIUS).mesh().ico(4).unwrap());
    let sphere_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.85, 0.35, 0.2),
        perceptual_roughness: 0.8,
        ..default()
    });

    let mut view = Entity::PLACEHOLDER;

    commands.spawn_big_space(Grid::default(), |root| {
        root.spawn((
            Transform::IDENTITY,
            CellCoord::default(),
            GlobalTransform::from_translation(Vec3::Z),
            atmosphere,
        ));

        let surface = california_surface_point();
        let up = california_surface_normal();
        let camera_position = surface + up.as_vec3().as_dvec3() * CALIFORNIA_CAMERA_ALTITUDE;
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
                GroundAlignedFreeCamera::new(TerrainShape::WGS84),
                FloatingOrigin,
                AtmosphereEnvironmentMapLight::default(),
            ))
            .id();

        // Grid of spheres hovering above the terrain to test mesh → terrain shadows.
        let up_d = up.as_vec3().as_dvec3();
        let mut east = DVec3::Y.cross(up_d);
        if east.length_squared() < 1e-8 {
            east = DVec3::X;
        }
        let east = east.normalize();
        let north = up_d.cross(east).normalize();
        let half = (SHADOW_SPHERE_GRID - 1) as f64 * 0.5;

        for ix in 0..SHADOW_SPHERE_GRID {
            for iz in 0..SHADOW_SPHERE_GRID {
                let offset = east * ((ix as f64 - half) * SHADOW_SPHERE_SPACING)
                    + north * ((iz as f64 - half) * SHADOW_SPHERE_SPACING);
                let world = surface + offset + up_d * SHADOW_SPHERE_HEIGHT;
                let (sphere_cell, sphere_local) =
                    grid.imprecise_translation_to_grid(world.as_vec3());
                root.spawn_spatial((
                    sphere_cell,
                    Transform::from_translation(sphere_local),
                    Mesh3d(sphere_mesh.clone()),
                    MeshMaterial3d(sphere_material.clone()),
                ));
            }
        }
    });

    commands.spawn_terrain(
        asset_server.load("terrains/california/config.tc.ron"),
        TerrainViewConfig {
            view_lod: CALIFORNIA_LOD,
            ..default()
        },
        CaliforniaMaterial::default(),
        view,
    );
}

fn terrain_overlay_controls(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut materials: ResMut<Assets<CaliforniaMaterial>>,
    terrain_materials: Query<&MeshMaterial3d<CaliforniaMaterial>, With<TileAtlas>>,
) {
    let toggle_landcover = keyboard_input.just_pressed(KeyCode::KeyL);
    let toggle_albedo = keyboard_input.just_pressed(KeyCode::KeyI);

    if !toggle_landcover && !toggle_albedo {
        return;
    }

    for terrain_material in &terrain_materials {
        let Some(mut material) = materials.get_mut(terrain_material.id()) else {
            continue;
        };

        if toggle_landcover {
            material.settings.show_landcover = 1 - material.settings.show_landcover;
            println!(
                "WorldCover landcover overlay: {}",
                if material.settings.show_landcover != 0 {
                    "on"
                } else {
                    "off"
                }
            );
        }

        if toggle_albedo {
            material.settings.show_albedo = 1 - material.settings.show_albedo;
            println!(
                "Satellite albedo overlay: {}",
                if material.settings.show_albedo != 0 {
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
