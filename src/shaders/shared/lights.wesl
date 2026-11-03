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
