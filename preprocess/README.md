# Bevy Terrain Preprocess

A preprocessing tool for the [bevy_terrain](https://github.com/mate-h/bevy_terrain) library.

The bevy terrain library makes use of a quite specific terrain data format.
To use the library, you have to convert your geodata, heightmaps, etc. to the format expected by bevy terrain.
To make this as simple as possible this library provides a CLI, that is build using the [GDAL](https://gdal.org) geodata
translator library.

## Georeferenced mode (default)

Input must be a GeoTIFF with CRS and geotransform metadata. The pipeline reprojects data onto a WGS84 cube-sphere and writes `TerrainShape::WGS84` in `config.tc.ron`.

```bash
cargo run --release -p bevy_terrain_preprocess -- \
  source_data/mount_baker_full.tif \
  --terrain-path assets/terrains/mount_baker \
  --overwrite
```

## Planar mode (`--planar`)

For game worlds that are not derived from GIS data, use `--planar` to skip georeferencing. The raster is treated as a plain heightmap mapped to face 0 with UV coordinates where `(0.5, 0.5)` is the map center and local terrain coordinates use origin `(0, 0)` at that center.

`--side-length` (meters / world units) is required and becomes `TerrainShape::Plane { side_length }` in the output config.

```bash
cargo run --release -p bevy_terrain_preprocess -- \
  source_data/my_heightmap.tif \
  --terrain-path assets/terrains/my_world \
  --planar --side-length 8192 \
  --overwrite
```

Or via example:

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
