// Colour look-up table. The table is a 2D texture of width size², height
// size: texel (r + size·b, g) holds the output colour for the grid point
// (r, g, b). s0 = (size, intensity, tetrahedral flag, unused). Looked up
// with textureLoad on the grid, so no half-texel offsets apply.
@group(2) @binding(0) var lut_texture: texture_2d<f32>;
@group(2) @binding(1) var lut_sampler: sampler;

fn lut_at(r: i32, g: i32, b: i32, size: i32) -> vec3f {
    let x = clamp(r, 0, size - 1) + size * clamp(b, 0, size - 1);
    let y = clamp(g, 0, size - 1);
    return textureLoad(lut_texture, vec2i(x, y), 0).rgb;
}

fn lut_trilinear(c: vec3f, size: i32) -> vec3f {
    let n = f32(size - 1);
    let p = clamp(c, vec3f(0.0), vec3f(1.0)) * n;
    let i0 = vec3i(floor(p));
    let t = p - vec3f(i0);
    let i1 = min(i0 + vec3i(1), vec3i(size - 1));
    let c000 = lut_at(i0.x, i0.y, i0.z, size);
    let c100 = lut_at(i1.x, i0.y, i0.z, size);
    let c010 = lut_at(i0.x, i1.y, i0.z, size);
    let c110 = lut_at(i1.x, i1.y, i0.z, size);
    let c001 = lut_at(i0.x, i0.y, i1.z, size);
    let c101 = lut_at(i1.x, i0.y, i1.z, size);
    let c011 = lut_at(i0.x, i1.y, i1.z, size);
    let c111 = lut_at(i1.x, i1.y, i1.z, size);
    let c00 = mix(c000, c100, t.x);
    let c10 = mix(c010, c110, t.x);
    let c01 = mix(c001, c101, t.x);
    let c11 = mix(c011, c111, t.x);
    let c0 = mix(c00, c10, t.y);
    let c1 = mix(c01, c11, t.y);
    return mix(c0, c1, t.z);
}

// Tetrahedral interpolation: the cube cell is split into six tetrahedra
// by the order of the fractional parts; four corners are read.
fn lut_tetrahedral(c: vec3f, size: i32) -> vec3f {
    let n = f32(size - 1);
    let p = clamp(c, vec3f(0.0), vec3f(1.0)) * n;
    let i0 = vec3i(floor(p));
    let f = p - vec3f(i0);
    let i1 = min(i0 + vec3i(1), vec3i(size - 1));
    let c000 = lut_at(i0.x, i0.y, i0.z, size);
    let c111 = lut_at(i1.x, i1.y, i1.z, size);
    var result: vec3f;
    if (f.x >= f.y) {
        if (f.y >= f.z) {
            let c100 = lut_at(i1.x, i0.y, i0.z, size);
            let c110 = lut_at(i1.x, i1.y, i0.z, size);
            result = (1.0 - f.x) * c000 + (f.x - f.y) * c100 + (f.y - f.z) * c110 + f.z * c111;
        } else if (f.x >= f.z) {
            let c100 = lut_at(i1.x, i0.y, i0.z, size);
            let c101 = lut_at(i1.x, i0.y, i1.z, size);
            result = (1.0 - f.x) * c000 + (f.x - f.z) * c100 + (f.z - f.y) * c101 + f.y * c111;
        } else {
            let c001 = lut_at(i0.x, i0.y, i1.z, size);
            let c101 = lut_at(i1.x, i0.y, i1.z, size);
            result = (1.0 - f.z) * c000 + (f.z - f.x) * c001 + (f.x - f.y) * c101 + f.y * c111;
        }
    } else {
        if (f.z >= f.y) {
            let c001 = lut_at(i0.x, i0.y, i1.z, size);
            let c011 = lut_at(i0.x, i1.y, i1.z, size);
            result = (1.0 - f.z) * c000 + (f.z - f.y) * c001 + (f.y - f.x) * c011 + f.x * c111;
        } else if (f.z >= f.x) {
            let c010 = lut_at(i0.x, i1.y, i0.z, size);
            let c011 = lut_at(i0.x, i1.y, i1.z, size);
            result = (1.0 - f.y) * c000 + (f.y - f.z) * c010 + (f.z - f.x) * c011 + f.x * c111;
        } else {
            let c010 = lut_at(i0.x, i1.y, i0.z, size);
            let c110 = lut_at(i1.x, i1.y, i0.z, size);
            result = (1.0 - f.y) * c000 + (f.y - f.x) * c010 + (f.x - f.z) * c110 + f.z * c111;
        }
    }
    return result;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let source = textureSample(input_texture, input_sampler, input.tex_coord);
    let size = max(i32(round(uniforms.s0.x)), 2);
    let intensity = clamp(uniforms.s0.y, 0.0, 1.0);
    let tetrahedral = uniforms.s0.z > 0.5;
    var graded: vec3f;
    if (tetrahedral) {
        graded = lut_tetrahedral(source.rgb, size);
    } else {
        graded = lut_trilinear(source.rgb, size);
    }
    return vec4f(mix(source.rgb, graded, intensity), source.a);
}
