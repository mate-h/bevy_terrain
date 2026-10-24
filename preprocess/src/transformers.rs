use crate::{
    gdal_extension::{GDALCustomTransformer, GDALTransformerInfo, Transformer},
    result::{PreprocessError, PreprocessResult},
};
use bevy_terrain::math::Coordinate;
use gdal::{Dataset, GeoTransform, GeoTransformEx, errors::GdalError, spatial_ref::SpatialRef};
use gdal_sys::{
    GDALCreateReprojectionTransformerEx, GDALDestroyReprojectionTransformer,
    GDALReprojectionTransform,
};
use glam::{DVec2, DVec3};
use itertools::izip;
use std::ffi::c_void;
use std::ptr;

impl Transformer for GeoTransform {
    fn transform(
        &mut self,
        dst_to_src: bool,
        x: &mut [f64],
        y: &mut [f64],
        _: &mut [f64],
        _: &mut [bool],
    ) -> PreprocessResult<()> {
        let transform = if dst_to_src {
            self
        } else {
            &mut self.invert()?
        };

        for (x, y) in x.iter_mut().zip(y.iter_mut()) {
            (*x, *y) = transform.apply(*x, *y);
        }

        Ok(())
    }
}

pub struct ReprojectionTransformer {
    ptr: *mut c_void,
    counter: u32,
}

impl ReprojectionTransformer {
    fn new(src_spatial_ref: &SpatialRef, dst_spatial_ref: &SpatialRef) -> PreprocessResult<Self> {
        let ptr = unsafe {
            GDALCreateReprojectionTransformerEx(
                src_spatial_ref.to_c_hsrs(),
                dst_spatial_ref.to_c_hsrs(),
                ptr::null(),
            )
        };
        if ptr.is_null() {
            return Err(GdalError::NullPointer {
                method_name: "GDALCreateReprojectionTransformerEx",
                msg: "Creating the transformer failed".to_string(),
            }
            .into());
        }

        Ok(Self { ptr, counter: 0 })
    }
}

impl Drop for ReprojectionTransformer {
    fn drop(&mut self) {
        unsafe { GDALDestroyReprojectionTransformer(self.ptr) }
    }
}

impl Transformer for ReprojectionTransformer {
    fn transform(
        &mut self,
        dst_to_src: bool,
        x: &mut [f64],
        y: &mut [f64],
        z: &mut [f64],
        success: &mut [bool],
    ) -> PreprocessResult<()> {
        let mut success_int = vec![0; x.len()];

        self.counter += 1;

        let return_value = unsafe {
            GDALReprojectionTransform(
                self.ptr,
                dst_to_src.into(),
                x.len().try_into().unwrap(),
                x.as_mut_ptr(),
                y.as_mut_ptr(),
                z.as_mut_ptr(),
                success_int.as_mut_ptr(),
            )
        };

        if return_value == 0 {
            return Err(PreprocessError::TransformOperationFailed);
        }

        for (success_bool, &success_int) in success.iter_mut().zip(success_int.iter()) {
            *success_bool = *success_bool && success_int != 0;
        }

        Ok(())
    }
}

struct CubeTransformer {
    face: u32,
    is_spherical: bool,
}

impl CubeTransformer {
    fn new(face: u32, is_spherical: bool) -> Self {
        Self { face, is_spherical }
    }
}

impl Transformer for CubeTransformer {
    fn transform(
        &mut self,
        dst_to_src: bool,
        lon_or_u: &mut [f64],
        lat_or_v: &mut [f64],
        _: &mut [f64],
        success: &mut [bool],
    ) -> PreprocessResult<()> {
        if self.is_spherical {
            if dst_to_src {
                for (lon_or_u, lat_or_v, success) in
                    izip!(lon_or_u.iter_mut(), lat_or_v.iter_mut(), success.iter_mut())
                {
                    let coordinate = Coordinate::new(self.face, DVec2::new(*lon_or_u, *lat_or_v));
                    let unit_position = coordinate.unit_position(true);

                    let lon = unit_position.z.atan2(-unit_position.x);
                    let lat = unit_position.y.asin();

                    *success = *success && !lat.is_nan();
                    *lon_or_u = lon.to_degrees();
                    *lat_or_v = lat.to_degrees();
                }
            } else {
                for (lon_or_u, lat_or_v, success) in
                    izip!(lon_or_u.iter_mut(), lat_or_v.iter_mut(), success.iter_mut())
                {
                    let lon = lon_or_u.to_radians();
                    let lat = lat_or_v.to_radians();

                    let unit_position =
                        DVec3::new(-lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());

                    let coordinate = Coordinate::from_unit_position(unit_position, true);

                    *success = *success
                        && (unit_position.length() - 1.0).abs() < 0.00001
                        && coordinate.face == self.face;
                    *lon_or_u = coordinate.uv.x;
                    *lat_or_v = coordinate.uv.y;
                }
            }
        } else if dst_to_src {
            for (lon_or_u, lat_or_v, success) in
                izip!(lon_or_u.iter_mut(), lat_or_v.iter_mut(), success.iter_mut())
            {
                let coordinate = Coordinate::new(self.face, DVec2::new(*lon_or_u, *lat_or_v));
                let unit_position = coordinate.unit_position(false);
                *lon_or_u = unit_position.x + 0.5;
                *lat_or_v = unit_position.z + 0.5;
                *success =
                    *success && (0.0..=1.0).contains(lon_or_u) && (0.0..=1.0).contains(lat_or_v);
            }
        } else {
            for (lon_or_u, lat_or_v, success) in
                izip!(lon_or_u.iter_mut(), lat_or_v.iter_mut(), success.iter_mut())
            {
                let unit_position = DVec3::new(*lon_or_u - 0.5, 0.0, *lat_or_v - 0.5);
                let coordinate = Coordinate::from_unit_position(unit_position, false);
                *success = *success && coordinate.face == self.face;
                *lon_or_u = coordinate.uv.x;
                *lat_or_v = coordinate.uv.y;
            }
        }
        Ok(())
    }
}

