const VOXELS_PER_CHUNK: u32 = 8u;
const CHUNKS_PER_AXIS: u32 = 8u;
const VOXELS_PER_AXIS: i32 = i32(VOXELS_PER_CHUNK * CHUNKS_PER_AXIS);
const WORDS_PER_CHUNK: u32 = VOXELS_PER_CHUNK * VOXELS_PER_CHUNK * VOXELS_PER_CHUNK / 32u;
const BLOCKS_PER_AXIS: u32 = VOXELS_PER_CHUNK / 4u;

struct VoxelParams {
    dirty_chunks: array<vec4u, 4>,
    ring_offset: vec4u,
    region_origin: vec4f,
}

struct VoxelTriangle {
    a: vec4f,
    b: vec4f,
    c: vec4f,
}

@group(0) @binding(0)
var<uniform> params: VoxelParams;

@group(0) @binding(1)
var<storage, read> dirty_indices: array<u32>;

@group(1) @binding(0)
var<storage, read_write> occupancy: array<atomic<u32>>;

@group(2) @binding(0)
var<storage, read> triangles: array<VoxelTriangle>;

@group(3) @binding(0)
var<storage, read> models: array<mat4x4<f32>>;

@compute @workgroup_size(16, 1, 1)
fn clear_dirty_chunks(@builtin(global_invocation_id) global_id: vec3u) {
    let word_index = global_id.x;
    let chunk_index = dirty_indices[global_id.y];
    if word_index >= WORDS_PER_CHUNK
        || chunk_index >= CHUNKS_PER_AXIS * CHUNKS_PER_AXIS * CHUNKS_PER_AXIS
    {
        return;
    }

    if chunk_is_dirty(chunk_index) {
        let logical_chunk = vec3u(
            chunk_index % CHUNKS_PER_AXIS,
            (chunk_index / CHUNKS_PER_AXIS) % CHUNKS_PER_AXIS,
            chunk_index / (CHUNKS_PER_AXIS * CHUNKS_PER_AXIS),
        );
        let physical_chunk = (logical_chunk + params.ring_offset.xyz) % vec3u(CHUNKS_PER_AXIS);
        let physical_index = physical_chunk.x
            + CHUNKS_PER_AXIS * physical_chunk.y
            + CHUNKS_PER_AXIS * CHUNKS_PER_AXIS * physical_chunk.z;
        atomicStore(&occupancy[physical_index * WORDS_PER_CHUNK + word_index], 0u);
    }
}

@compute @workgroup_size(64, 1, 1)
fn voxelize_triangles(@builtin(global_invocation_id) global_id: vec3u) {
    let triangle_index = global_id.x;
    let chunk_index = dirty_indices[global_id.y];
    let instance_index = global_id.z;
    if triangle_index >= arrayLength(&triangles)
        || instance_index >= arrayLength(&models)
        || chunk_index >= CHUNKS_PER_AXIS * CHUNKS_PER_AXIS * CHUNKS_PER_AXIS
    {
        return;
    }
    if !chunk_is_dirty(chunk_index) {
        return;
    }

    let model = models[instance_index];
    let triangle = triangles[triangle_index];
    let a = (model * triangle.a).xyz;
    let b = (model * triangle.b).xyz;
    let c = (model * triangle.c).xyz;
    let triangle_min = min(min(a, b), c);
    let triangle_max = max(max(a, b), c);
    let region_origin = params.region_origin.xyz;
    let voxel_size = params.region_origin.w;
    let region_max = region_origin + vec3f(f32(VOXELS_PER_AXIS) * voxel_size);
    if any(triangle_max < region_origin) || any(triangle_min > region_max) {
        return;
    }

    let candidate_min = vec3i(clamp(
        floor((triangle_min - region_origin) / voxel_size) - vec3f(1.0),
        vec3f(0.0),
        vec3f(f32(VOXELS_PER_AXIS - 1)),
    ));
    let candidate_max = vec3i(clamp(
        floor((triangle_max - region_origin) / voxel_size),
        vec3f(0.0),
        vec3f(f32(VOXELS_PER_AXIS - 1)),
    ));
    let chunk_coordinates = vec3i(
        i32(chunk_index % CHUNKS_PER_AXIS),
        i32((chunk_index / CHUNKS_PER_AXIS) % CHUNKS_PER_AXIS),
        i32(chunk_index / (CHUNKS_PER_AXIS * CHUNKS_PER_AXIS)),
    );
    let chunk_min = chunk_coordinates * i32(VOXELS_PER_CHUNK);
    let chunk_max = chunk_min + vec3i(i32(VOXELS_PER_CHUNK) - 1);
    let physical_chunk_coordinates =
        (vec3u(chunk_coordinates) + params.ring_offset.xyz) % vec3u(CHUNKS_PER_AXIS);
    let physical_chunk_index = physical_chunk_coordinates.x
        + CHUNKS_PER_AXIS * physical_chunk_coordinates.y
        + CHUNKS_PER_AXIS * CHUNKS_PER_AXIS * physical_chunk_coordinates.z;
    let voxel_minimum = max(candidate_min, chunk_min);
    let voxel_maximum = min(candidate_max, chunk_max);
    if any(voxel_minimum > voxel_maximum) {
        return;
    }

    for (var z = voxel_minimum.z; z <= voxel_maximum.z; z += 1) {
        for (var y = voxel_minimum.y; y <= voxel_maximum.y; y += 1) {
            for (var x = voxel_minimum.x; x <= voxel_maximum.x; x += 1) {
                let cell_min = region_origin + vec3f(f32(x), f32(y), f32(z)) * voxel_size;
                let cell_max = cell_min + vec3f(voxel_size);

                if triangle_intersects_box(a, b, c, cell_min, cell_max) {
                    mark_occupied(physical_chunk_index, vec3u(u32(x), u32(y), u32(z)));
                }
            }
        }
    }
}

