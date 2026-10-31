import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;
import bevy_pbr::render::pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT;
import bevy_pbr::render::pbr_functions::apply_pbr_lighting;
import bevy_pbr::deferred::functions::{deferred_gbuffer_from_pbr_input, pbr_input_from_deferred_gbuffer};

// Shades the terrain's private G-buffer once per pixel. The terrain draw only
// writes material inputs, so overdraw (and the mask `discard`, which defeats
// hidden surface removal) no longer multiplies the lighting cost. The result
// is written back as an unlit G-buffer texel, which the composite restages
// into Bevy's deferred attachments and Bevy's deferred lighting pass copies.
//
// Bevy's deferred lighting can't shade the terrain itself: it has no
// `directional_shadow_factor` for the terrain shadow map. The terrain draw
// passes that factor in the diffuse occlusion instead.
//
// Groups 0 and 1 are Bevy's mesh-view bind groups. They contain the
// current-frame deferred texture, which is why this pass writes another
// private target instead of Bevy's attachments.
@group(2) @binding(0)
var terrain_gbuffer: texture_2d<u32>;
@group(2) @binding(1)
var terrain_depth: texture_depth_2d;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<u32> {
    let pixel = vec2<u32>(in.position.xy);
    let depth = textureLoad(terrain_depth, pixel, 0);

    // No terrain here. The composite's depth test never copies these texels.
    if (depth == 0.0) { discard; }

    var pbr_input = pbr_input_from_deferred_gbuffer(vec4<f32>(in.position.xy, depth, 1.0),
                                                    textureLoad(terrain_gbuffer, pixel, 0));

    pbr_input.directional_shadow_factor = pbr_input.diffuse_occlusion.x;
    pbr_input.diffuse_occlusion         = vec3<f32>(1.0);

    pbr_input.material.base_color = apply_pbr_lighting(pbr_input);
    pbr_input.material.flags      = pbr_input.material.flags | STANDARD_MATERIAL_FLAGS_UNLIT_BIT;

    return deferred_gbuffer_from_pbr_input(pbr_input);
}
