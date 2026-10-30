public fn linear_srgb_to_rec2020(color: vec3f) -> vec3f {
    return vec3f(
        dot(color, vec3f(0.6274, 0.3293, 0.0433)),
        dot(color, vec3f(0.0691, 0.9195, 0.0113)),
        dot(color, vec3f(0.0164, 0.0880, 0.8956)),
    );
}

public fn rec2020_to_linear_srgb(color: vec3f) -> vec3f {
    return vec3f(
        dot(color, vec3f(1.6605, -0.5876, -0.0728)),
        dot(color, vec3f(-0.1246, 1.1329, -0.0083)),
        dot(color, vec3f(-0.0182, -0.1006, 1.1187)),
    );
}

fn linear_to_srgb(color: vec3f) -> vec3f {
    let low = color * 12.92;
    let high = 1.055 * pow(color, vec3f(1.0 / 2.4)) - vec3f(0.055);

    return select(high, low, color <= vec3f(0.0031308));
}

public fn encode_output(color: vec3f, surface_is_srgb: u32) -> vec3f {
    if surface_is_srgb == 0u {
        return linear_to_srgb(color);
    }

    return color;
}
