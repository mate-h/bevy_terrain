use bevy::camera::Hdr;
use bevy::light::{Atmosphere, atmosphere::ScatteringMedium};
use bevy::pbr::{AtmosphereMode, AtmosphereSettings};
use bevy::shader::ShaderRef;
use bevy::window::WindowResolution;
use bevy::{prelude::*, reflect::TypePath, render::render_resource::*};
use bevy_terrain::prelude::*;

/// Matches `TerrainShape::WGS84` used by the preprocessed earth dataset.
const EARTH_MAJOR_RADIUS: f32 = 6378137.0;
const EARTH_MINOR_RADIUS: f32 = 6356752.314245;
const EARTH_ATMOSPHERE_SHELL: f32 = 100_000.0;

#[derive(ShaderType, Clone)]
struct GradientInfo {
    mode: u32,
}

#[derive(Asset, AsBindGroup, TypePath, Clone)]
pub struct CustomMaterial {
    #[texture(0)]
    #[sampler(1)]
    gradient: Handle<Image>,
    #[uniform(2)]
    gradient_info: GradientInfo,
}

impl Material for CustomMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/spherical.wesl".into()
    }
}

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        resolution: WindowResolution::new(1920, 1080),
                        ..default()
                    }),
                    ..default()
                })
                .build()
                .disable::<TransformPlugin>(),
            TerrainPlugin,
            TerrainMaterialPlugin::<CustomMaterial>::default(),
            TerrainDebugPlugin, // enable debug settings and controls
            TerrainPickingPlugin,
        ))
        .insert_resource(TerrainSettings::new(vec!["albedo", "landcover"]))
        .insert_resource(GlobalAmbientLight::NONE)
        .insert_resource(ClearColor(Color::BLACK))
        .add_systems(Startup, initialize)
        .run();
}

#[allow(clippy::too_many_arguments)]
fn initialize(
    mut commands: Commands,
    mut images: ResMut<LoadingImages>,
    mut scattering_mediums: ResMut<Assets<ScatteringMedium>>,
    asset_server: Res<AssetServer>,
) {
    let gradient1 = asset_server.load("textures/gradient1.png");
    images.load_image(
        &gradient1,
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
    );

    let gradient2 = asset_server.load("textures/gradient2.png");
    images.load_image(
        &gradient2,
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
    );

    let earth_medium = scattering_mediums.add(ScatteringMedium::earth(256, 256));
    let mut atmosphere = Atmosphere::earth(earth_medium);
    atmosphere.inner_radius = EARTH_MINOR_RADIUS;
    atmosphere.outer_radius = EARTH_MINOR_RADIUS + EARTH_ATMOSPHERE_SHELL;

    let mut view = Entity::PLACEHOLDER;

    commands.spawn_big_space(Grid::default(), |root| {
        // Planet center at big_space origin. `CellCoord::default()` lets propagation keep
        // `GlobalTransform` in sync when the floating origin recenters.
        let atmosphere_entity = root
            .spawn((
                Transform::IDENTITY,
                CellCoord::default(),
                GlobalTransform::from_translation(Vec3::Z),
            ))
            .id();
        root.commands().entity(atmosphere_entity).insert(atmosphere);

        view = root
            .spawn_spatial((
                Transform::from_translation(-Vec3::X * EARTH_MAJOR_RADIUS * 3.0)
                    .looking_to(Vec3::X, Vec3::Y),
                Camera3d::default(),
                Hdr,
                AtmosphereSettings {
                    rendering_method: AtmosphereMode::Raymarched,
                    ..default()
                },
                DebugCameraController::new(EARTH_MAJOR_RADIUS as f64),
                OrbitalCameraController::default(),
            ))
            .id();
    });

    commands.spawn_terrain(
        asset_server.load("terrains/earth/config.tc.ron"),
        TerrainViewConfig::default(),
        CustomMaterial {
            gradient: gradient1.clone(),
            gradient_info: GradientInfo { mode: 2 },
        },
        view,
    );

    // commands.spawn_terrain(
    //     asset_server.load("terrains/los/config.tc.ron"),
    //     TerrainViewConfig {
    //         order: 1,
    //         ..default()
    //     },
    //     CustomMaterial {
    //         gradient: gradient2.clone(),
    //         gradient_info: GradientInfo { mode: 0 },
    //     },
    //     view,
    // );
    // //
    // commands.spawn_terrain(
    //     asset_server.load("terrains/npd/config.tc.ron"),
    //     TerrainViewConfig {
    //         order: 2,
    //         ..default()
    //     },
    //     CustomMaterial {
    //         gradient: gradient2.clone(),
    //         gradient_info: GradientInfo { mode: 0 },
    //     },
    //     view,
    // );
    //
    // commands.spawn_terrain(
    //     asset_server.load("terrains/utsira/config.tc.ron"),
    //     TerrainViewConfig {
    //         order: 1,
    //         ..default()
    //     },
    //     CustomMaterial {
    //         gradient: gradient2.clone(),
    //         gradient_info: GradientInfo { mode: 0 },
    //     },
    //     view,
    // );
    //
    // commands.spawn_terrain(
    //     asset_server.load("terrains/sas/config.tc.ron"),
    //     TerrainViewConfig {
    //         order: 2,
    //         ..default()
    //     },
    //     CustomMaterial {
    //         gradient: gradient2.clone(),
    //         gradient_info: GradientInfo { mode: 3 },
    //     },
    //     view,
    // );
    //
    commands.spawn_terrain(
        asset_server.load("terrains/california/config.tc.ron"),
        TerrainViewConfig {
            order: 1,
            ..default()
        },
        CustomMaterial {
            gradient: gradient1.clone(),
            gradient_info: GradientInfo { mode: 1 },
        },
        view,
    );
    //
    // commands.spawn_terrain(
    //     asset_server.load("terrains/hartenstein/config.tc.ron"),
    //     TerrainViewConfig {
    //         order: 1,
    //         ..default()
    //     },
    //     CustomMaterial {
    //         gradient: gradient2.clone(),
    //         gradient_info: GradientInfo { mode: 2 },
    //     },
    //     view,
    // );
}
