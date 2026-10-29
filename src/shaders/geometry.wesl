import super::palette::vertex_color;

struct FrameUniform {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> frame: FrameUniform;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
}

struct Instance {
    @location(2) model_0: vec4f,
    @location(3) model_1: vec4f,
    @location(4) model_2: vec4f,
    @location(5) model_3: vec4f,

    @location(6) normal_0: vec4f,
    @location(7) normal_1: vec4f,
    @location(8) normal_2: vec4f,

    @location(9) albedo: vec4f,
    @location(10) emission: vec4f,
}

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) normal: vec3f,
    @location(1) albedo: vec3f,
    @location(2) emission: vec3f,
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

    let normal_matrix = mat3x3<f32>(
        instance.normal_0.xyz,
        instance.normal_1.xyz,
        instance.normal_2.xyz,
    );

    output.position = frame.view_proj * model * vec4f(vertex.position, 1.0);
    output.normal = normal_matrix * vertex.normal;
    output.albedo = instance.albedo.rgb;
    output.emission = instance.emission.rgb;

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    let light_dir = normalize(vec3f(0.4, 0.7, 0.5));
    let light = 0.32 + 0.68 * max(dot(normalize(input.normal), light_dir), 0.0);
    let color = input.albedo * light + input.emission;
    return vec4f(color, 1.0);
}
