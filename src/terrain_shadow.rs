//! RDR2-style terrain shadow map.
//!
//! A small top-down texture centered on the view is regenerated each frame by
//! raymarching the terrain height map towards the sun. Each texel stores the
//! elevation below which the terrain is occluded (R) and the ray length to the
//! occluder (G), which widens the penumbra for cheap soft shadows.
//!
//! The raymarch only reruns when its result can change: when the map moves, the
//! settings change, the sun turns past [`TerrainShadowSettings::light_update_threshold`],
//! or a tile at or coarser than the sampled lod enters or leaves the atlas.

use crate::{
    math::Coordinate,
    terrain_data::{AttachmentLabel, TerrainTileDropped, TerrainTileReady, TileAtlas, TileTree},
    terrain_view::TerrainViewComponents,
};
use bevy::{
    math::{DMat4, DVec3, DVec4},
    platform::collections::HashMap,
    prelude::*,
    render::{Extract, render_resource::ShaderType},
};

/// The angular radius of the sun as seen from earth, in radians (~0.265 degrees).
const SUN_ANGULAR_RADIUS: f32 = 0.004625;

/// Frames to keep regenerating the map after a tile enters or leaves the atlas.
/// The tile tree buffer follows on the same frame, but the atlas texture upload
/// lags one render frame behind.
const TILE_CHANGE_REFRESH_FRAMES: u32 = 3;

/// Raymarched heightfield shadow. Off until `enabled` is set.
///
/// [`TerrainPlugin`](crate::plugin::TerrainPlugin) inserts the default. Other materials sample
/// the same map; see `terrain_shadow_factor_at` in `bevy_terrain::shaders::terrain_shadow`.
#[derive(Resource, Clone)]
pub struct TerrainShadowSettings {
    pub enabled: bool,
    /// The resolution of the (square) shadow map texture.
    /// Fixed after startup.
    pub map_size: u32,
    /// Half size of the region covered by the shadow map around the view, in meters.
    pub extent: f32,
    /// The maximum horizon scan distance of the raymarch, in meters.
    pub max_distance: f32,
    /// The number of raymarch steps per texel.
    pub steps: u32,
    /// Multiplier on the penumbra width derived from the occluder distance.
    pub softness: f32,
    /// The minimum penumbra width, in meters. Hides the quantization of the
    /// 16-bit shadow map at hard shadow edges.
    pub min_penumbra: f32,
    /// The angle, in radians, the light has to turn before the map is regenerated.
    /// Smaller sun movements reuse the previous map.
    pub light_update_threshold: f32,
}

impl Default for TerrainShadowSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            map_size: 512,
            extent: 25_000.0,
            max_distance: 100_000.0,
            steps: 96,
            softness: 1.0,
            min_penumbra: 10.0,
            // ~0.02 degrees, well below the sun's angular radius.
            light_update_threshold: 0.00035,
        }
    }
}

/// GPU parameters of the terrain shadow map.
/// Must match the `TerrainShadow` struct in `types.wesl`.
#[derive(Clone, Default, PartialEq, ShaderType)]
pub struct TerrainShadowUniform {
    unit_from_world: Mat4,
    center: Vec3,
    extent: f32,
    east: Vec3,
    texel_size: f32,
    north: Vec3,
    curvature: f32,
    up: Vec3,
    max_distance: f32,
    light_direction: Vec3,
    penumbra_scale: f32,
    min_penumbra: f32,
    map_size: f32,
    steps: u32,
    pub(crate) enabled: u32,
    lod: u32,
}

/// Tracks whether the shadow map of a (terrain, view) has to be regenerated.
#[derive(Clone, Default)]
pub(crate) struct TerrainShadowRefresh {
    /// The uniform of the last regeneration request.
    last: TerrainShadowUniform,
    /// The number of upcoming frames (including this one) that regenerate the map.
    pub(crate) frames: u32,
}

