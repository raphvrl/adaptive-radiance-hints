import super::palette::vertex_color;

struct FrameUniform {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> frame: FrameUniform;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) model_0: vec4f,
    @location(3) model_1: vec4f,
    @location(4) model_2: vec4f,
    @location(5) model_3: vec4f,
    @location(6) normal_0: vec4f,
    @location(7) normal_1: vec4f,
    @location(8) normal_2: vec4f,
}

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) normal: vec3f,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;

    let model = mat4x4<f32>(
        input.model_0,
        input.model_1,
        input.model_2,
        input.model_3,
    );

    let normal_matrix = mat3x3<f32>(
        input.normal_0.xyz,
        input.normal_1.xyz,
        input.normal_2.xyz,
    );

    output.position = frame.view_proj * model * vec4f(input.position, 1.0);
    output.normal = normal_matrix * input.normal;

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    let light_dir = normalize(vec3f(0.4, 0.7, 0.5));
    let light = 0.2 + 0.8 * max(dot(normalize(input.normal), light_dir), 0.0);
    let color = vec3f(0.2, 0.65, 0.95) * light;
    return vec4f(color, 1.0);
}
