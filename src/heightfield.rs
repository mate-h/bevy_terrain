//! [`Heightfield`] tiles a height grid in memory, so a planar terrain needs no GDAL preprocessing.
//!
//! [`TerrainConfig::from_heightfield`] produces the same tile pyramid `bevy_terrain_preprocess`
//! writes for a planar map, but keeps the tiles in the config instead of on disk. Add the config
//! to `Assets<TerrainConfig>` and pass the handle to
//! [`spawn_terrain`](crate::spawn::SpawnTerrainCommandsExt::spawn_terrain).
//!
//! Tiling is synchronous CPU work. For large inputs build the config on the
//! `AsyncComputeTaskPool` and add it once it is done.

use crate::{
    math::{TerrainShape, TileCoordinate},
    terrain::TerrainConfig,
    terrain_data::{AttachmentConfig, AttachmentFormat, AttachmentLabel},
};
use bevy::{platform::collections::HashMap, prelude::*, render::render_resource::TextureFormat};
use bytemuck::cast_slice;
use std::{fmt, sync::Arc, thread};

/// Height tiles of a [`TerrainConfig`] built by [`TerrainConfig::from_heightfield`].
#[derive(Clone)]
pub(crate) struct MemoryTiles(pub(crate) Arc<HashMap<TileCoordinate, Vec<f32>>>);

impl fmt::Debug for MemoryTiles {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MemoryTiles({} tiles)", self.0.len())
    }
}

/// A grid of heights in meters, row-major with row 0 at the top of the image.
#[derive(Clone, Debug)]
pub struct Heightfield {
    width: u32,
    height: u32,
    data: Vec<f32>,
    no_data: Option<f32>,
}

impl Heightfield {
    /// `data` holds `width * height` heights, row 0 first.
    pub fn new(width: u32, height: u32, data: Vec<f32>) -> Self {
        assert!(width > 0 && height > 0, "heightfield must not be empty");
        assert_eq!(
            data.len(),
            (width * height) as usize,
            "heightfield data must hold width * height values"
        );

        Self {
            width,
            height,
            data,
            no_data: None,
        }
    }

    /// Treat texels equal to `value` (or NaN) as missing. The terrain masks them out.
    pub fn with_no_data(mut self, value: f32) -> Self {
        self.no_data = Some(value);
        self
    }

    /// Read the first channel of a grayscale or RGBA image and map `0..=1` to `min..=max`.
    ///
    /// Supports `R8Unorm`, `R16Unorm`, `R16Uint` (16-bit PNGs), `R32Float`, and `Rgba8Unorm(Srgb)`.
    /// `R32Float` values are taken as is, without the remap.
    pub fn from_image(image: &Image, min: f32, max: f32) -> Result<Self> {
        let bytes = image
            .data
            .as_deref()
            .ok_or("heightfield image has no CPU data")?;
        let remap = |value: f32| min + value * (max - min);

        let data: Vec<f32> = match image.texture_descriptor.format {
            TextureFormat::R8Unorm => bytes.iter().map(|&v| remap(v as f32 / 255.0)).collect(),
            TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => bytes
                .chunks_exact(4)
                .map(|v| remap(v[0] as f32 / 255.0))
                .collect(),
            TextureFormat::R16Unorm | TextureFormat::R16Uint => cast_slice::<u8, u16>(bytes)
                .iter()
                .map(|&v| remap(v as f32 / 65535.0))
                .collect(),
            TextureFormat::R32Float => cast_slice::<u8, f32>(bytes).to_vec(),
            format => return Err(format!("unsupported heightfield format {format:?}").into()),
        };

        Ok(Self::new(image.width(), image.height(), data))
    }

    fn sample(&self, x: u32, y: u32) -> f32 {
        let value = self.data[(y * self.width + x) as usize];
        if self.no_data == Some(value) {
            f32::NAN
        } else {
            value
        }
    }

    /// Bilinear sample at a continuous pixel position (texel centers on integers), clamped to
    /// the edge. Missing taps are left out of the weights, NaN if all four are missing.
    fn bilinear(&self, x: f64, y: f64) -> f32 {
        let x = x.clamp(0.0, (self.width - 1) as f64);
        let y = y.clamp(0.0, (self.height - 1) as f64);
        let (x0, y0) = (x.floor() as u32, y.floor() as u32);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32);

