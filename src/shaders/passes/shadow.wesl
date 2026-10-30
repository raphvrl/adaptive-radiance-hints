struct ShadowFrame {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> shadow_frame: ShadowFrame;

struct VertexInput {
    @location(0) position: vec3f,
}

struct Instance {
    @location(2) model_0: vec4f,
    @location(3) model_1: vec4f,
    @location(4) model_2: vec4f,
    @location(5) model_3: vec4f,
}

struct VertexOutput {
    @builtin(position) position: vec4f,
}

@vertex
fn vs_main(vertex: VertexInput, instance: Instance) -> VertexOutput {
    var output: VertexOutput;

    let model = mat4x4<f32>(
        instance.model_0,
        instance.model_1,
        instance.model_2,
        instance.model_3,
    );

    output.position = shadow_frame.view_proj * model * vec4f(vertex.position, 1.0);

    return output;
}
