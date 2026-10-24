# bevy_terrain

[![crates.io](https://img.shields.io/crates/v/bevy_terrain)](https://crates.io/crates/bevy_terrain)
[![docs.rs](https://docs.rs/bevy_terrain/badge.svg)](https://docs.rs/bevy_terrain)

![California terrain in the atmosphere example](https://raw.githubusercontent.com/mate-h/bevy_terrain/bevy-0.20.0/images/atmosphere.webp)

A Bevy plugin for large-scale planetary and planar terrain. It streams tiled height and attachment data, builds view-dependent geometry, and keeps GPU positions stable at planetary scale.

Based on [this master thesis](https://doi.org/10.60687/2025-0147). [Video](https://youtu.be/zdz3-77-3EI).

## Features

- Spherical / WGS84 ellipsoid terrains and planar game-world terrains
- Quadtree tile streaming with morphing and blending
- Custom terrain materials and extra attachments (albedo, landcover, masks, ids)
- Bevy PBR lighting, atmosphere, deferred G-buffer, and motion vectors
- Heightfield terrain shadows that other materials can sample
- Optional [`big_space`](https://github.com/mate-h/big_space) floating origin
- GDAL-based preprocess crate for GeoTIFF and planar heightmaps

## Installation

```toml
bevy_terrain = "0.1"
```

Requires Bevy 0.20. The `big_space` feature is enabled by default. Disable default features if you do not need a floating origin:

```toml
bevy_terrain = { version = "0.1", default-features = false }
```

The GDAL preprocess tool lives in this repository (`preprocess/`) and is not published.

## Bevy version

| Branch | Bevy |
| --- | --- |
| [`bevy-0.20.0`](https://github.com/mate-h/bevy_terrain/tree/bevy-0.20.0) (and `main` after the 0.20 release) | 0.20 from crates.io |
| [`bevy-main`](https://github.com/mate-h/bevy_terrain/tree/bevy-main) | Bevy `main` |

## Examples

Preprocess a GeoTIFF first (see [`preprocess/README.md`](https://github.com/mate-h/bevy_terrain/blob/bevy-0.20.0/preprocess/README.md)). Example datasets are [here](https://drive.proton.me/urls/ZRDAC9SWTM#IxwKkKWSBgnV). Put source rasters in `source_data` and write tiles under `assets/terrains/<name>/`.

```bash
# California + atmosphere (needs assets/terrains/california)
cargo run --example atmosphere --features atmosphere_example

# Mount Baker + deferred shadows (R despawns / respawns the terrain)
cargo run --example mount_baker --features atmosphere_example

# Flat world on a planetary atmosphere shell
cargo run --example planar_world --features atmosphere_example

# Spherical globe (expects assets/terrains/earth; also loads California)
cargo run --example spherical
```

`bc_atmosphere` is the same path as `atmosphere` for the BC dataset. `precision_demo` needs `--features rand`.

Atmosphere examples use Bevy's free camera (`WASD`, mouse). Mount Baker uses deferred rendering; `T` toggles terrain shadows, Enter moves the sun, and `R` despawns / respawns the terrain.

`examples/spherical.rs` enables `TerrainDebugPlugin`: `T` fly camera, `R` orbit camera, `W` wireframe, `L`/`Y` LOD views, `N`/`E` blend distance, `I`/`O` morph distance.

## Screenshots

![Earth from the spherical example](https://raw.githubusercontent.com/mate-h/bevy_terrain/bevy-0.20.0/images/spherical.webp)

![California terrain, hazy light](https://raw.githubusercontent.com/mate-h/bevy_terrain/bevy-0.20.0/images/atmosphere-haze.webp)

## Attribution

Originally [kurtkuehnert/bevy_terrain](https://github.com/kurtkuehnert/bevy_terrain) by [Kurt Kühnert](https://github.com/kurtkuehnert).

The examples use the following [demo datasets](https://drive.proton.me/urls/ZRDAC9SWTM#IxwKkKWSBgnV):

- GEBCO Compilation Group (2023) - GEBCO 2023 Grid
- Unearthed Outdoors - True Marble Global Image Dataset GeoTIFF - [Creative Commons Attribution 3.0 United States License](https://creativecommons.org/licenses/by/3.0/us/legalcode)
- ©swisstopo - swissALTIRegio
- This work utilizes data made available under the Norwegian Licence for Open Government Data (NLOD), distributed by the Norwegian Offshore Directorate. The data were originally acquired by various entities. For more information, see https://www.sodir.no/en/about-us/open-data/.

## License

bevy_terrain source code is dual-licensed under either:

* MIT License (LICENSE-MIT or http://opensource.org/licenses/MIT)
* Apache License, Version 2.0 (LICENSE-APACHE or http://www.apache.org/licenses/LICENSE-2.0)

at your option.

The original thesis PDF, when present, is licensed under the [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) license instead.
