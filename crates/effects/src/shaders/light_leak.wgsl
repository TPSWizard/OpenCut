// Light leak: two soft glows of colour that drift in from one edge and
// flicker, screen-blended over the frame (made here, no footage).
// s0 = (intensity 0..1, size 0.2..1.5, angle in radians of the edge they
// come from, time in seconds), s1 = (speed, seed, colour 1 red, green),
// s2 = (colour 1 blue, colour 2 red, green, blue). Coordinates are the
// frame's height units around its centre, so the glows stay round.
fn leak_glow(p: vec2f, centre: vec2f, size: f32) -> f32 {
    let d = p - centre;
    return exp(-dot(d, d) / (size * size * 0.5));
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let base = textureSample(input_texture, input_sampler, input.tex_coord);
    let aspect = uniforms.resolution.x / max(uniforms.resolution.y, 1.0);
    let p = (input.tex_coord - vec2f(0.5)) * vec2f(aspect, 1.0);
    let intensity = uniforms.s0.x;
    let size = max(uniforms.s0.y, 0.01);
    let angle = uniforms.s0.z;
    let t = uniforms.s0.w * uniforms.s1.x;
    let seed = uniforms.s1.y;
    // Screen y points down, so the angle turns anticlockwise on screen.
    let along = vec2f(cos(angle), -sin(angle));
    let across = vec2f(-along.y, along.x);
    let edge = along * (0.5 * max(aspect, 1.0));
    let c1 = edge + across * (0.35 * sin(t * 0.9 + seed));
    let c2 = edge * 0.8 + across * (0.45 * cos(t * 0.6 + seed * 1.7));
    let flicker = 0.8 + 0.2 * sin(t * 1.7 + seed * 2.3);
    let colour1 = vec3f(uniforms.s1.z, uniforms.s1.w, uniforms.s2.x);
    let colour2 = vec3f(uniforms.s2.y, uniforms.s2.z, uniforms.s2.w);
    let light = clamp((colour1 * leak_glow(p, c1, size) + colour2 * leak_glow(p, c2, size * 0.8)) * flicker * intensity, vec3f(0.0), vec3f(1.0));
    // Screen in premultiplied terms; the alpha grows to hold the light.
    let premultiplied = base.rgb * base.a;
    let screened = premultiplied + light - premultiplied * light;
    let alpha = max(base.a, max(light.r, max(light.g, light.b)));
    if (alpha <= 0.0) {
        return vec4f(0.0);
    }
    return vec4f(min(screened / alpha, vec3f(1.0)), alpha);
}
