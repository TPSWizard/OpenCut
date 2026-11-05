// Mirror. s0.x = mode: 0 copies the left half onto the right, 1 the right
// half onto the left, 2 the top onto the bottom, 3 the bottom onto the top.
@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let mode = i32(round(uniforms.s0.x));
    var uv = input.tex_coord;
    if (mode == 0 && uv.x > 0.5) {
        uv.x = 1.0 - uv.x;
    } else if (mode == 1 && uv.x < 0.5) {
        uv.x = 1.0 - uv.x;
    } else if (mode == 2 && uv.y > 0.5) {
        uv.y = 1.0 - uv.y;
    } else if (mode == 3 && uv.y < 0.5) {
        uv.y = 1.0 - uv.y;
    }
    return textureSample(input_texture, input_sampler, uv);
}