        weighted_mean([
            (self.sample(x0, y0), (1.0 - fx) * (1.0 - fy)),
            (self.sample(x1, y0), fx * (1.0 - fy)),
            (self.sample(x0, y1), (1.0 - fx) * fy),
            (self.sample(x1, y1), fx * fy),
        ])
    }
}

/// Grid size and tile layout for [`TerrainConfig::from_heightfield`].
#[derive(Clone, Debug)]
pub struct HeightfieldSettings {
    /// World width and depth of the map, in meters.
    pub side_length: f64,
    /// Tile edge length in texels, including the border.
    pub texture_size: u32,
    /// Overlap around each tile, so filtering never reads past its edge.
    pub border_size: u32,
    /// LOD count. `None` picks the smallest count whose finest level resolves every source texel.
    pub lod_count: Option<u32>,
}

impl Default for HeightfieldSettings {
    fn default() -> Self {
        Self {
            side_length: 1.0,
            texture_size: 512,
            border_size: 1,
            lod_count: None,
        }
    }
}

impl HeightfieldSettings {
    /// Defaults with the given map size in meters.
    pub fn new(side_length: f64) -> Self {
        Self {
            side_length,
            ..default()
        }
    }
}

/// Mean of the non-NaN values, or NaN if every weighted value is NaN.
fn weighted_mean<const N: usize>(taps: [(f32, f32); N]) -> f32 {
    let (mut sum, mut weight) = (0.0, 0.0);
    for (value, w) in taps {
        if !value.is_nan() && w > 0.0 {
            sum += value * w;
            weight += w;
        }
    }
    if weight > 0.0 { sum / weight } else { f32::NAN }
}

/// Fill `values` (a square grid with `size` columns) row by row, in parallel.
fn fill_rows(values: &mut [f32], size: usize, row: impl Fn(usize, &mut [f32]) + Sync) {
    let threads = thread::available_parallelism().map_or(1, |n| n.get());
    let rows_per_chunk = size.div_ceil(threads).max(1);

    thread::scope(|scope| {
        for (chunk_index, chunk) in values.chunks_mut(rows_per_chunk * size).enumerate() {
            let row = &row;
            scope.spawn(move || {
                for (i, values) in chunk.chunks_mut(size).enumerate() {
                    row(chunk_index * rows_per_chunk + i, values);
                }
            });
        }
    });
}

/// One LOD resampled onto the quadtree pixel grid. NaN marks missing data.
struct Level {
    size: usize,
    values: Vec<f32>,
}

impl Level {
    /// The finest level. Matches the planar warp of `bevy_terrain_preprocess`: the map spans
    /// UV `0..1`, and UV `v = 0` is the bottom row of the source.
    fn resample(heightfield: &Heightfield, size: usize) -> Self {
        let (width, height) = (heightfield.width as f64, heightfield.height as f64);
        let mut values = vec![0.0; size * size];

        fill_rows(&mut values, size, |row, values| {
            let v = (row as f64 + 0.5) / size as f64;
            let y = (1.0 - v) * height - 0.5;
            for (column, value) in values.iter_mut().enumerate() {
                let u = (column as f64 + 0.5) / size as f64;
                *value = heightfield.bilinear(u * width - 0.5, y);
            }
        });

        Self { size, values }
    }

    /// The next coarser level: each texel averages the present ones of its 2x2 children.
    fn downsample(&self) -> Self {
        let size = self.size / 2;
        let mut values = vec![0.0; size * size];

        fill_rows(&mut values, size, |row, values| {
            for (column, value) in values.iter_mut().enumerate() {
                let child = |dx, dy| self.get(2 * column + dx, 2 * row + dy);
                *value = weighted_mean([
                    (child(0, 0), 1.0),
                    (child(1, 0), 1.0),
                    (child(0, 1), 1.0),
                    (child(1, 1), 1.0),
                ]);
            }
        });

        Self { size, values }
    }

    fn get(&self, x: usize, y: usize) -> f32 {
        self.values[y * self.size + x]
    }

    fn get_clamped(&self, x: i64, y: i64) -> f32 {
        let max = self.size as i64 - 1;
        self.get(x.clamp(0, max) as usize, y.clamp(0, max) as usize)
    }
}

