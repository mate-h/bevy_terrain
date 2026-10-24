//! Preprocess a heightmap without georeferencing into flat tiles centered at the origin.
//!
//! Uses `source_data/mount_baker_full.tif` as sample input (geotags are ignored).
//!
//! `cargo run --release -p bevy_terrain_preprocess --example preprocess_planar`

use bevy_terrain::prelude::*;
use bevy_terrain_preprocess::prelude::*;
use gdal::raster::GdalDataType;

const SIDE_LENGTH: f64 = 8192.0;

fn main() {
    let args = Cli {
        src_path: vec!["source_data/mount_baker_full.tif".into()],
        terrain_path: "assets/terrains/planar_world".into(),
        temp_path: None,
        overwrite: true,
        no_data: PreprocessNoData::Source,
        data_type: PreprocessDataType::DataType(GdalDataType::Float32),
        fill_radius: 32.0,
        create_mask: true,
        lod_count: None,
        attachment_label: AttachmentLabel::Height,
        texture_size: 512,
        border_size: 1,
        mip_level_count: 1,
        format: AttachmentFormat::R32F,
        resample_alg: PreprocessResampleAlg::Bilinear,
        sparse: false,
        planar: true,
        side_length: Some(SIDE_LENGTH),
    };

    let (src_dataset, mut context) = PreprocessContext::from_cli(args).unwrap();

    preprocess(src_dataset, &mut context);
}
