import super::palette::vertex_color;

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) color: vec3f,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    let positions = array<vec2f, 3>(
        vec2f( 0.0,  0.6),
        vec2f(-0.6, -0.6),
        vec2f( 0.6, -0.6),
    );

    var output: VertexOutput;
    output.position = vec4f(positions[index], 0.0, 1.0);
    output.color = vertex_color(index);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    return vec4f(input.color, 1.0);
}
