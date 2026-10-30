import package::shared::color::space::{linear_srgb_to_rec2020, rec2020_to_linear_srgb};

const AGX_MIN_EV: f32 = -12.47393;
const AGX_MAX_EV: f32 = 4.026069;

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

public fn agx_tonemap(color: vec3f, exposure: f32) -> vec3f {
    var mapped = linear_srgb_to_rec2020(max(color, vec3f(0.0)) * exposure);
    mapped = agx_log_encode(mapped);
    mapped = agx_default_contrast(mapped);
    mapped = agx_outset(mapped);
    mapped = pow(max(mapped, vec3f(0.0)), vec3f(2.2));
    mapped = rec2020_to_linear_srgb(mapped);

    return clamp(mapped, vec3f(0.0), vec3f(1.0));
}
