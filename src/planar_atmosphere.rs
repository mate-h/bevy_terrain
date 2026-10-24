//! Runtime alignment of a planetary atmosphere shell over flat [`TerrainShape::Plane`] terrain.
//!
//! Spherical atmosphere rendering expects a planet center in world space. On a flat map the
//! center must sit below the camera so the inner sphere is tangent to the terrain. When the
//! camera moves horizontally, the center should move with it so the horizon stays level (+Y up).

#[cfg(feature = "big_space")]
use crate::floating_origin::view_local_position;
use crate::math::TerrainShape;
use bevy::{light::Atmosphere, prelude::*, transform::TransformSystems};
#[cfg(feature = "big_space")]
use big_space::prelude::{CellCoord, Grid, Grids};

/// Configuration for a planetary atmosphere over flat terrain.
#[derive(Resource, Clone, Copy, Debug)]
pub struct PlanarAtmosphereSettings {
    pub inner_radius: f32,
    pub outer_radius: f32,
    pub surface_height: f32,
    /// When true, slide the planet center horizontally with the camera each frame.
    pub follow_camera: bool,
}

impl PlanarAtmosphereSettings {
    pub fn new(
        surface_height: f32,
        reference_minor_axis: f64,
        atmosphere_shell: f32,
        follow_camera: bool,
    ) -> Self {
        let (inner_radius, outer_radius, _) = TerrainShape::planar_atmosphere_alignment(
            surface_height,
            reference_minor_axis,
            atmosphere_shell,
        );
        Self {
            inner_radius,
            outer_radius,
            surface_height,
            follow_camera,
        }
    }

    /// Initial planet-center transform at the map origin (non-`big_space` spawn).
    pub fn initial_transform(&self) -> Transform {
        Transform::from_translation(self.planet_center(Vec2::ZERO))
    }

    /// Planet center in world space for the given camera horizontal position.
    pub fn planet_center(&self, camera_horizontal: Vec2) -> Vec3 {
        let horizontal = if self.follow_camera {
            camera_horizontal
        } else {
            Vec2::ZERO
        };
        TerrainShape::planar_atmosphere_center(self.surface_height, self.inner_radius, horizontal)
    }

    /// Initial `(CellCoord, Transform)` for an atmosphere entity under `big_space`.
    #[cfg(feature = "big_space")]
    pub fn initial_big_space_position(&self, grid: &Grid) -> (CellCoord, Vec3) {
        if self.follow_camera {
            (CellCoord::default(), Vec3::ZERO)
        } else {
            grid.imprecise_translation_to_grid(self.planet_center(Vec2::ZERO))
        }
    }
}

/// Keeps the atmosphere sphere tangent to flat terrain under the camera.
pub struct PlanarAtmospherePlugin;

impl Plugin for PlanarAtmospherePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            align_planar_atmosphere
                .before(TransformSystems::Propagate)
                .run_if(|settings: Res<PlanarAtmosphereSettings>| settings.follow_camera),
        );
    }
}

#[cfg(not(feature = "big_space"))]
fn align_planar_atmosphere(
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut atmosphere: Query<&mut Transform, With<Atmosphere>>,
    settings: Res<PlanarAtmosphereSettings>,
) {
    let Ok(camera) = camera.single() else {
        return;
    };

    let center = settings.planet_center(camera.translation().xz());

    for mut transform in atmosphere.iter_mut() {
        transform.translation = center;
    }
}

#[cfg(feature = "big_space")]
fn align_planar_atmosphere(
    camera: Query<(Entity, &Transform, &CellCoord), (With<Camera3d>, Without<Atmosphere>)>,
    grids: Grids,
    mut atmosphere: Query<
        (&mut CellCoord, &mut Transform, &ChildOf),
        (With<Atmosphere>, Without<Camera3d>),
    >,
    settings: Res<PlanarAtmosphereSettings>,
) {
    let Ok((view, camera_transform, camera_cell)) = camera.single() else {
        return;
    };

    let camera_world = view_local_position(&grids, view, camera_transform, camera_cell);
    let center = settings
        .planet_center(Vec2::new(camera_world.x as f32, camera_world.z as f32))
        .as_dvec3();

    for (mut cell, mut transform, parent) in atmosphere.iter_mut() {
        let grid = grids.get(parent.parent());
        let (grid_cell, local_translation) = grid.translation_to_grid(center);
        *cell = grid_cell;
        transform.translation = local_translation;
    }
}
