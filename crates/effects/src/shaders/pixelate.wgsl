// Pixelate (mosaic). s0.x = cell size in frame pixels; every pixel takes
// the colour at the centre of its cell.
@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let size = max(uniforms.s0.x, 1.0);
    let cell = vec2f(size) / uniforms.resolution;
    let uv = (floor(input.tex_coord / cell) + vec2f(0.5)) * cell;
    return sample_clamped(uv);
}
