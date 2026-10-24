//! California terrain — preprocessed from `source_data/california*.tif`.
//!
//! Run:
//! `cargo run --release -p bevy_terrain_preprocess --example preprocess_california`

use bevy_terrain::prelude::*;
use bevy_terrain_preprocess::prelude::*;
use gdal::raster::GdalDataType;
use std::path::Path;

const TERRAIN_PATH: &str = "assets/terrains/california";

fn main() {
    let args = Cli {
        src_path: vec!["source_data/california.tif".into()],
        terrain_path: TERRAIN_PATH.into(),
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
        planar: false,
        side_length: None,
    };

    let (src_dataset, mut context) = PreprocessContext::from_cli(args).unwrap();
    preprocess(src_dataset, &mut context);

    let lod_count = TerrainConfig::load_file(Path::new(TERRAIN_PATH).join("config.tc.ron"))
        .unwrap()
        .lod_count;

    let args = Cli {
        src_path: vec!["source_data/california_satellite_nad83.tif".into()],
        terrain_path: TERRAIN_PATH.into(),
        temp_path: None,
        overwrite: true,
        no_data: PreprocessNoData::NoData(0.0),
        data_type: PreprocessDataType::DataType(GdalDataType::UInt8),
        fill_radius: 0.0,
        create_mask: false,
        lod_count: Some(lod_count),
        attachment_label: AttachmentLabel::Custom("albedo".to_string()),
        texture_size: 512,
        border_size: 1,
        mip_level_count: 1,
        format: AttachmentFormat::Rgba8U,
        resample_alg: PreprocessResampleAlg::Bilinear,
        sparse: false,
        planar: false,
        side_length: None,
    };

    let (src_dataset, mut context) = PreprocessContext::from_cli(args).unwrap();
    preprocess(src_dataset, &mut context);

    let args = Cli {
        src_path: vec!["source_data/california_worldcover_nad83.tif".into()],
        terrain_path: TERRAIN_PATH.into(),
        temp_path: None,
        overwrite: true,
        no_data: PreprocessNoData::NoData(0.0),
        data_type: PreprocessDataType::DataType(GdalDataType::UInt8),
        fill_radius: 0.0,
        create_mask: false,
        lod_count: Some(lod_count),
        attachment_label: AttachmentLabel::Custom("landcover".to_string()),
        texture_size: 512,
        border_size: 1,
        mip_level_count: 1,
        format: AttachmentFormat::Rgba8U,
        resample_alg: PreprocessResampleAlg::Nearest,
        sparse: false,
        planar: false,
        side_length: None,
    };

    let (src_dataset, mut context) = PreprocessContext::from_cli(args).unwrap();
    preprocess(src_dataset, &mut context);
}
