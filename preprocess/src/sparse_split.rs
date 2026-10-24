use crate::{
    dataset::{FaceInfo, PreprocessContext, create_tile_dataset},
    gdal_extension::{CountingProgressCallback, ProgressCallback, SharedReadOnlyDataset, warp},
    reproject::compute_transforms,
    result::{PreprocessError, PreprocessResult},
    stitch::stitch,
    transformers::CustomTransformer,
};
use bevy_terrain::{math::TileCoordinate, prelude::AttachmentLabel};
use gdal::raster::GdalDataType;
use gdal::{Dataset, GeoTransform, raster::GdalType};
use glam::IVec2;
use itertools::iproduct;
use num::NumCast;
use rayon::prelude::*;
use std::{collections::HashMap, fs, path::PathBuf};

pub fn compute_face_infos(
    src_dataset: &Dataset,
    context: &mut PreprocessContext,
) -> PreprocessResult<HashMap<u32, FaceInfo>> {
    let transforms = compute_transforms(src_dataset, context, None)?;

    Ok(transforms
        .into_iter()
        .map(|transform| {
            (
                transform.face,
                FaceInfo {
                    lod: transform.lod,
                    pixel_start: transform.pixel_start,
                    pixel_end: transform.pixel_end,
                    path: PathBuf::new(),
                },
            )
        })
        .collect())
}

fn tile_geo_transform(tile: TileCoordinate, context: &PreprocessContext) -> GeoTransform {
    let center_size = context.attachment.center_size();
    let border = context.attachment.border_size as i32;
    let pixel_size = 1.0 / ((1 << tile.lod) * center_size) as f64;
    let pixel_start = tile.xy * center_size as i32 - border;

    GeoTransform::from([
        pixel_start.x as f64 * pixel_size,
        pixel_size,
        0.0,
        pixel_start.y as f64 * pixel_size,
        0.0,
        pixel_size,
    ])
}

fn initialize_tile_nodata<T: Copy + GdalType + NumCast>(
    dataset: &Dataset,
    context: &PreprocessContext,
) -> PreprocessResult<()> {
    let Some(no_data_value) = context.no_data_value else {
        return Ok(());
    };

    let Some(no_data) = T::from(no_data_value) else {
        return Ok(());
    };

    let size = context.attachment.texture_size as usize;
    let mut buffer = gdal::raster::Buffer::new((size, size), vec![no_data; size * size]);

    for band in dataset.rasterbands() {
        band?.write((0, 0), (size, size), &mut buffer)?;
    }

    Ok(())
}

fn is_valid_pixel<T: Copy + PartialEq + NumCast>(
    value: T,
    no_data: Option<T>,
    data_type: GdalDataType,
) -> bool {
    match no_data {
        None => true,
        Some(no_data_value) => {
            if data_type == GdalDataType::Float32 {
                let value = value.to_f64().unwrap_or(f64::NAN);
                let no_data_value = no_data_value.to_f64().unwrap_or(f64::NAN);
                if no_data_value.is_nan() {
                    return !value.is_nan();
                }
            }
            value != no_data_value
        }
    }
}

fn tile_has_data<T: Copy + PartialEq + NumCast + GdalType>(
    dataset: &Dataset,
    context: &PreprocessContext,
) -> PreprocessResult<bool> {
    let mut has_data = false;

    for raster in dataset.rasterbands() {
        let raster = raster?;
        let buffer = raster.read_as::<T>(
            (0, 0),
            (
                context.attachment.texture_size as usize,
                context.attachment.texture_size as usize,
            ),
            (
                context.attachment.texture_size as usize,
                context.attachment.texture_size as usize,
            ),
            None,
        )?;

        let no_data_value = raster
            .no_data_value()
            .map(|v| T::from(v).ok_or(PreprocessError::NoDataOutOfRange))
            .transpose()?;

        has_data |= buffer
            .data()
            .iter()
            .any(|&value| is_valid_pixel(value, no_data_value, context.data_type));
    }

    Ok(has_data)
}

fn tile_height_range<T: Copy + GdalType + NumCast + PartialEq>(
    dataset: &Dataset,
    context: &PreprocessContext,
) -> Option<(f32, f32)> {
    let raster = dataset.rasterband(1).ok()?;
    let buffer = raster
        .read_as::<T>(
            (0, 0),
            (
                context.attachment.texture_size as usize,
                context.attachment.texture_size as usize,
            ),
            (
                context.attachment.texture_size as usize,
                context.attachment.texture_size as usize,
            ),
            None,
        )
        .ok()?;

    let no_data_value = raster.no_data_value().and_then(|v| T::from(v));

    let mut min = f32::MAX;
    let mut max = f32::MIN;
    let mut found = false;

    for &value in buffer.data() {
        if !is_valid_pixel(value, no_data_value, context.data_type) {
            continue;
        }
        let value = value.to_f64()? as f32;
        min = min.min(value);
        max = max.max(value);
        found = true;
    }

    found.then_some((min, max))
}

