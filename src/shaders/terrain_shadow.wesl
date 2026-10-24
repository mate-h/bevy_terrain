import bevy_terrain::shaders::types::TerrainShadow;

// Samples the RDR2-style terrain shadow map: compares the point's elevation
// above the ellipsoid with the raymarched intersection height (R), fading over
// a penumbra width derived from the ray length to the occluder (G). Takes the
// map, sampler, and params as arguments so materials outside the terrain
// pipeline can bind them in their own groups.
fn terrain_shadow_factor_at(
    world_position: vec3<f32>,
    params: TerrainShadow,
    map: texture_2d<f32>,
    map_sampler: sampler,
) -> f32 {
    if (params.enabled == 0u) { return 1.0; }

    let delta      = world_position - params.center;
    let horizontal = vec2<f32>(dot(delta, params.east), dot(delta, params.north));
    let uv         = horizontal / (2.0 * params.extent) + 0.5;

    // Elevation above the ellipsoid, reconstructed from the tangent frame with the
    // spherical curvature drop added back.
    let elevation = dot(delta, params.up) + dot(horizontal, horizontal) * params.curvature;

    let shadow     = textureSampleLevel(map, map_sampler, uv, 0.0);
    let ray_length = shadow.g * params.max_distance;
    let penumbra   = max(params.min_penumbra, ray_length * params.penumbra_scale);
    let visibility = smoothstep(0.0, 1.0, (elevation - shadow.r) / penumbra + 0.5);

    // Fade out towards the edge of the map, so shadows vanish smoothly instead of
    // being cut off.
    let border = max(abs(uv.x - 0.5), abs(uv.y - 0.5)) * 2.0;
    let fade   = 1.0 - smoothstep(0.9, 1.0, border);

    return mix(1.0, visibility, fade);
}