fn chunk_is_dirty(chunk_index: u32) -> bool {
    let word_index = chunk_index / 32u;
    return (params.dirty_chunks[word_index / 4u][word_index % 4u] & (1u << (chunk_index % 32u))) != 0u;
}

fn mark_occupied(chunk_index: u32, voxel: vec3u) {
    let local_voxel = voxel % vec3u(VOXELS_PER_CHUNK);
    let block = (local_voxel.x >> 2u)
        + BLOCKS_PER_AXIS * (local_voxel.y >> 2u)
        + BLOCKS_PER_AXIS * BLOCKS_PER_AXIS * (local_voxel.z >> 2u);
    let bit = (local_voxel.x & 3u)
        + 4u * (local_voxel.y & 3u)
        + 16u * (local_voxel.z & 3u);
    let word_index = chunk_index * WORDS_PER_CHUNK + block * 2u + bit / 32u;
    atomicOr(&occupancy[word_index], 1u << (bit % 32u));
}

fn triangle_intersects_box(a: vec3f, b: vec3f, c: vec3f, minimum: vec3f, maximum: vec3f) -> bool {
    let center = (minimum + maximum) * 0.5;
    let half_size = (maximum - minimum) * 0.5;
    let v0 = a - center;
    let v1 = b - center;
    let v2 = c - center;
    let edges = array<vec3f, 3>(v1 - v0, v2 - v1, v0 - v2);
    let box_axes = array<vec3f, 3>(
        vec3f(1.0, 0.0, 0.0),
        vec3f(0.0, 1.0, 0.0),
        vec3f(0.0, 0.0, 1.0),
    );

    for (var axis_index = 0u; axis_index < 3u; axis_index += 1u) {
        if separates_on_axis(box_axes[axis_index], v0, v1, v2, half_size) {
            return false;
        }
    }

    for (var edge_index = 0u; edge_index < 3u; edge_index += 1u) {
        for (var axis_index = 0u; axis_index < 3u; axis_index += 1u) {
            if separates_on_axis(
                cross(edges[edge_index], box_axes[axis_index]),
                v0,
                v1,
                v2,
                half_size,
            ) {
                return false;
            }
        }
    }

    return !separates_on_axis(cross(edges[0], edges[1]), v0, v1, v2, half_size);
}

fn separates_on_axis(axis: vec3f, a: vec3f, b: vec3f, c: vec3f, half_size: vec3f) -> bool {
    if dot(axis, axis) <= 1.0e-12 {
        return false;
    }

    let projection_a = dot(a, axis);
    let projection_b = dot(b, axis);
    let projection_c = dot(c, axis);
    let triangle_min = min(min(projection_a, projection_b), projection_c);
    let triangle_max = max(max(projection_a, projection_b), projection_c);
    let box_radius = dot(half_size, abs(axis));

    return triangle_min > box_radius || triangle_max < -box_radius;
}