impl TerrainConfig {
    /// Tile `heightfield` into a planar terrain held in memory.
    ///
    /// The result has one `R32F` height attachment and the same tile layout
    /// `bevy_terrain_preprocess --planar` would write. Add it to `Assets<TerrainConfig>` and
    /// spawn the handle. With [`Heightfield::with_no_data`], missing texels are filled with the
    /// lowest height and masked out.
    pub fn from_heightfield(heightfield: &Heightfield, settings: &HeightfieldSettings) -> Self {
        let center_size = settings.texture_size - 2 * settings.border_size;
        let max_lod = settings.lod_count.map_or_else(
            || {
                let max_dim = heightfield.width.max(heightfield.height) as f64;
                (max_dim / center_size as f64).log2().ceil().max(0.0) as u32
            },
            |lod_count| lod_count.max(1) - 1,
        );
        let mask = heightfield.no_data.is_some();

        let mut levels = vec![Level::resample(
            heightfield,
            (1usize << max_lod) * center_size as usize,
        )];
        for _ in 0..max_lod {
            levels.push(levels.last().unwrap().downsample());
        }
        levels.reverse();

        let (min_height, max_height) = levels[max_lod as usize]
            .values
            .iter()
            .filter(|value| !value.is_nan())
            .fold((f32::MAX, f32::MIN), |(min, max), &value| {
                (min.min(value), max.max(value))
            });
        let (min_height, max_height) = if min_height <= max_height {
            (min_height, max_height)
        } else {
            (0.0, 0.0)
        };

        let mut tiles = HashMap::new();
        for (lod, level) in levels.iter().enumerate() {
            let tile_count = 1 << lod;
            for (x, y) in (0..tile_count).flat_map(|y| (0..tile_count).map(move |x| (x, y))) {
                if let Some(data) = cut_tile(level, x, y, settings, min_height, mask) {
                    tiles.insert(TileCoordinate::new(0, lod as u32, IVec2::new(x, y)), data);
                }
            }
        }

        let mut config = Self {
            path: default(),
            shape: TerrainShape::Plane {
                side_length: settings.side_length,
            },
            lod_count: max_lod + 1,
            min_height,
            max_height,
            attachments: default(),
            tiles: tiles.keys().copied().collect(),
            memory_tiles: Some(MemoryTiles(Arc::new(tiles))),
        };
        config.add_attachment(
            AttachmentLabel::Height,
            AttachmentConfig {
                texture_size: settings.texture_size,
                border_size: settings.border_size,
                mip_level_count: 1,
                mask,
                format: AttachmentFormat::R32F,
            },
        );
        config
    }
}

impl TerrainConfig {
    /// The in-memory height tile at `coordinate`, if this config came from
    /// [`TerrainConfig::from_heightfield`] and the tile has data.
    pub fn memory_tile(&self, coordinate: TileCoordinate) -> Option<&[f32]> {
        self.memory_tiles.as_ref()?.0.get(&coordinate).map(Vec::as_slice)
    }
}

