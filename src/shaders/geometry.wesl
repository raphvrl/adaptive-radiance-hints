import super::palette::vertex_color;

const AMBIENT_STRENGTH: f32 = 0.1;

struct FrameUniform {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> frame: FrameUniform;

struct GpuLight {
    position: vec3f,
    range: f32,

    direction: vec3f,
    inner_cos: f32,

    color: vec3f,
    intensity: f32,

    outer_cos: f32,
    is_spot: u32,
}

struct Lights {
    length: u32,
    lights: array<GpuLight>,
}

@group(1) @binding(0)
var<storage, read> light_buffer: Lights;

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
    @location(3) world_position: vec3f,
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

    let world_position = model * vec4f(vertex.position, 1.0);
    output.position = frame.view_proj * world_position;
    output.normal = normal_matrix * vertex.normal;
    output.albedo = instance.albedo.rgb;
    output.emission = instance.emission.rgb;
    output.world_position = world_position.xyz;

    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    let normal = normalize(input.normal);
    var color = input.emission + input.albedo * AMBIENT_STRENGTH;

    for (var i = 0u; i < light_buffer.length; i += 1u) {
        let light = light_buffer.lights[i];
        let to_light = light.position - input.world_position;
        let light_distance = length(to_light);
        let range = light.range;

        if light_distance > 0.0001 && light_distance < range {
            let direction_to_light = to_light / light_distance;
            let diffuse = max(dot(normal, direction_to_light), 0.0);
            let range_falloff = 1.0 - light_distance / range;
            let attenuation = range_falloff * range_falloff / (1.0 + light_distance * light_distance);
            var spot_factor = 1.0;

            if light.is_spot != 0u {
                let cosine = dot(-direction_to_light, normalize(light.direction));
                let inner_cos = light.inner_cos;
                let outer_cos = light.outer_cos;
                let cone = clamp(
                    (cosine - outer_cos) / max(inner_cos - outer_cos, 0.0001),
                    0.0,
                    1.0,
                );
                spot_factor = cone * cone * (3.0 - 2.0 * cone);
            }

            color += input.albedo
                * light.color
                * light.intensity
                * diffuse
                * attenuation
                * spot_factor;
        }
    }

    return vec4f(color, 1.0);
}
