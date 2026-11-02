
import bevy_terrain::shaders::types::{Coordinate, INVALID_ATLAS_INDEX};
import bevy_terrain::shaders::bindings::{terrain_data, terrain_sampler, height_attachment};
import bevy_terrain::shaders::functions::lookup_tile_tree_entry;

const HEIGHTFIELD_SIGMA: f32 = 0.87 * 0.87;

// Inverse of the cube-sphere projection in `Coordinate::from_unit_position`.
fn unit_to_coordinate(unit: vec3<f32>) -> Coordinate {
    @if(SPHERICAL) {
        let abs_u = abs(unit);
        var face: u32;
        var xy: vec2<f32>;

        if (abs_u.x >= abs_u.y && abs_u.x >= abs_u.z) {
            if (unit.x < 0.0) {
                face = 0u;
                xy = vec2(unit.z, -unit.y) / abs_u.x;
            } else {
                face = 3u;
                xy = vec2(-unit.y, unit.z) / abs_u.x;
            }
        } else if (abs_u.y >= abs_u.x && abs_u.y >= abs_u.z) {
            if (unit.y < 0.0) {
                face = 5u;
                xy = vec2(unit.z, unit.x) / abs_u.y;
            } else {
                face = 2u;
                xy = vec2(unit.x, unit.z) / abs_u.y;
            }
        } else if (unit.z < 0.0) {
            face = 4u;
            xy = vec2(-unit.y, unit.x) / abs_u.z;
        } else {
            face = 1u;
            xy = vec2(unit.x, -unit.y) / abs_u.z;
        }

        let uv = clamp(
            0.5 * xy * sqrt((1.0 + HEIGHTFIELD_SIGMA) / (1.0 + HEIGHTFIELD_SIGMA * xy * xy)) + 0.5,
            vec2(0.0),
            vec2(1.0),
        );

        return Coordinate(face, 0u, vec2<u32>(0u), uv);
    } @else {
        let uv = clamp(unit.xz + 0.5, vec2(0.0), vec2(1.0));
        return Coordinate(0u, 0u, vec2<u32>(0u), uv);
    }
}

// Samples the terrain height (meters above the ellipsoid) at an arbitrary world position.
//
// A single fixed `lod` must be used for all samples of a pass. It has to be coarse
// enough that the (viewer-centered) tile tree window at that lod covers every sampled
// position; otherwise the wrapping tree lookup would return unrelated tiles.
fn sample_height_at_world(world_position: vec3<f32>, unit_from_world: mat4x4<f32>, lod: u32) -> f32 {
    return sample_height_at_unit((unit_from_world * vec4<f32>(world_position, 1.0)).xyz, lod);
}

// Like `sample_height_at_world`, for a position already transformed into terrain unit
// space by `unit_from_world` (for spherical terrains, not yet projected onto the unit
// cube sphere).
fn sample_height_at_unit(unit_position: vec3<f32>, lod: u32) -> f32 {
    @if(SPHERICAL)
    let unit = normalize(unit_position);
    @if(!(SPHERICAL))
    let unit = unit_position;

    let coordinate = unit_to_coordinate(unit);

    // The coordinate is at lod 0, where uv spans the whole face, so the tile tree
    // entry and the atlas uv follow from scaling uv by a power of two. This is
    // equivalent to (and much cheaper than) the general `lookup_tile`.
    let tree_xy = vec2<u32>(coordinate.uv * f32(1u << lod));
    let entry   = lookup_tile_tree_entry(Coordinate(coordinate.face, lod, tree_xy, vec2<f32>(0.0)));

    // Outside the dataset (no tile at any lod) there is no terrain: report the
    // lowest height instead of sampling garbage, so the void never casts shadows.
    if (entry.atlas_index == INVALID_ATLAS_INDEX) { return terrain_data.terrain.min_height; }

    let tile_uv = fract(coordinate.uv * f32(1u << entry.atlas_lod));

    return sample_height_gathered(entry.atlas_index, tile_uv);
}

// Bilinear height at `tile_uv` of an atlas tile from a single gather, which also
// serves the mask test (the low bit of each raw texel), instead of a gather plus a
// filtered sample. Masked-out texels (outside the dataset at the edge of a tile)
// report the lowest height.
fn sample_height_gathered(atlas_index: u32, tile_uv: vec2<f32>) -> f32 {
    let attachment = terrain_data.attachments.height;
    let uv         = tile_uv * attachment.scale + attachment.offset;
    let texels     = textureGather(0, height_attachment, terrain_sampler, uv, atlas_index);

    if (attachment.mask != 0 && any((bitcast<vec4<u32>>(texels) & vec4<u32>(1)) == vec4<u32>(0))) {
        return terrain_data.terrain.min_height;
    }

    // Gather order: (u0, v1), (u1, v1), (u1, v0), (u0, v0).
    let weight = fract(uv * vec2<f32>(textureDimensions(height_attachment)) - 0.5);
    let top    = mix(texels.w, texels.z, weight.x);
    let bottom = mix(texels.x, texels.y, weight.x);

    return terrain_data.terrain.height_scale * mix(top, bottom, weight.y);
}
