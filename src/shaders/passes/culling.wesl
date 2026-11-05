import package::shared::lights::Lights;

const TILE_SIZE: u32 = 16u;
const TILE_LANES: u32 = TILE_SIZE * TILE_SIZE;

struct CullParams {
    view_proj: mat4x4<f32>,
    viewport_tiles: vec4u,
}

@group(0) @binding(0)
var<uniform> cull: CullParams;

@group(1) @binding(0)
var<storage, read> light_buffer: Lights;

@group(2) @binding(0)
var scene_depth: texture_depth_2d;

struct TileParams {
    tiles_x: u32,
    tiles_y: u32,
    tile_size: u32,
    max_lights_per_tile: u32,
}

@group(3) @binding(0)
var<storage, read_write> tile_data: array<u32>;

@group(3) @binding(1)
var<uniform> tile_params: TileParams;

struct NdcBounds {
    minimum: vec3f,
    maximum: vec3f,
    visible: u32,
}

var<workgroup> tile_min_depth_bits: atomic<u32>;
var<workgroup> tile_max_depth_bits: atomic<u32>;
var<workgroup> tile_light_count: atomic<u32>;

fn ratio_bounds(center: f32, extent: f32, w: f32, w_extent: f32) -> vec2f {
    let w_near = w - w_extent;
    let w_far = w + w_extent;
    let a = (center - extent) / w_near;
    let b = (center - extent) / w_far;
    let c = (center + extent) / w_near;
    let d = (center + extent) / w_far;

    return vec2f(min(min(a, b), min(c, d)), max(max(a, b), max(c, d)));
}

fn projected_sphere(center: vec3f, radius: f32) -> NdcBounds {
    var bounds: NdcBounds;
    bounds.minimum = vec3f(-1.0, -1.0, 0.0);
    bounds.maximum = vec3f(1.0, 1.0, 1.0);
    bounds.visible = 1u;

    let matrix = cull.view_proj;
    let center_clip = matrix * vec4f(center, 1.0);
    let row_x = vec3f(matrix[0].x, matrix[1].x, matrix[2].x);
    let row_y = vec3f(matrix[0].y, matrix[1].y, matrix[2].y);
    let row_z = vec3f(matrix[0].z, matrix[1].z, matrix[2].z);
    let row_w = vec3f(matrix[0].w, matrix[1].w, matrix[2].w);
    let extent_x = radius * length(row_x);
    let extent_y = radius * length(row_y);
    let extent_z = radius * length(row_z);
    let extent_w = radius * length(row_w);
    let w_near = center_clip.w - extent_w;
    let w_far = center_clip.w + extent_w;

    if w_far <= 0.0 {
        bounds.visible = 0u;
        return bounds;
    }

    if w_near <= 0.0 {
        return bounds;
    }

    let x_bounds = ratio_bounds(center_clip.x, extent_x, center_clip.w, extent_w);
    let y_bounds = ratio_bounds(center_clip.y, extent_y, center_clip.w, extent_w);
    let z_bounds = ratio_bounds(center_clip.z, extent_z, center_clip.w, extent_w);
    bounds.minimum = max(vec3f(x_bounds.x, y_bounds.x, z_bounds.x), vec3f(-1.0, -1.0, 0.0));
    bounds.maximum = min(vec3f(x_bounds.y, y_bounds.y, z_bounds.y), vec3f(1.0, 1.0, 1.0));
    bounds.visible = select(0u, 1u, all(bounds.minimum <= bounds.maximum));

    return bounds;
}

fn overlaps_tile(
    bounds: NdcBounds,
    tile_minimum: vec2f,
    tile_maximum: vec2f,
    depth_minimum: f32,
    depth_maximum: f32,
) -> bool {
    return bounds.visible != 0u
        && bounds.minimum.x <= tile_maximum.x
        && bounds.maximum.x >= tile_minimum.x
        && bounds.minimum.y <= tile_maximum.y
        && bounds.maximum.y >= tile_minimum.y
        && bounds.minimum.z <= depth_maximum
        && bounds.maximum.z >= depth_minimum;
}

@compute @workgroup_size(16, 16, 1)
fn cull_tiles(
    @builtin(workgroup_id) workgroup_id: vec3u,
    @builtin(local_invocation_index) lane: u32,
    @builtin(global_invocation_id) global_id: vec3u,
) {
    if lane == 0u {
        atomicStore(&tile_min_depth_bits, bitcast<u32>(1.0));
        atomicStore(&tile_max_depth_bits, bitcast<u32>(0.0));
        atomicStore(&tile_light_count, 0u);
    }
    workgroupBarrier();

    let pixel_in_bounds = global_id.x < cull.viewport_tiles.x
        && global_id.y < cull.viewport_tiles.y;
    if pixel_in_bounds {
        let depth = textureLoad(scene_depth, vec2i(global_id.xy), 0);
        atomicMin(&tile_min_depth_bits, bitcast<u32>(depth));
        atomicMax(&tile_max_depth_bits, bitcast<u32>(depth));
    }
    workgroupBarrier();

    let tile_index = workgroup_id.y * cull.viewport_tiles.z + workgroup_id.x;
    let header_offset = tile_index * 2u;
    let depth_minimum = bitcast<f32>(atomicLoad(&tile_min_depth_bits));
    let depth_maximum = bitcast<f32>(atomicLoad(&tile_max_depth_bits));

    let tile_size = tile_params.tile_size;
    let viewport = cull.viewport_tiles.xy;
    let tile_origin = workgroup_id.xy * tile_size;
    let tile_end = min(tile_origin + vec2u(tile_size), viewport);
    let viewport_f = vec2f(viewport);
    let tile_minimum = vec2f(
        2.0 * f32(tile_origin.x) / viewport_f.x - 1.0,
        1.0 - 2.0 * f32(tile_end.y) / viewport_f.y,
    );
    let tile_maximum = vec2f(
        2.0 * f32(tile_end.x) / viewport_f.x - 1.0,
        1.0 - 2.0 * f32(tile_origin.y) / viewport_f.y,
    );

    for (var base = 0u; base < light_buffer.length; base += TILE_LANES) {
        let light_index = base + lane;
        if light_index < light_buffer.length {
            let light = light_buffer.lights[light_index];
            let bounds = projected_sphere(light.position, light.range);

            if overlaps_tile(bounds, tile_minimum, tile_maximum, depth_minimum, depth_maximum) {
                let slot = atomicAdd(&tile_light_count, 1u);
                if slot < tile_params.max_lights_per_tile {
                    let indices_offset = cull.viewport_tiles.z * cull.viewport_tiles.w * 2u
                        + tile_index * tile_params.max_lights_per_tile;
                    tile_data[indices_offset + slot] = light_index;
                }
            }
        }
    }

    storageBarrier();
    workgroupBarrier();

    if lane == 0u {
        let count = atomicLoad(&tile_light_count);
        let capacity = tile_params.max_lights_per_tile;
        tile_data[header_offset] = min(count, capacity);
        tile_data[header_offset + 1u] = select(0u, 1u, count > capacity);
    }
}
