fn vertex_color(index: u32) -> vec3f {
    let colors = array<vec3f, 3>(
        vec3f(1.0, 0.2, 0.2),
        vec3f(0.2, 1.0, 0.2),
        vec3f(0.2, 0.4, 1.0),
    );

    return colors[index];
}
