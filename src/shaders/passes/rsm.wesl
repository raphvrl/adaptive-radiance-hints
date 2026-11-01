const PI: f32 = 3.14159265359;

struct RsmFrame {
    view_proj: mat4x4<f32>,
    light_index: u32,
}

@group(0) @binding(0)
var<uniform> rsm_frame: RsmFrame;

struct GpuLight {
    position: vec3f,
    range: f32,

    direction: vec3f,
    inner_cos: f32,

    color: vec3f,
    intensity: f32,

    outer_cos: f32,
    is_spot: u32,
    shadow_index: u32,
    shadow_normal_bias: f32,
    shadow_resolution: u32,
    shadow_bias: f32,
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

    @location(9) base_color: vec4f,
    @location(11) metallic_roughness: vec2f,
}

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) world_position: vec3f,
    @location(1) world_normal: vec3f,
    @location(2) base_color: vec3f,
    @location(3) metallic: f32,
}

@vertex
fn vs_main(vertex: VertexInput, instance: Instance) -> VertexOutput {
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

    var output: VertexOutput;
    output.position = rsm_frame.view_proj * world_position;
    output.world_position = world_position.xyz;
    output.world_normal = normal_matrix * vertex.normal;
    output.base_color = instance.base_color.rgb;
    output.metallic = instance.metallic_roughness.x;

    return output;
}

struct RsmOutput {
    @location(0) normal: vec4f,
    @location(1) flux: vec4f,
}

@fragment
fn fs_main(input: VertexOutput) -> RsmOutput {
    let normal = normalize(input.world_normal);

    var flux = vec3f(0.0);

    if rsm_frame.light_index < light_buffer.length {
        let light = light_buffer.lights[rsm_frame.light_index];
        let to_light = light.position - input.world_position;
        let distance = length(to_light);

        if distance > 0.0001 && distance < light.range {
            let direction_to_light = to_light / distance;
            let n_dot_l = max(dot(normal, direction_to_light), 0.0);

            if n_dot_l > 0.0 {
                var spot = 1.0;
                if light.is_spot != 0u {
                    let cosine = dot(normalize(light.direction), -direction_to_light);
                    let cone = clamp(
                        (cosine - light.outer_cos)
                            / max(light.inner_cos - light.outer_cos, 0.0001),
                        0.0,
                        1.0,
                    );
                    spot = cone * cone * (3.0 - 2.0 * cone);
                }

                let range_falloff = 1.0 - distance / light.range;
                let attenuation = range_falloff * range_falloff / (1.0 + distance * distance);

                let diffuse = input.base_color
                    * (1.0 - input.metallic)
                    / PI;

                flux = diffuse
                    * light.color
                    * light.intensity
                    * attenuation
                    * spot
                    * n_dot_l;
            }
        }
    }

    return RsmOutput(
        vec4f(normal, 1.0),
        vec4f(flux, 1.0),
    );
}
