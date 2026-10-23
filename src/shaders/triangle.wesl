import super::palette::vertex_color;

struct FrameUniform {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> frame: FrameUniform;

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) color: vec3f,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    let positions = array<vec3f, 3>(
        vec3f(0.0, 0.8, 0.0),
        vec3f(-0.8, -0.6, 0.0),
        vec3f(0.8, -0.6, 0.0),
    );

    var out: VertexOutput;
    out.position = frame.view_proj * vec4f(positions[index], 1.0);
    out.color = vertex_color(index);
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    return vec4f(input.color, 1.0);
}
