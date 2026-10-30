struct TonemapParams {
    exposure: f32,
    surface_is_srgb: u32,
}

@group(0) @binding(0)
var hdr_image: texture_2d<f32>;

@group(0) @binding(1)
var<uniform> params: TonemapParams;

const AGX_MIN_EV: f32 = -12.47393;
const AGX_MAX_EV: f32 = 4.026069;

struct VertexOutput {
    @builtin(position) position: vec4f,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let p = vec2f(
        f32((vertex_index << 1u) & 2u),
        f32(vertex_index & 2u),
    );

    var output: VertexOutput;

    output.position = vec4f(
        p.x * 2.0 - 1.0,
        1.0 - p.y * 2.0,
        0.0,
        1.0,
    );

    return output;
}

fn linear_srgb_to_rec2020(color: vec3f) -> vec3f {
    return vec3f(
        dot(color, vec3f(0.6274, 0.3293, 0.0433)),
        dot(color, vec3f(0.0691, 0.9195, 0.0113)),
        dot(color, vec3f(0.0164, 0.0880, 0.8956)),
    );
}

fn rec2020_to_linear_srgb(color: vec3f) -> vec3f {
    return vec3f(
        dot(color, vec3f(1.6605, -0.5876, -0.0728)),
        dot(color, vec3f(-0.1246, 1.1329, -0.0083)),
        dot(color, vec3f(-0.0182, -0.1006, 1.1187)),
    );
}

fn agx_inset(color: vec3f) -> vec3f {
    return vec3f(
        dot(color, vec3f(0.856627153315983, 0.0951212405381588, 0.0482516061458583)),
        dot(color, vec3f(0.137318972929847, 0.761241990602591, 0.101439036467562)),
        dot(color, vec3f(0.11189821299995, 0.0767994186031903, 0.811302368396859)),
    );
}

fn agx_log_encode(color: vec3f) -> vec3f {
    let agx_color = max(agx_inset(color), vec3f(1e-10));
    let log_color = log2(agx_color);

    return clamp(
        (log_color - vec3f(AGX_MIN_EV)) / (AGX_MAX_EV - AGX_MIN_EV),
        vec3f(0.0),
        vec3f(1.0),
    );
}

fn agx_default_contrast(color: vec3f) -> vec3f {
    let color2 = color * color;
    let color4 = color2 * color2;

    return 15.5 * color4 * color2
        - 40.14 * color4 * color
        + 31.96 * color4
        - 6.868 * color2 * color
        + 0.4298 * color2
        + 0.1191 * color
        - 0.00232;
}

fn agx_outset(color: vec3f) -> vec3f {
    return vec3f(
        dot(color, vec3f(1.1271005818144368, -0.11060664309660323, -0.016493938717834573)),
        dot(color, vec3f(-0.1413297634984383, 1.157823702216272, -0.016493938717834257)),
        dot(color, vec3f(-0.14132976349843826, -0.11060664309660294, 1.2519364065950405)),
    );
}

fn agx_tonemap(color: vec3f, exposure: f32) -> vec3f {
    var mapped = linear_srgb_to_rec2020(max(color, vec3f(0.0)) * exposure);
    mapped = agx_log_encode(mapped);
    mapped = agx_default_contrast(mapped);
    mapped = agx_outset(mapped);
    mapped = pow(max(mapped, vec3f(0.0)), vec3f(2.2));
    mapped = rec2020_to_linear_srgb(mapped);

    return clamp(mapped, vec3f(0.0), vec3f(1.0));
}

fn linear_to_srgb(color: vec3f) -> vec3f {
    let low = color * 12.92;
    let high = 1.055 * pow(color, vec3f(1.0 / 2.4)) - vec3f(0.055);

    return select(high, low, color <= vec3f(0.0031308));
}

fn encode_output(color: vec3f) -> vec3f {
    if params.surface_is_srgb == 0u {
        return linear_to_srgb(color);
    }

    return color;
}

fn load_hdr(pixel: vec2f) -> vec3f {
    return max(
        textureLoad(hdr_image, vec2i(pixel), 0).rgb,
        vec3f(0.0),
    );
}

@fragment
fn fs_main(@builtin(position) pixel: vec4f) -> @location(0) vec4f {
    let hdr = load_hdr(pixel.xy);
    let mapped = agx_tonemap(hdr, params.exposure);

    return vec4f(encode_output(mapped), 1.0);
}
