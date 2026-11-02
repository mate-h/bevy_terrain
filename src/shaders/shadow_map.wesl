// Generates the RDR2-style terrain shadow map: for every texel of a small top-down
// map centered on the view, the terrain height map is raymarched towards the sun and
// the highest occluding elevation (R) plus the ray length to that occluder, as a
// fraction of `max_distance` (G), are stored. At shading time fragments compare
// their elevation against R and use G to widen the penumbra for cheap soft shadows.

import bevy_terrain::shaders::bindings::{terrain_data, terrain_shadow, shadow_map_out};
import bevy_terrain::shaders::heightfield::sample_height_at_unit;

// Elevation sentinel meaning "no occluder" (well below any terrain, fits rg16float).
const NO_OCCLUDER: f32 = -60000.0;
// Elevation meaning "everything is occluded" (well above any terrain, fits rg16float).
const FULLY_OCCLUDED: f32 = 60000.0;

@compute @workgroup_size(8, 8, 1)
fn compute_shadow_map(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let size = u32(terrain_shadow.map_size);
    if (invocation.x >= size || invocation.y >= size) { return; }

    // Horizontal offset of this texel from the map center, in meters.
    let uv = (vec2<f32>(invocation.xy) + 0.5) / terrain_shadow.map_size;
    let origin = (uv * 2.0 - 1.0) * terrain_shadow.extent;

    // Decompose the sun direction into the local tangent frame of the map center.
    let up        = terrain_shadow.up;
    let sun_up    = dot(terrain_shadow.light_direction, up);
    let horizon   = terrain_shadow.light_direction - sun_up * up;
    let horizon_length = length(horizon);

    var intersection_height = NO_OCCLUDER;
    var ray_length          = terrain_shadow.max_distance;

    if (horizon_length > 1e-4) {
        let slope = sun_up / horizon_length;
        let direction = vec2<f32>(dot(horizon, terrain_shadow.east),
                                  dot(horizon, terrain_shadow.north)) / horizon_length;

        // Exponentially growing steps out to the max scan distance, but never shorter
        // than a texel: closer samples would only resolve detail the map cannot hold.
        // Start beyond the texel's own cell, so the coarse height there cannot
        // self-shadow fragments that sit below the cell average (close-range shadows
        // are handled by the regular shadow maps).
        let texel_size = terrain_shadow.texel_size;
        let start      = 1.5 * texel_size;
        let count      = terrain_shadow.steps;
        let growth     = pow(terrain_shadow.max_distance / start, 1.0 / f32(count - 1u));

        let max_height = terrain_data.terrain.max_height;

        // The ray is a straight line in world space, so its unit space position is
        // affine in t and the transform can be hoisted out of the loop.
        let unit_from_world = terrain_shadow.unit_from_world;
        let origin_world    = terrain_shadow.center
            + origin.x * terrain_shadow.east + origin.y * terrain_shadow.north;
        let direction_world = direction.x * terrain_shadow.east + direction.y * terrain_shadow.north;
        let unit_origin     = (unit_from_world * vec4<f32>(origin_world, 1.0)).xyz;
        let unit_direction  = (unit_from_world * vec4<f32>(direction_world, 0.0)).xyz;

        var t = start;
        for (var i = 0u; i < count && t <= terrain_shadow.max_distance; i = i + 1u) {
            // Elevation the occluder at distance t has to exceed to shadow this texel:
            // the spherical curvature drop plus the elevation the sun ray gains while
            // traveling the horizontal distance t.
            let drop = t * t * terrain_shadow.curvature + t * slope;

            // The occlusion height of any remaining sample is at most the highest
            // terrain minus the drop, which only grows with t when the sun is above
            // the horizon. Once that bound cannot beat the current occluder, the
            // result is final.
            if (slope >= 0.0 && max_height - drop <= intersection_height) { break; }

            let height = sample_height_at_unit(unit_origin + t * unit_direction, terrain_shadow.lod);
            let occlusion_height = height - drop;

            if (occlusion_height > intersection_height) {
                intersection_height = occlusion_height;
                ray_length          = t;
            }

            t = max(t * growth, t + texel_size);
        }
    } else if (sun_up < 0.0) {
        // Sun pointing straight down: everything is occluded.
        intersection_height = FULLY_OCCLUDED;
        ray_length          = terrain_shadow.texel_size;
    }

    // A light below the horizon raises the occlusion height without bound. Past
    // rg16float's range (+-65504) the store becomes Inf, which decodes to NaN.
    textureStore(shadow_map_out, vec2<i32>(invocation.xy),
                 vec4<f32>(min(intersection_height, FULLY_OCCLUDED),
                           ray_length / terrain_shadow.max_distance, 0.0, 0.0));
}
