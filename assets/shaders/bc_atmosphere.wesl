import bevy_terrain::shaders::types::AtlasTile;
import bevy_terrain::shaders::bindings::{terrain_data, albedo_attachment, normal_attachment, terrain_sampler};
import bevy_terrain::shaders::attachments::{compute_sample_uv, sample_height_mask, sample_surface_gradient};
import bevy_terrain::shaders::render::fragment::{FragmentInput, FragmentOutput, fragment_info, fragment_output};
@if(!DEFERRED_PREPASS) import bevy_terrain::shaders::render::fragment::fragment_debug;
import bevy_terrain::shaders::functions::lookup_tile;

struct BcMaterialSettings {
    show_albedo: u32,
    show_normal: u32,
}

@group(4) @binding(0)
var<uniform> material_settings: BcMaterialSettings;

fn sample_albedo(tile: AtlasTile) -> vec4<f32> {
    let uv = compute_sample_uv(tile, terrain_data.attachments.albedo);

    @if(SAMPLE_GRAD)
    return textureSampleGrad(albedo_attachment, terrain_sampler, uv.uv, tile.index, uv.dx, uv.dy);
    @if(!SAMPLE_GRAD)
    return textureSampleLevel(albedo_attachment, terrain_sampler, uv.uv, tile.index, tile.blend_ratio);
}

fn sample_normal(tile: AtlasTile) -> vec4<f32> {
    let uv = compute_sample_uv(tile, terrain_data.attachments.normal);

    @if(SAMPLE_GRAD)
    return textureSampleGrad(normal_attachment, terrain_sampler, uv.uv, tile.index, uv.dx, uv.dy);
    @if(!SAMPLE_GRAD)
    return textureSampleLevel(normal_attachment, terrain_sampler, uv.uv, tile.index, tile.blend_ratio);
}

@fragment
fn fragment(input: FragmentInput) -> FragmentOutput {
    var info = fragment_info(input);

    let tile             = lookup_tile(info.coordinate, info.blend);
    let mask             = sample_height_mask(tile);
    var color            = vec4<f32>(0.5);
    let surface_gradient = sample_surface_gradient(tile, info.tangent_space);

    if (material_settings.show_albedo != 0u) {
        color = sample_albedo(tile);
    } else if (material_settings.show_normal != 0u) {
        color = vec4<f32>(sample_normal(tile).rgb, 1.0);
    }

    if (color.a < 0.01) {
        color = vec4<f32>(0.5);
    }

    if (mask) { discard; }

    var output: FragmentOutput;
    fragment_output(&info, &output, color, surface_gradient);
    @if(DEFERRED_PREPASS)
    return output;
    @if(!DEFERRED_PREPASS)
    fragment_debug(&info, &output, tile, surface_gradient);
    @if(!DEFERRED_PREPASS)
    return FragmentOutput(vec4<f32>(output.color.xyz, 1.0));
}
