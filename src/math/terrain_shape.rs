use crate::math::spheroid::project_point_spheroid;
use bevy::{math::DVec3, prelude::*};
use serde::{Deserialize, Serialize};

/// World shape stored on [`TerrainConfig`](crate::terrain::TerrainConfig).
///
/// A plane is one face. A sphere or spheroid is a six-face cube-sphere.
/// [`TerrainShape::WGS84`] is the Earth ellipsoid the GeoTIFF preprocess path writes.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum TerrainShape {
    /// Flat map. `side_length` is the full width and depth. UV `(0.5, 0.5)` is the center.
    Plane { side_length: f64 },
    /// Cube-sphere with this radius, in meters.
    Sphere { radius: f64 },
    /// Ellipsoid. `major_axis` is the equatorial radius and `minor_axis` the polar radius, in meters.
    Spheroid { major_axis: f64, minor_axis: f64 },
}

impl TerrainShape {
    pub const WGS84: Self = TerrainShape::Spheroid {
        major_axis: 6378137.0,
        minor_axis: 6356752.314245,
    };

    pub fn face_size(self) -> f64 {
        2.0 * std::f64::consts::PI / 4.0 * self.scale_scalar()
    }

    pub fn scale_scalar(self) -> f64 {
        match self {
            TerrainShape::Plane { side_length } => side_length / 2.0,
            TerrainShape::Sphere { radius } => radius,
            TerrainShape::Spheroid { major_axis, .. } => major_axis,
        }
    }
    pub fn scale(self) -> DVec3 {
        match self {
            TerrainShape::Plane { side_length } => DVec3::new(side_length, 1.0, side_length),
            TerrainShape::Sphere { radius } => DVec3::splat(radius),
            TerrainShape::Spheroid {
                major_axis,
                minor_axis,
            } => DVec3::new(major_axis, minor_axis, major_axis),
        }
    }

    pub fn transform(self) -> Transform {
        Transform::from_scale(self.scale().as_vec3())
    }
    pub(crate) fn is_spherical(self) -> bool {
        match self {
            TerrainShape::Plane { .. } => false,
            TerrainShape::Sphere { .. } => true,
            TerrainShape::Spheroid { .. } => true,
        }
    }
    pub fn face_count(self) -> u32 {
        if self.is_spherical() { 6 } else { 1 }
    }

    /// Returns atmosphere radii and an initial planet-center transform for a flat map
    /// centered at the origin, with the planet north pole aligned to world `(0, 0, 0)` at `+Y`.
    ///
    /// At runtime, call [`planar_atmosphere_center`](Self::planar_atmosphere_center) each frame with the camera's horizontal
    /// position so the spherical atmosphere stays tangent to the flat terrain (+Y up).
    pub fn planar_atmosphere_alignment(
        min_height: f32,
        reference_minor_axis: f64,
        atmosphere_shell: f32,
    ) -> (f32, f32, Transform) {
        let inner_radius = (reference_minor_axis - min_height as f64) as f32;
        let outer_radius = inner_radius + atmosphere_shell;
        let transform = Transform::from_translation(Self::planar_atmosphere_center(
            min_height,
            inner_radius,
            Vec2::ZERO,
        ));
        (inner_radius, outer_radius, transform)
    }

    /// Planet center for a flat map with +Y up and the inner atmosphere sphere tangent at
    /// `horizontal` on the terrain reference height `surface_height`.
    pub fn planar_atmosphere_center(
        surface_height: f32,
        inner_radius: f32,
        horizontal: Vec2,
    ) -> Vec3 {
        Vec3::new(horizontal.x, surface_height - inner_radius, horizontal.y)
    }

    pub fn position_unit_to_local(self, unit_position: DVec3, height: f64) -> DVec3 {
        let local_position = self.scale() * unit_position;
        let local_normal = (self.scale()
            * if self.is_spherical() {
                unit_position
            } else {
                DVec3::Y
            })
        .normalize();

        local_position + height * local_normal
    }

    pub fn position_local_to_unit(self, local_position: DVec3) -> DVec3 {
        match self {
            TerrainShape::Plane { .. } => DVec3::new(1.0, 0.0, 1.0) * local_position / self.scale(),
            TerrainShape::Sphere { .. } => (local_position / self.scale()).normalize(),
            TerrainShape::Spheroid {
                major_axis,
                minor_axis,
            } => {
                let surface_position =
                    project_point_spheroid(major_axis, minor_axis, local_position);
                (surface_position / self.scale()).normalize()
            }
        }
    }
}
