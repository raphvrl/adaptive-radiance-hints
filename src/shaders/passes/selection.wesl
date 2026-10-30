struct FrameUniform {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> frame: FrameUniform;

struct SelectionUniform {
    model: mat4x4<f32>,
}

@group(1) @binding(0)
var<uniform> selection: SelectionUniform;

struct VertexInput {
    @location(0) position: vec3f,
}

@vertex
fn vs_main(input: VertexInput) -> @builtin(position) vec4f {
    return frame.view_proj * selection.model * vec4f(input.position, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4f {
    return vec4f(1.0, 1.0, 1.0, 1.0);
}
