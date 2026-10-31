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

struct GpuShadowView {
    view_proj: mat4x4<f32>,
    atlas_uv: vec4f,
    has_atlas_tile: u32,
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

const SHADOW_FILTER_RADIUS_TEXELS: f32 = 1.5;

const VOGEL_DISK_OFFSETS: array<vec2f, 16> = array<vec2f, 16>(
    vec2f(0.176777, 0.000000),
    vec2f(-0.225772, 0.206826),
    vec2f(0.034558, -0.393771),
    vec2f(0.284571, 0.371173),
    vec2f(-0.522223, -0.092374),
    vec2f(0.494695, -0.314685),
    vec2f(-0.165466, 0.615525),
    vec2f(-0.315561, -0.607594),
    vec2f(0.684642, 0.250030),
    vec2f(-0.712256, 0.294009),
    vec2f(0.343354, -0.733729),
    vec2f(0.253730, 0.808932),
    vec2f(-0.764746, -0.443186),
    vec2f(0.897134, -0.197232),
    vec2f(-0.547507, 0.778772),
    vec2f(-0.126487, -0.976090),
);

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
    light_distance: f32,
) -> f32 {
    var view_index = light.shadow_index;

    if light.is_spot == 0u {
        view_index += point_shadow_face(world_position - light.position);
    }

    if view_index >= shadow_views.length {
        return 1.0;
    }

    let shadow_view = shadow_views.views[view_index];
    if shadow_view.has_atlas_tile == 0u {
        return 1.0;
    }

    let tangent_half_fov = select(
        1.0,
        sqrt(max(1.0 - light.outer_cos * light.outer_cos, 0.0))
            / max(light.outer_cos, 0.0001),
        light.is_spot != 0u,
    );
    let texel_world_size = 2.0 * light_distance * tangent_half_fov / f32(light.shadow_resolution);
    let grazing_angle = 1.0 - clamp(abs(dot(normal, direction_to_light)), 0.0, 1.0);
    let biased_position = world_position + normal * texel_world_size * light.shadow_normal_bias * grazing_angle;
    let clip_position = shadow_view.view_proj * vec4f(biased_position, 1.0);
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
    var depth_bias = light.shadow_bias / 100.0;
    if light.is_spot == 0u {
        let near_plane = min(max(light.range * 0.05, 0.001), light.range * 0.5);
        let view_depth = max(clip_position.w, near_plane);
        let face_alignment = max(
            abs(direction_to_light.x),
            max(abs(direction_to_light.y), abs(direction_to_light.z)),
        );
        let depth_slope = light.range * near_plane
            / (max(light.range - near_plane, 0.0001) * view_depth * view_depth);
        // Point lights use a world-space bias; convert it through this perspective map's depth curve.
        depth_bias = light.shadow_bias * face_alignment * depth_slope;
    }
    let texel_size = 1.0 / vec2f(textureDimensions(shadow_atlas));
    let half_texel = texel_size * 0.5;
    let min_uv = shadow_view.atlas_uv.zw + half_texel;
    let max_uv = shadow_view.atlas_uv.zw + shadow_view.atlas_uv.xy - half_texel;
    var visibility = 0.0;

    for (var i = 0u; i < 16u; i += 1u) {
        let offset = VOGEL_DISK_OFFSETS[i] * SHADOW_FILTER_RADIUS_TEXELS * texel_size;
        let sample_uv = clamp(atlas_uv + offset, min_uv, max_uv);
        visibility += textureSampleCompare(
            shadow_atlas,
            shadow_sampler,
            sample_uv,
            projected.z - depth_bias,
        );
    }

    return visibility / 16.0;
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
    let visibility = shadow_visibility(
        light,
        world_position,
        normal,
        direction_to_light,
        light_distance,
    );

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
