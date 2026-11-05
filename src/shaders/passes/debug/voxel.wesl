const VOXELS_PER_CHUNK: u32 = 8u;
const CHUNKS_PER_AXIS: u32 = 8u;
const VOXELS_PER_AXIS: u32 = VOXELS_PER_CHUNK * CHUNKS_PER_AXIS;
const WORDS_PER_CHUNK: u32 = VOXELS_PER_CHUNK * VOXELS_PER_CHUNK * VOXELS_PER_CHUNK / 32u;
const BLOCKS_PER_AXIS: u32 = VOXELS_PER_CHUNK / 4u;

struct FrameUniform {
    view_proj: mat4x4<f32>,
}

struct VoxelParams {
    dirty_chunks: array<vec4u, 4>,
    ring_offset: vec4u,
    region_origin: vec4f,
}

@group(0) @binding(0)
var<uniform> frame: FrameUniform;

@group(1) @binding(0)
var<storage, read> occupancy: array<u32>;

@group(2) @binding(0)
var<uniform> params: VoxelParams;

@group(3) @binding(0)
var<uniform> finer_params: VoxelParams;

struct VertexInput {
    @location(0) local_position: vec3f,
}

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) @interpolate(flat) cascade_index: u32,
}

fn voxel_is_occupied(voxel_index: u32) -> bool {
    let voxel = vec3u(
        voxel_index % VOXELS_PER_AXIS,
        (voxel_index / VOXELS_PER_AXIS) % VOXELS_PER_AXIS,
        voxel_index / (VOXELS_PER_AXIS * VOXELS_PER_AXIS),
    );
    let chunk = voxel / vec3u(VOXELS_PER_CHUNK);
    let local_voxel = voxel % vec3u(VOXELS_PER_CHUNK);
    let physical_chunk = (chunk + params.ring_offset.xyz) % vec3u(CHUNKS_PER_AXIS);
    let chunk_index = physical_chunk.x
        + CHUNKS_PER_AXIS * physical_chunk.y
        + CHUNKS_PER_AXIS * CHUNKS_PER_AXIS * physical_chunk.z;
    let block = (local_voxel.x >> 2u)
        + BLOCKS_PER_AXIS * (local_voxel.y >> 2u)
        + BLOCKS_PER_AXIS * BLOCKS_PER_AXIS * (local_voxel.z >> 2u);
    let bit = (local_voxel.x & 3u)
        + 4u * (local_voxel.y & 3u)
        + 16u * (local_voxel.z & 3u);
    let word_index = chunk_index * WORDS_PER_CHUNK + block * 2u + bit / 32u;

    return (occupancy[word_index] & (1u << (bit % 32u))) != 0u;
}

@vertex
fn vs_main(input: VertexInput, @builtin(instance_index) instance_index: u32) -> VertexOutput {
    let voxel_count = VOXELS_PER_AXIS * VOXELS_PER_AXIS * VOXELS_PER_AXIS;
    let voxel_index = instance_index % voxel_count;
    var output: VertexOutput;
    output.cascade_index = instance_index / voxel_count;
    if !voxel_is_occupied(voxel_index) {
        output.position = vec4f(2.0, 2.0, 2.0, 1.0);
        return output;
    }

    let voxel = vec3u(
        voxel_index % VOXELS_PER_AXIS,
        (voxel_index / VOXELS_PER_AXIS) % VOXELS_PER_AXIS,
        voxel_index / (VOXELS_PER_AXIS * VOXELS_PER_AXIS),
    );
    let center = params.region_origin.xyz + (vec3f(voxel) + vec3f(0.5)) * params.region_origin.w;
    let finer_origin = finer_params.region_origin.xyz;
    let finer_max = finer_origin + vec3f(f32(VOXELS_PER_AXIS) * finer_params.region_origin.w);
    if output.cascade_index > 0u && all(center >= finer_origin) && all(center < finer_max) {
        output.position = vec4f(2.0, 2.0, 2.0, 1.0);
        return output;
    }
    let world_position = center + input.local_position * (params.region_origin.w * 0.5);
    output.position = frame.view_proj * vec4f(world_position, 1.0);

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    if input.cascade_index == 0u {
        return vec4f(0.05, 0.9, 1.0, 0.9);
    }
    return vec4f(1.0, 0.55, 0.1, 0.9);
}
