struct TileParams {
    tiles_x: u32,
    tiles_y: u32,
    tile_size: u32,
    max_lights_per_tile: u32,
}

@group(0) @binding(0)
var<storage, read> tile_data: array<u32>;

@group(0) @binding(1)
var<uniform> tile_params: TileParams;

struct VertexOutput {
    @builtin(position) position: vec4f,
}

fn thermal_color(value: f32) -> vec3f {
    let heat = clamp(value, 0.0, 1.0);

    if heat < 0.25 {
        return mix(vec3f(0.015, 0.01, 0.12), vec3f(0.02, 0.16, 0.95), heat * 4.0);
    }
    if heat < 0.5 {
        return mix(vec3f(0.02, 0.16, 0.95), vec3f(0.0, 0.9, 0.7), (heat - 0.25) * 4.0);
    }
    if heat < 0.75 {
        return mix(vec3f(0.0, 0.9, 0.7), vec3f(1.0, 0.88, 0.02), (heat - 0.5) * 4.0);
    }

    return mix(vec3f(1.0, 0.88, 0.02), vec3f(1.0, 0.04, 0.01), (heat - 0.75) * 4.0);
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array<vec2f, 3>(
        vec2f(-1.0, -1.0),
        vec2f(3.0, -1.0),
        vec2f(-1.0, 3.0),
    );
    var output: VertexOutput;
    output.position = vec4f(positions[vertex_index], 0.0, 1.0);
    return output;
}

@fragment
fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let pixel = vec2u(position.xy);
    let tile_xy = pixel / tile_params.tile_size;
    let tile_index = tile_xy.y * tile_params.tiles_x + tile_xy.x;
    let header_offset = tile_index * 2u;
    let light_count = tile_data[header_offset];
    let overflow = tile_data[header_offset + 1u];

    let heat = log2(f32(light_count) + 1.0) / log2(f32(tile_params.max_lights_per_tile) + 1.0);
    var color = thermal_color(heat);
    var alpha = 0.52;

    if overflow != 0u {
        color = vec3f(1.0);
        alpha = 0.85;
    }

    return vec4f(color, alpha);
}
