import package::shared::color::agx::agx_tonemap;
import package::shared::color::space::encode_output;

struct TonemapParams {
    exposure: f32,
    surface_is_srgb: u32,
}

@group(0) @binding(0)
var hdr_image: texture_2d<f32>;

@group(0) @binding(1)
var<uniform> params: TonemapParams;

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

    return vec4f(encode_output(mapped, params.surface_is_srgb), 1.0);
}