fn collect_candidate_tiles(
    faces: &HashMap<u32, FaceInfo>,
    context: &PreprocessContext,
) -> Vec<TileCoordinate> {
    faces
        .iter()
        .flat_map(|(&face, info)| {
            let xy_start = info.pixel_start / context.attachment.center_size() as i32;
            let xy_end = (info.pixel_end - 1) / context.attachment.center_size() as i32 + 1;

            iproduct!(xy_start.x..xy_end.x, xy_start.y..xy_end.y)
                .map(move |(x, y)| TileCoordinate::new(face, info.lod, IVec2::new(x, y)))
        })
        .collect()
}

fn warp_tile<T: Copy + GdalType + PartialEq + NumCast>(
    src_dataset: &SharedReadOnlyDataset,
    tile_coordinate: TileCoordinate,
    context: &PreprocessContext,
    force: bool,
) -> PreprocessResult<Option<(TileCoordinate, Option<(f32, f32)>)>> {
    let src = src_dataset.get();
    let tile_path = tile_coordinate.path(&context.tile_dir);
    let mut transformer = CustomTransformer::new(
        src,
        tile_coordinate.face,
        Some(tile_geo_transform(tile_coordinate, context)),
        context.planar,
    )?;

    let tile_dataset = create_tile_dataset::<T>(tile_coordinate, context)?;

    initialize_tile_nodata::<T>(&tile_dataset, context)?;

    warp(src, &tile_dataset, context, &mut transformer, None)?;

    let has_data = tile_has_data::<T>(&tile_dataset, context)?;

    if !has_data && !force {
        drop(tile_dataset);
        let _ = fs::remove_file(&tile_path);
        return Ok(None);
    }

    let height_range = if matches!(context.attachment_label, AttachmentLabel::Height) && has_data {
        tile_height_range::<T>(&tile_dataset, context)
    } else {
        None
    };

    if matches!(context.attachment_label, AttachmentLabel::Height)
        && has_data
        && height_range.is_none()
    {
        drop(tile_dataset);
        let _ = fs::remove_file(&tile_path);
        return Ok(None);
    }

    Ok(Some((tile_coordinate, height_range)))
}

pub fn sparse_split_and_stitch<T: Copy + GdalType + PartialEq + NumCast>(
    src_dataset: &Dataset,
    context: &mut PreprocessContext,
    progress_callback: Option<&ProgressCallback>,
) -> PreprocessResult<Vec<TileCoordinate>> {
    let faces = compute_face_infos(src_dataset, context)?;
    let max_lod = context.lod_count.unwrap() - 1;

    let candidate_tiles = if let Some(manifest) = &context.tile_manifest {
        manifest
            .iter()
            .copied()
            .filter(|tile| tile.lod == max_lod)
            .collect()
    } else {
        collect_candidate_tiles(&faces, context)
    };

    let total = candidate_tiles.len() as u64;
    let progress_callback = CountingProgressCallback::new(total * 2, progress_callback);
    let force_tiles = context.tile_manifest.is_some();
    let src = SharedReadOnlyDataset::new(&context.src_path);

    let warp_results: Vec<_> = candidate_tiles
        .par_iter()
        .map(|&tile_coordinate| {
            let result = warp_tile::<T>(&src, tile_coordinate, context, force_tiles);
            progress_callback.increment();
            result
        })
        .collect::<PreprocessResult<Vec<_>>>()?;

    let mut output_tiles = Vec::new();
    for result in warp_results.into_iter().flatten() {
        if let Some((min, max)) = result.1 {
            context.min_height = context.min_height.min(min);
            context.max_height = context.max_height.max(max);
        }
        output_tiles.push(result.0);
    }

    let placed = output_tiles.len();
    let skipped = candidate_tiles.len() - placed;
    println!(
        "Sparse warp: placed {placed} tiles, skipped {skipped} empty ({} candidates)",
        candidate_tiles.len()
    );

    stitch::<T>(&output_tiles, context, &progress_callback)?;

    Ok(output_tiles)
}
