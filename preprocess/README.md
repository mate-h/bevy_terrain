# Bevy Terrain Preprocess

Tiles a GeoTIFF or a plain heightmap into the files [`bevy_terrain`](https://crates.io/crates/bevy_terrain) streams at runtime. Reprojection and raster IO go through [GDAL](https://gdal.org/).

## GDAL

The build links GDAL and generates bindings from its headers, so libclang has to be installed too.

### macOS

```bash
brew install gdal
xcode-select --install
```

### Ubuntu

```bash
sudo apt install libgdal-dev libclang-dev
```

### Windows

Install [OSGeo4W](https://trac.osgeo.org/osgeo4w/) and [LLVM](https://github.com/llvm/llvm-project/releases). The MSVC build looks for `include\` and `lib\gdal_i.lib` under `GDAL_HOME`, and bindgen needs `libclang.dll`.

```bat
set GDAL_HOME=C:\OSGeo4W
set LIBCLANG_PATH=C:\Program Files\LLVM\bin
```

Put `%GDAL_HOME%\bin` on `PATH` when you run the tool so `gdal.dll` loads.

## Install

```bash
cargo install --git https://github.com/mate-h/bevy_terrain bevy_terrain_preprocess
```

The executable is `bevy_terrain_preprocess`. From a clone of this repo the same program is `cargo run --release -p bevy_terrain_preprocess --`.

## Georeferenced mode

This is the default. The input is a GeoTIFF with a CRS and a geotransform. The tool reprojects it onto a WGS84 cube-sphere and writes `TerrainShape::WGS84` in `config.tc.ron`.

```bash
bevy_terrain_preprocess source_data/mount_baker_full.tif \
  --terrain-path assets/terrains/mount_baker \
  --overwrite
```

## Planar mode (`--planar`)

For a game world that is not GIS data, `--planar` skips georeferencing. The raster is a plain heightmap on face 0. UV `(0.5, 0.5)` is the center, and local terrain coordinates use origin `(0, 0)` there.

`--side-length` is required, in meters or world units. It becomes `TerrainShape::Plane { side_length }` in the output config.

```bash
bevy_terrain_preprocess source_data/my_heightmap.tif \
  --terrain-path assets/terrains/my_world \
  --planar --side-length 8192 \
  --overwrite
```

From a clone:

```bash
cargo run --release -p bevy_terrain_preprocess --example preprocess_planar
```

At runtime, keep the terrain at identity transform and add `PlanarAtmospherePlugin` with `PlanarAtmosphereSettings::follow_camera` set to `true` so the atmosphere planet center tracks the camera horizontally on flat maps. Set `follow_camera` to `false` to keep a fixed planet center at the map origin instead (see the `planar_world` example).

## License

Bevy Terrain Preprocess source code is dual-licensed under either:

* MIT License (LICENSE-MIT or http://opensource.org/licenses/MIT)
* Apache License, Version 2.0 (LICENSE-APACHE or http://www.apache.org/licenses/LICENSE-2.0)

at your option.

## Contributions

Unless explicitly stated otherwise, any contribution intentionally submitted for inclusion in the work, as
defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
