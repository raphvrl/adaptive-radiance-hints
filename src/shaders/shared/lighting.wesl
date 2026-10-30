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
}

struct Lights {
    length: u32,
    lights: array<GpuLight>,
}

@group(1) @binding(0)
var<storage, read> light_buffer: Lights;

struct GpuShadowView {
    view_proj: mat4x4<f32>,
    atlas_uv: vec4f,
}

struct ShadowViews {
    length: u32,
    views: array<GpuShadowView>,
}

@group(2) @binding(0)
var<storage, read> shadow_views: ShadowViews;

@group(2) @binding(1)
var shadow_atlas: texture_depth_2d;

@group(2) @binding(2)
var shadow_sampler: sampler_comparison;

const SHADOW_DEPTH_BIAS: f32 = 0.0008;

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

fn point_shadow_face(direction_from_light: vec3f) -> u32 {
    let absolute_direction = abs(direction_from_light);

    if absolute_direction.x >= absolute_direction.y
        && absolute_direction.x >= absolute_direction.z {
        return select(1u, 0u, direction_from_light.x >= 0.0);
    }

    if absolute_direction.y >= absolute_direction.z {
        return select(3u, 2u, direction_from_light.y >= 0.0);
    }

    return select(5u, 4u, direction_from_light.z >= 0.0);
}

fn shadow_visibility(
    light: GpuLight,
    world_position: vec3f,
    normal: vec3f,
    direction_to_light: vec3f,
) -> f32 {
    var view_index = light.shadow_index;

    if light.is_spot == 0u {
        view_index += point_shadow_face(world_position - light.position);
    }

    if view_index >= shadow_views.length {
        return 1.0;
    }

    let shadow_view = shadow_views.views[view_index];
    let clip_position = shadow_view.view_proj * vec4f(world_position, 1.0);
    if clip_position.w <= 0.0 {
        return 1.0;
    }

    let projected = clip_position.xyz / clip_position.w;
    if projected.x < -1.0 || projected.x > 1.0
        || projected.y < -1.0 || projected.y > 1.0
        || projected.z < 0.0 || projected.z > 1.0 {
        return 1.0;
    }

    let local_uv = vec2f(
        projected.x * 0.5 + 0.5,
        0.5 - projected.y * 0.5,
    );
    let atlas_uv = local_uv * shadow_view.atlas_uv.xy + shadow_view.atlas_uv.zw;
    let normal_light = max(dot(normal, direction_to_light), 0.0);
    let depth_bias = SHADOW_DEPTH_BIAS * (2.0 - normal_light);
    let texel_size = 1.0 / vec2f(textureDimensions(shadow_atlas));
    let half_texel = texel_size * 0.5;
    let min_uv = shadow_view.atlas_uv.zw + half_texel;
    let max_uv = shadow_view.atlas_uv.zw + shadow_view.atlas_uv.xy - half_texel;
    var visibility = 0.0;

    for (var y = -1i; y <= 1i; y += 1i) {
        for (var x = -1i; x <= 1i; x += 1i) {
            let offset = vec2f(f32(x), f32(y)) * texel_size;
            let sample_uv = clamp(atlas_uv + offset, min_uv, max_uv);
            visibility += textureSampleCompare(
                shadow_atlas,
                shadow_sampler,
                sample_uv,
                projected.z - depth_bias,
            );
        }
    }

    return visibility / 9.0;
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
    let visibility = shadow_visibility(light, world_position, normal, direction_to_light);

    return albedo * light.color * light.intensity * diffuse * attenuation * spot * visibility;
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

fn ambient_lighting(albedo: vec3f, color: vec3f, intensity: f32) -> vec3f {
    return albedo * color * intensity;
}

fn shade_surface(
    world_position: vec3f,
    normal: vec3f,
    albedo: vec3f,
    emission: vec3f,
    ambient_color: vec3f,
    ambient_intensity: f32,
) -> vec3f {
    let ambient = ambient_lighting(albedo, ambient_color, ambient_intensity);
    let direct = direct_lighting(world_position, normal, albedo);

    return emission + ambient + direct;
}