/// Computes the per-(terrain, view) shadow map parameters: the tangent frame of the
/// map region (snapped to the texel grid to avoid shimmer) and the sun direction.
pub(crate) fn update_terrain_shadow(
    settings: Res<TerrainShadowSettings>,
    tile_trees: Res<TerrainViewComponents<TileTree>>,
    tile_atlases: Query<&TileAtlas>,
    lights: Query<(&DirectionalLight, &GlobalTransform)>,
    mut uniforms: ResMut<TerrainViewComponents<TerrainShadowUniform>>,
    mut refreshes: ResMut<TerrainViewComponents<TerrainShadowRefresh>>,
    mut tile_ready: MessageReader<TerrainTileReady>,
    mut tile_dropped: MessageReader<TerrainTileDropped>,
) {
    refreshes.retain(|key, _| tile_trees.contains_key(key));

    // Coarsest changed tile lod per terrain. Finer tiles never affect the map,
    // since the raymarch samples a fixed lod.
    let mut changed_lods = HashMap::<Entity, u32>::new();
    for (terrain, lod) in tile_ready
        .read()
        .map(|message| (message.terrain, message.coordinate.lod))
        .chain(
            tile_dropped
                .read()
                .map(|message| (message.terrain, message.coordinate.lod)),
        )
    {
        changed_lods
            .entry(terrain)
            .and_modify(|min| *min = (*min).min(lod))
            .or_insert(lod);
    }

    for (&(terrain, view), tile_tree) in tile_trees.iter() {
        let Ok(tile_atlas) = tile_atlases.get(terrain) else {
            continue;
        };

        let shape = tile_tree.shape;
        let spherical = shape.is_spherical();
        let view_local = tile_tree.view_local_position;
        let view_world = tile_tree.view_world_position.as_dvec3();

        let texel_size = 2.0 * settings.extent as f64 / settings.map_size as f64;

        // Project the view onto the terrain surface (height 0).
        let coordinate = Coordinate::from_local_position(view_local, shape);
        let mut unit = coordinate.unit_position(spherical);

        if spherical {
            // Snap the surface direction to a fixed angular grid with one texel per
            // step. The tangent frame is derived from the snapped direction, so both
            // the frame and the center stay put while the camera moves within a texel.
            // (Snapping the center along east/north would not be stable, since those
            // axes themselves rotate with the camera.)
            let angular_texel = texel_size / shape.scale_scalar();
            let latitude = (unit.y.asin() / angular_texel).round() * angular_texel;
            let longitude_step = angular_texel / latitude.cos().max(1e-3);
            let longitude = (unit.z.atan2(unit.x) / longitude_step).round() * longitude_step;
            unit = DVec3::new(
                latitude.cos() * longitude.cos(),
                latitude.sin(),
                latitude.cos() * longitude.sin(),
            );
        }

        let mut center_local = shape.position_unit_to_local(unit, 0.0);

        // Local tangent frame at the (snapped) map center.
        let up = if spherical {
            (shape.scale() * unit).normalize()
        } else {
            DVec3::Y
        };
        let mut east = DVec3::Y.cross(up);
        if east.length_squared() < 1e-8 {
            east = DVec3::X;
        }
        let east = east.normalize();
        let north = up.cross(east);

        if !spherical {
            // Planar terrain: east/north are fixed axes, so snapping the center works.
            let snap = |value: f64| (value / texel_size).round() * texel_size - value;
            center_local +=
                snap(center_local.dot(east)) * east + snap(center_local.dot(north)) * north;
        }

        // The raymarch samples heights with a single fixed tile tree lod: fine enough
        // that one height texel is no larger than one shadow map texel, but coarse
        // enough that the whole scan range stays inside the (viewer-centered) tile
        // tree window at that lod, where the wrapping tree lookup is valid.
        let face_size = shape.face_size();
        let height_texture_size = tile_atlas
            .attachments
            .get(&AttachmentLabel::Height)
            .map_or(512.0, |attachment| attachment.center_size as f64);
        let detail_lod = (face_size / (texel_size * height_texture_size))
            .log2()
            .ceil();
        let reach = (settings.extent + settings.max_distance) as f64;
        // Margin of one tile plus 25% for the uv distortion of the cube sphere.
        let window_tiles = 0.75 * (tile_tree.tree_size as f64 / 2.0 - 1.0);
        let window_lod = (window_tiles * face_size / reach).log2().floor();
        let lod = detail_lod
            .min(window_lod)
            .clamp(0.0, (tile_tree.lod_count - 1) as f64) as u32;

        // The terrain local space (meters, planet-centered) and the world space differ
        // by a pure translation, which is recovered from the view's position in both.
        let center_world = (view_world + (center_local - view_local)).as_vec3();

        let inverse_scale = 1.0 / shape.scale();
        let translation = (view_local - view_world) * inverse_scale;
        let unit_from_world = DMat4::from_cols(
            DVec4::new(inverse_scale.x, 0.0, 0.0, 0.0),
            DVec4::new(0.0, inverse_scale.y, 0.0, 0.0),
            DVec4::new(0.0, 0.0, inverse_scale.z, 0.0),
            DVec4::new(translation.x, translation.y, translation.z, 1.0),
        )
        .as_mat4();

        let curvature = if spherical {
            (0.5 / shape.scale_scalar()) as f32
        } else {
            0.0
        };

        // March towards the brightest light above the map's horizon, else the
        // brightest overall. A set sun can outshine the moon while it still
        // lights the twilight sky, but it no longer lights the terrain.
        let up_world = up.as_vec3();
        let light_direction = lights
            .iter()
            .map(|(light, transform)| (transform.back().as_vec3(), light.illuminance))
            .max_by(|(a, a_lux), (b, b_lux)| {
                (a.dot(up_world) > 0.0)
                    .cmp(&(b.dot(up_world) > 0.0))
                    .then(a_lux.total_cmp(b_lux))
            })
            .map(|(direction, _)| direction);

        let uniform = TerrainShadowUniform {
            unit_from_world,
            center: center_world,
            extent: settings.extent,
            east: east.as_vec3(),
            texel_size: texel_size as f32,
            north: north.as_vec3(),
            curvature,
            up: up_world,
            max_distance: settings.max_distance,
            light_direction: light_direction.unwrap_or(Vec3::Y),
            penumbra_scale: SUN_ANGULAR_RADIUS.tan() * settings.softness,
            min_penumbra: settings.min_penumbra,
            map_size: settings.map_size as f32,
            steps: settings.steps,
            enabled: (settings.enabled && light_direction.is_some()) as u32,
            lod,
        };

        // A new entry differs from any real uniform, so the first frame regenerates.
        let refresh = refreshes.entry((terrain, view)).or_default();
        refresh.frames = refresh.frames.saturating_sub(1);

        if changed_lods
            .get(&terrain)
            .is_some_and(|&changed| changed <= lod)
        {
            refresh.frames = TILE_CHANGE_REFRESH_FRAMES;
        }

        // The cross product resolves small angles that the dot product loses to f32
        // rounding near 1.
        let (light, last_light) = (uniform.light_direction, refresh.last.light_direction);
        let light_moved = light.dot(last_light) <= 0.0
            || light.cross(last_light).length() > settings.light_update_threshold.sin();
        let frame_changed = TerrainShadowUniform {
            light_direction: refresh.last.light_direction,
            ..uniform.clone()
        } != refresh.last;

        if light_moved || frame_changed {
            refresh.frames = refresh.frames.max(1);
            refresh.last = uniform.clone();
        }

        uniforms.insert((terrain, view), uniform);
    }
}

pub(crate) fn extract_terrain_shadow(
    mut settings: ResMut<TerrainShadowSettings>,
    mut uniforms: ResMut<TerrainViewComponents<TerrainShadowUniform>>,
    mut refreshes: ResMut<TerrainViewComponents<TerrainShadowRefresh>>,
    main_settings: Extract<Res<TerrainShadowSettings>>,
    main_uniforms: Extract<Res<TerrainViewComponents<TerrainShadowUniform>>>,
    main_refreshes: Extract<Res<TerrainViewComponents<TerrainShadowRefresh>>>,
) {
    *settings = main_settings.clone();

    for (&key, uniform) in main_uniforms.iter() {
        uniforms.insert(key, uniform.clone());
    }
    for (&key, refresh) in main_refreshes.iter() {
        refreshes.insert(key, refresh.clone());
    }
}
