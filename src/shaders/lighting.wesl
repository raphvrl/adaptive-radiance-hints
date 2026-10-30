const AMBIENT_STRENGTH: f32 = 0.1;

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

fn light_attenuation(light_distance: f32, range: f32) -> f32 {
    let range_falloff = 1.0 - light_distance / range;
    return range_falloff * range_falloff / (1.0 + light_distance * light_distance);
}

fn spot_factor(light: GpuLight, direction_from_light: vec3f) -> f32 {
    if light.is_spot == 0u {
        return 1.0;
    }

    let cosine = dot(direction_from_light, normalize(light.direction));
    let cone = clamp(
        (cosine - light.outer_cos) / max(light.inner_cos - light.outer_cos, 0.0001),
        0.0,
        1.0,
    );

    return cone * cone * (3.0 - 2.0 * cone);
}

fn evaluate_light(
    light: GpuLight,
    world_position: vec3f,
    normal: vec3f,
    albedo: vec3f,
) -> vec3f {
    let to_light = light.position - world_position;
    let light_distance = length(to_light);

    let is_in_range = light_distance > 0.0001 && light_distance < light.range;
    if !is_in_range {
        return vec3f(0.0);
    }

    let direction_to_light = to_light / light_distance;
    let diffuse = max(dot(normal, direction_to_light), 0.0);
    let attenuation = light_attenuation(light_distance, light.range);
    let spot = spot_factor(light, -direction_to_light);

    return albedo * light.color * light.intensity * diffuse * attenuation * spot;
}

fn direct_lighting(world_position: vec3f, normal: vec3f, albedo: vec3f) -> vec3f {
    var color = vec3f(0.0);

    for (var i = 0u; i < light_buffer.length; i += 1u) {
        color += evaluate_light(
            light_buffer.lights[i],
            world_position,
            normal,
            albedo,
        );
    }

    return color;
}

public fn shade_surface(
    world_position: vec3f,
    normal: vec3f,
    albedo: vec3f,
    emission: vec3f,
) -> vec3f {
    let ambient = albedo * AMBIENT_STRENGTH;
    let direct = direct_lighting(world_position, normal, albedo);

    return emission + ambient + direct;
}