/// Copy one tile, border included, out of `level`. `None` if its center holds no data.
/// Borders read the neighbours' texels directly, so no separate stitch pass is needed.
fn cut_tile(
    level: &Level,
    x: i32,
    y: i32,
    settings: &HeightfieldSettings,
    fill: f32,
    mask: bool,
) -> Option<Vec<f32>> {
    let center_size = (settings.texture_size - 2 * settings.border_size) as i64;
    let size = settings.texture_size as i64;
    let border = settings.border_size as i64;
    let origin = (
        x as i64 * center_size - border,
        y as i64 * center_size - border,
    );

    let in_center = |i: i64| (border..border + center_size).contains(&i);
    let mut has_data = false;
    let mut data = Vec::with_capacity((size * size) as usize);

    for j in 0..size {
        for i in 0..size {
            let value = level.get_clamped(origin.0 + i, origin.1 + j);
            let present = !value.is_nan();
            has_data |= present && in_center(i) && in_center(j);

            let value = if present { value } else { fill };
            // Same encoding as the preprocess mask: present texels have the low bit set.
            data.push(if mask {
                f32::from_bits((value.to_bits() & !1) | present as u32)
            } else {
                value
            });
        }
    }

    has_data.then_some(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(texture_size: u32, border_size: u32) -> HeightfieldSettings {
        HeightfieldSettings {
            side_length: 100.0,
            texture_size,
            border_size,
            lod_count: None,
        }
    }

    fn tile(config: &TerrainConfig, lod: u32, x: i32, y: i32) -> &Vec<f32> {
        &config.memory_tiles.as_ref().unwrap().0[&TileCoordinate::new(0, lod, IVec2::new(x, y))]
    }

    #[test]
    fn lod_count_covers_the_source() {
        let heightfield = Heightfield::new(100, 60, vec![0.0; 6000]);
        // center 14: 100 / 14 -> 2^3 tiles per side
        let config = TerrainConfig::from_heightfield(&heightfield, &settings(16, 1));
        assert_eq!(config.lod_count, 4);
        assert_eq!(config.tiles.len(), 1 + 4 + 16 + 64);

        let small = Heightfield::new(10, 10, vec![0.0; 100]);
        let config = TerrainConfig::from_heightfield(&small, &settings(16, 1));
        assert_eq!(config.lod_count, 1);
    }

    #[test]
    fn constant_input_gives_constant_tiles() {
        let heightfield = Heightfield::new(50, 50, vec![42.5; 2500]);
        let config = TerrainConfig::from_heightfield(&heightfield, &settings(16, 2));
        let close = |v: f32| (v - 42.5).abs() < 1e-4;
        assert!(close(config.min_height) && close(config.max_height));
        for data in config.memory_tiles.as_ref().unwrap().0.values() {
            assert_eq!(data.len(), 256);
            assert!(data.iter().copied().all(close));
        }
    }

    #[test]
    fn borders_match_neighbour_centers() {
        let (w, h) = (64, 64);
        let data = (0..w * h)
            .map(|i| ((i % w) * 3 + (i / w) * 7) as f32)
            .collect();
        let heightfield = Heightfield::new(w, h, data);
        let config = TerrainConfig::from_heightfield(&heightfield, &settings(20, 2));
        let (size, border, center) = (20usize, 2usize, 16usize);

        let lod = config.lod_count - 1;
        let left = tile(&config, lod, 0, 1);
        let right = tile(&config, lod, 1, 1);
        for j in 0..size {
            for b in 0..border {
                // right border of `left` == first center columns of `right`
                assert_eq!(
                    left[j * size + border + center + b],
                    right[j * size + border + b]
                );
                // left border of `right` == last center columns of `left`
                assert_eq!(right[j * size + b], left[j * size + center + b]);
            }
        }
    }

    #[test]
    fn min_max_follow_the_data() {
        let data = (0..400).map(|i| i as f32 - 100.0).collect();
        let heightfield = Heightfield::new(20, 20, data);
        let config = TerrainConfig::from_heightfield(&heightfield, &settings(32, 1));
        assert_eq!(config.lod_count, 1);
        assert!((config.min_height - -100.0).abs() < 1.0);
        assert!((config.max_height - 299.0).abs() < 1.0);
    }

    #[test]
    fn no_data_is_masked_and_empty_tiles_are_skipped() {
        // Left half valid, right half missing.
        let (w, h) = (56, 56);
        let data = (0..w * h)
            .map(|i| if i % w < w / 2 { 10.0 } else { -9999.0 })
            .collect();
        let heightfield = Heightfield::new(w, h, data).with_no_data(-9999.0);
        let config = TerrainConfig::from_heightfield(&heightfield, &settings(16, 1));
        assert_eq!(config.lod_count, 3);
        assert!(config.attachments[&AttachmentLabel::Height].mask);
        assert_eq!((config.min_height, config.max_height), (10.0, 10.0));

        let lod = config.lod_count - 1;
        assert!(
            config
                .tiles
                .contains(&TileCoordinate::new(0, lod, IVec2::new(0, 0)))
        );
        assert!(
            !config
                .tiles
                .contains(&TileCoordinate::new(0, lod, IVec2::new(3, 0)))
        );

        for data in config.memory_tiles.as_ref().unwrap().0.values() {
            assert!(
                data.iter().all(|&value| (value - 10.0).abs() < 1e-3),
                "missing texels are filled with the lowest height"
            );
        }
        let first = tile(&config, lod, 0, 0);
        assert_eq!(
            first[16 + 1].to_bits() & 1,
            1,
            "valid texel keeps its mask bit"
        );
        // Tile (1, 0) covers columns 14..28; its texel 15 reads column 28, the first missing one.
        let edge = tile(&config, lod, 1, 0);
        assert_eq!(
            edge[16 + 15].to_bits() & 1,
            0,
            "missing texel has its mask bit cleared"
        );
    }
}