struct PlanarPixelTransformer {
    src_width: f64,
    src_height: f64,
}

impl PlanarPixelTransformer {
    fn new(src: &Dataset) -> PreprocessResult<Self> {
        let (width, height) = src.raster_size();
        Ok(Self {
            src_width: width as f64,
            src_height: height as f64,
        })
    }
}

impl Transformer for PlanarPixelTransformer {
    fn transform(
        &mut self,
        dst_to_src: bool,
        x: &mut [f64],
        y: &mut [f64],
        _: &mut [f64],
        success: &mut [bool],
    ) -> PreprocessResult<()> {
        if dst_to_src {
            for (x, y, success) in izip!(x.iter_mut(), y.iter_mut(), success.iter_mut()) {
                let in_bounds = (0.0..=1.0).contains(x) && (0.0..=1.0).contains(y);
                *x = *x * self.src_width - 0.5;
                *y = (1.0 - *y) * self.src_height - 0.5;
                *success = *success
                    && in_bounds
                    && *x >= -0.5
                    && *y >= -0.5
                    && *x <= self.src_width - 0.5
                    && *y <= self.src_height - 0.5;
            }
        } else {
            for (x, y, success) in izip!(x.iter_mut(), y.iter_mut(), success.iter_mut()) {
                let in_bounds = *x >= -0.5
                    && *y >= -0.5
                    && *x <= self.src_width - 0.5
                    && *y <= self.src_height - 0.5;
                *x = (*x + 0.5) / self.src_width;
                *y = 1.0 - (*y + 0.5) / self.src_height;
                *success = *success && in_bounds;
            }
        }

        Ok(())
    }
}

#[repr(C)]
struct GeorefCustomTransformer {
    src_inverse_geo_transform: GeoTransform,
    dst_geo_transform: Option<GeoTransform>,
    lon_lat_transformer: ReprojectionTransformer,
    cube_transformer: CubeTransformer,
}

#[repr(C)]
struct PlanarCustomTransformer {
    src_pixel_transformer: PlanarPixelTransformer,
    dst_geo_transform: Option<GeoTransform>,
}

#[unsafe(no_mangle)]
pub extern "C" fn clone_custom_transformer(arg: *mut c_void, _: f64, _: f64) -> *mut c_void {
    arg
}

impl CustomTransformer {
    pub fn new(
        src: &Dataset,
        face: u32,
        dst_geo_transform: Option<GeoTransform>,
        planar: bool,
    ) -> PreprocessResult<GDALCustomTransformer> {
        let inner: Box<dyn Transformer> = if planar {
            Box::new(PlanarCustomTransformer {
                src_pixel_transformer: PlanarPixelTransformer::new(src)?,
                dst_geo_transform,
            })
        } else {
            Box::new(GeorefCustomTransformer {
                src_inverse_geo_transform: src.geo_transform()?.invert()?,
                dst_geo_transform,
                lon_lat_transformer: ReprojectionTransformer::new(
                    &src.spatial_ref()?,
                    &SpatialRef::from_proj4("+proj=lonlat +ellps=WGS84 +datum=WGS84")?,
                )?,
                cube_transformer: CubeTransformer::new(face, true),
            })
        };

        Ok(GDALCustomTransformer {
            info: GDALTransformerInfo::new(clone_custom_transformer),
            inner,
        })
    }
}

pub struct CustomTransformer;

impl Transformer for GeorefCustomTransformer {
    fn transform(
        &mut self,
        dst_to_src: bool,
        x: &mut [f64],
        y: &mut [f64],
        z: &mut [f64],
        success: &mut [bool],
    ) -> PreprocessResult<()> {
        for success in success.iter_mut() {
            *success = true;
        }

        if dst_to_src {
            if let Some(mut geo_transform) = self.dst_geo_transform {
                geo_transform.transform(dst_to_src, x, y, z, success)?;
            }

            self.cube_transformer
                .transform(dst_to_src, x, y, z, success)?;
            self.lon_lat_transformer
                .transform(dst_to_src, x, y, z, success)?;
            self.src_inverse_geo_transform
                .transform(dst_to_src, x, y, z, success)?;
        } else {
            self.src_inverse_geo_transform
                .transform(dst_to_src, x, y, z, success)?;
            self.lon_lat_transformer
                .transform(dst_to_src, x, y, z, success)?;
            self.cube_transformer
                .transform(dst_to_src, x, y, z, success)?;
        }

        Ok(())
    }
}

impl Transformer for PlanarCustomTransformer {
    fn transform(
        &mut self,
        dst_to_src: bool,
        x: &mut [f64],
        y: &mut [f64],
        z: &mut [f64],
        success: &mut [bool],
    ) -> PreprocessResult<()> {
        for success in success.iter_mut() {
            *success = true;
        }

        if dst_to_src {
            if let Some(mut geo_transform) = self.dst_geo_transform {
                geo_transform.transform(dst_to_src, x, y, z, success)?;
            }

            self.src_pixel_transformer
                .transform(dst_to_src, x, y, z, success)?;
        } else {
            self.src_pixel_transformer
                .transform(dst_to_src, x, y, z, success)?;
        }

        Ok(())
    }
}
