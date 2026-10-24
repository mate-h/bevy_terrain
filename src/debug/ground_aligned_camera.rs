use crate::math::TerrainShape;
use bevy::{
    app::{RunFixedMainLoop, RunFixedMainLoopSystems},
    camera_controller::free_camera::{
        rotate_freecam_to, FreeCamera, FreeCameraState, VerticalMovementAxis,
    },
    prelude::*,
    time::{Real, Time},
};

#[cfg(feature = "big_space")]
use crate::floating_origin::view_local_position;
#[cfg(feature = "big_space")]
use big_space::prelude::{CellCoord, Grids};

/// Makes [`FreeCamera`] yaw around the terrain surface up instead of world +Y.
///
/// Bevy's free camera stores pitch/yaw as world-Y eulers. On a spheroid that
/// axis is not the ground, so mouse look and Q/E drift off the horizon.
/// This remaps those eulers into the local ground frame after the free camera
/// runs.
#[derive(Component, Clone, Copy)]
pub struct GroundAlignedFreeCamera {
    pub shape: TerrainShape,
    aligned: bool,
}

impl GroundAlignedFreeCamera {
    pub fn new(shape: TerrainShape) -> Self {
        Self {
            shape,
            aligned: false,
        }
    }
}

pub struct GroundAlignedFreeCameraPlugin;

impl Plugin for GroundAlignedFreeCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            RunFixedMainLoop,
            align_free_camera_to_ground
                .after(rotate_freecam_to)
                .in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
        );
    }
}

fn ground_up(shape: TerrainShape, world_position: bevy::math::DVec3) -> Vec3 {
    match shape {
        TerrainShape::Plane { .. } => Vec3::Y,
        TerrainShape::Sphere { .. } | TerrainShape::Spheroid { .. } => {
            let unit = shape.position_local_to_unit(world_position);
            (shape.scale() * unit).normalize().as_vec3()
        }
    }
}

fn ground_from_y(up: Vec3) -> Quat {
    let up = up.normalize_or(Vec3::Y);
    Quat::from_rotation_arc(Vec3::Y, up)
}

#[cfg(feature = "big_space")]
fn camera_world_position(
    grids: &Grids,
    entity: Entity,
    transform: &Transform,
    cell: Option<&CellCoord>,
) -> bevy::math::DVec3 {
    cell.map(|cell| view_local_position(grids, entity, transform, cell))
        .unwrap_or_else(|| transform.translation.as_dvec3())
}

#[cfg(not(feature = "big_space"))]
fn camera_world_position(
    _entity: Entity,
    transform: &Transform,
    global: Option<&GlobalTransform>,
) -> bevy::math::DVec3 {
    global
        .map(|g| g.translation().as_dvec3())
        .unwrap_or_else(|| transform.translation.as_dvec3())
}

fn apply_ground_alignment(
    dt: f32,
    world_position: bevy::math::DVec3,
    transform: &mut Transform,
    state: &mut FreeCameraState,
    config: &FreeCamera,
    ground: &mut GroundAlignedFreeCamera,
) {
    let up = ground_up(ground.shape, world_position);
    let ground_rot = ground_from_y(up);

    if !ground.aligned {
        let local = ground_rot.inverse() * transform.rotation;
        let (yaw, pitch, _) = local.to_euler(EulerRot::YXZ);
        state.yaw = yaw;
        state.pitch = pitch.clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2);
        ground.aligned = true;
    }

    if matches!(config.vertical_movement_axis, VerticalMovementAxis::World)
        && state.velocity.y != 0.0
    {
        transform.translation += state.velocity.y * dt * (up - Vec3::Y);
    }

    transform.rotation = ground_rot * Quat::from_euler(EulerRot::ZYX, 0.0, state.yaw, state.pitch);
}

#[cfg(feature = "big_space")]
fn align_free_camera_to_ground(
    time: Res<Time<Real>>,
    grids: Grids,
    mut cameras: Query<(
        Entity,
        &mut Transform,
        &mut FreeCameraState,
        &FreeCamera,
        &mut GroundAlignedFreeCamera,
        Option<&CellCoord>,
    )>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, mut state, config, mut ground, cell) in &mut cameras {
        let world_position = camera_world_position(&grids, entity, &transform, cell);
        apply_ground_alignment(
            dt,
            world_position,
            &mut transform,
            &mut state,
            config,
            &mut ground,
        );
    }
}

#[cfg(not(feature = "big_space"))]
fn align_free_camera_to_ground(
    time: Res<Time<Real>>,
    mut cameras: Query<(
        Entity,
        &mut Transform,
        &mut FreeCameraState,
        &FreeCamera,
        &mut GroundAlignedFreeCamera,
        Option<&GlobalTransform>,
    )>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, mut state, config, mut ground, global) in &mut cameras {
        let world_position = camera_world_position(entity, &transform, global);
        apply_ground_alignment(
            dt,
            world_position,
            &mut transform,
            &mut state,
            config,
            &mut ground,
        );
    }
}
