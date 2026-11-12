use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use gpu::{FULLSCREEN_SHADER_SOURCE, GpuContext};
use thiserror::Error;
use wgpu::util::DeviceExt;

use crate::{EffectPass, UniformValue};

/// Declarations every effect shader shares: the vertex output, the uniform
/// block and a few helpers. Prepended to each shader's own source.
const COMMON_SHADER_SOURCE: &str = include_str!("shaders/common.wgsl");

/// One effect shader: its id (the `shader` of a pass), its WGSL body and
/// the number uniforms it reads, in the order they fill the uniform
/// block's sixteen scalar slots. `u_direction` is read by every shader
/// and defaults to zero.
pub struct ShaderSpec {
    pub id: &'static str,
    pub source: &'static str,
    pub numbers: &'static [&'static str],
}

/// Every shader the pipeline knows. The web app and the desktop build
/// passes for these by the same ids with the same uniform names.
pub const SHADERS: &[ShaderSpec] = &[
    ShaderSpec {
        id: "gaussian-blur",
        source: include_str!("shaders/gaussian_blur.wgsl"),
        numbers: &["u_sigma", "u_step"],
    },
    ShaderSpec {
        id: "color-correct",
        source: include_str!("shaders/color_correct.wgsl"),
        numbers: &[
            "u_brightness",
            "u_contrast",
            "u_saturation",
            "u_hue",
            "u_exposure",
            "u_gamma",
            "u_temperature",
            "u_tint",
        ],
    },
    ShaderSpec {
        id: "vignette",
        source: include_str!("shaders/vignette.wgsl"),
        numbers: &["u_amount", "u_radius", "u_softness"],
    },
    ShaderSpec {
        id: "film-grain",
        source: include_str!("shaders/film_grain.wgsl"),
        numbers: &["u_amount", "u_size", "u_time", "u_seed"],
    },
    ShaderSpec {
        id: "rgb-split",
        source: include_str!("shaders/rgb_split.wgsl"),
        numbers: &["u_amount", "u_angle"],
    },
    ShaderSpec {
        id: "pixelate",
        source: include_str!("shaders/pixelate.wgsl"),
        numbers: &["u_size"],
    },
    ShaderSpec {
        id: "sharpen",
        source: include_str!("shaders/sharpen.wgsl"),
        numbers: &["u_amount"],
    },
    ShaderSpec {
        id: "mirror",
        source: include_str!("shaders/mirror.wgsl"),
        numbers: &["u_mode"],
    },
    ShaderSpec {
        id: "glitch",
        source: include_str!("shaders/glitch.wgsl"),
        numbers: &["u_amount", "u_bands", "u_time", "u_seed"],
    },
    ShaderSpec {
        id: "kaleidoscope",
        source: include_str!("shaders/kaleidoscope.wgsl"),
        numbers: &["u_segments", "u_angle"],
    },
    ShaderSpec {
        id: "lift-gamma-gain",
        source: include_str!("shaders/lift_gamma_gain.wgsl"),
        numbers: &[
            "u_lift",
            "u_lift_r",
            "u_lift_g",
            "u_lift_b",
            "u_gamma",
            "u_gamma_r",
            "u_gamma_g",
            "u_gamma_b",
            "u_gain",
            "u_gain_r",
            "u_gain_g",
            "u_gain_b",
        ],
    },
    ShaderSpec {
        id: "curves",
        source: include_str!("shaders/curves.wgsl"),
        numbers: &[
            "u_master_0",
            "u_master_25",
            "u_master_50",
            "u_master_75",
            "u_master_100",
            "u_red_25",
            "u_red_50",
            "u_red_75",
            "u_green_25",
            "u_green_50",
            "u_green_75",
            "u_blue_25",
            "u_blue_50",
            "u_blue_75",
        ],
    },
    ShaderSpec {
        id: "levels",
        source: include_str!("shaders/levels.wgsl"),
        numbers: &["u_in_black", "u_in_white", "u_gamma", "u_out_black", "u_out_white"],
    },
    ShaderSpec {
        id: "shadows-highlights",
        source: include_str!("shaders/shadows_highlights.wgsl"),
        numbers: &["u_shadows", "u_highlights", "u_range"],
    },
    ShaderSpec {
        id: "lut",
        source: include_str!("shaders/lut.wgsl"),
        numbers: &["u_size", "u_intensity", "u_tetrahedral"],
    },
];

const DIRECTION_UNIFORM: &str = "u_direction";
const SCALAR_SLOTS: usize = 16;

pub fn shader_spec(id: &str) -> Option<&'static ShaderSpec> {
    SHADERS.iter().find(|spec| spec.id == id)
}

pub struct ApplyEffectsOptions<'a> {
    pub source: &'a wgpu::Texture,
    pub width: u32,
    pub height: u32,
    pub passes: &'a [EffectPass],
}

pub struct EffectPipeline {
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    pipelines: HashMap<String, wgpu::RenderPipeline>,
    /// Look-up tables uploaded with `register_lut`, by key.
    luts: std::sync::Mutex<HashMap<String, wgpu::Texture>>,
    /// Bound as the table for passes that read none.
    dummy_lut: wgpu::Texture,
}

#[derive(Debug, Error)]
pub enum EffectsError {
    #[error("At least one effect pass is required")]
    MissingEffectPasses,
    #[error("Unknown effect shader '{shader}'")]
    UnknownEffectShader { shader: String },
    #[error("Missing uniform '{uniform}' for shader '{shader}'")]
    MissingUniform { shader: String, uniform: String },
    #[error("Uniform '{uniform}' for shader '{shader}' must be a number")]
    InvalidNumberUniform { shader: String, uniform: String },
    #[error(
        "Uniform '{uniform}' for shader '{shader}' must be a vector of length {expected_length}"
    )]
    InvalidVectorUniform {
        shader: String,
        uniform: String,
        expected_length: usize,
    },
    #[error("Shader '{shader}' does not support uniform '{uniform}'")]
    UnsupportedUniform { shader: String, uniform: String },
    #[error("Look-up table '{key}' was not registered with the effect pipeline")]
    MissingLut { key: String },
}

/// The uniform block every effect shader reads; see `common.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct EffectUniformBuffer {
    resolution: [f32; 2],
    direction: [f32; 2],
    scalars: [f32; SCALAR_SLOTS],
}

impl EffectPipeline {
    pub fn new(context: &GpuContext) -> Self {
        let uniform_bind_group_layout =
            context
                .device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("effects-uniform-bind-group-layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });
        let vertex_shader_module =
            context
                .device()
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("effects-fullscreen-shader"),
                    source: wgpu::ShaderSource::Wgsl(FULLSCREEN_SHADER_SOURCE.into()),
                });
        let pipeline_layout =
            context
                .device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("effects-pipeline-layout"),
                    bind_group_layouts: &[
                        Some(context.texture_sampler_bind_group_layout()),
                        Some(&uniform_bind_group_layout),
                        Some(context.texture_sampler_bind_group_layout()),
                    ],
                    immediate_size: 0,
                });
        let mut pipelines = HashMap::new();
        for spec in SHADERS {
            let source = format!("{COMMON_SHADER_SOURCE}\n{}", spec.source);
            let fragment_module =
                context
                    .device()
                    .create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some(spec.id),
                        source: wgpu::ShaderSource::Wgsl(source.into()),
                    });
            let pipeline =
                context
                    .device()
                    .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some(spec.id),
                        layout: Some(&pipeline_layout),
                        vertex: wgpu::VertexState {
                            module: &vertex_shader_module,
                            entry_point: Some("vertex_main"),
                            buffers: &[wgpu::VertexBufferLayout {
                                array_stride: std::mem::size_of::<[f32; 2]>() as u64,
                                step_mode: wgpu::VertexStepMode::Vertex,
                                attributes: &[wgpu::VertexAttribute {
                                    format: wgpu::VertexFormat::Float32x2,
                                    offset: 0,
                                    shader_location: 0,
                                }],
                            }],
                            compilation_options: wgpu::PipelineCompilationOptions::default(),
                        },
                        fragment: Some(wgpu::FragmentState {
                            module: &fragment_module,
                            entry_point: Some("fragment_main"),
                            targets: &[Some(wgpu::ColorTargetState {
                                format: context.texture_format(),
                                blend: None,
                                write_mask: wgpu::ColorWrites::ALL,
                            })],
                            compilation_options: wgpu::PipelineCompilationOptions::default(),
                        }),
                        primitive: wgpu::PrimitiveState::default(),
                        depth_stencil: None,
                        multisample: wgpu::MultisampleState::default(),
                        multiview_mask: None,
                        cache: None,
                    });
            pipelines.insert(spec.id.to_string(), pipeline);
        }

        let dummy_lut = context.upload_texture(1, 1, &[0, 0, 0, 255], "effects-lut-dummy");
        Self {
            uniform_bind_group_layout,
            pipelines,
            luts: std::sync::Mutex::new(HashMap::new()),
            dummy_lut,
        }
    }

    /// Uploads a look-up table as the `lut` shader's texture: `rgba` holds
    /// `size³` pixels laid out `size²` wide (x = r + size·b) and `size`
    /// high (y = g), red/green/blue/alpha. Registering a key again replaces it.
    pub fn register_lut(&self, context: &GpuContext, key: &str, size: u32, rgba: &[u8]) {
        let width = size * size;
        let mut pixels = rgba.to_vec();
        if context.pixels_are_bgra() {
            for pixel in pixels.chunks_mut(4) {
                pixel.swap(0, 2);
            }
        }
        let texture = context.upload_texture(width, size, &pixels, "effects-lut");
        self.luts.lock().expect("lut registry").insert(key.to_owned(), texture);
    }

    pub fn has_lut(&self, key: &str) -> bool {
        self.luts.lock().expect("lut registry").contains_key(key)
    }

    pub fn apply(
        &self,
        context: &GpuContext,
        ApplyEffectsOptions {
            source,
            width,
            height,
            passes,
        }: ApplyEffectsOptions<'_>,
    ) -> Result<wgpu::Texture, EffectsError> {
        let mut encoder =
            context
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("effects-command-encoder"),
                });
        let output = self.apply_with_encoder(
            context,
            &mut encoder,
            ApplyEffectsOptions {
                source,
                width,
                height,
                passes,
            },
        )?;
        context.queue().submit([encoder.finish()]);
        Ok(output)
    }

    pub fn apply_with_encoder(
        &self,
        context: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        ApplyEffectsOptions {
            source,
            width,
            height,
            passes,
        }: ApplyEffectsOptions<'_>,
    ) -> Result<wgpu::Texture, EffectsError> {
        let mut current_texture: Option<wgpu::Texture> = None;

        for pass in passes {
            let input_texture = current_texture.as_ref().unwrap_or(source);
            let output_texture =
                context.create_render_texture(width, height, "effects-pass-output");
            let input_view = input_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let texture_bind_group =
                context
                    .device()
                    .create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("effects-texture-bind-group"),
                        layout: context.texture_sampler_bind_group_layout(),
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&input_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(context.linear_sampler()),
                            },
                        ],
                    });
            let uniform_buffer =
                context
                    .device()
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("effects-uniform-buffer"),
                        contents: bytemuck::bytes_of(&pack_effect_uniforms(pass, width, height)?),
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    });
            let uniform_bind_group =
                context
                    .device()
                    .create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("effects-uniform-bind-group"),
                        layout: &self.uniform_bind_group_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniform_buffer.as_entire_binding(),
                        }],
                    });
            let pipeline = self.pipelines.get(&pass.shader).ok_or_else(|| {
                EffectsError::UnknownEffectShader {
                    shader: pass.shader.clone(),
                }
            })?;
            let luts = self.luts.lock().expect("lut registry");
            let lut_texture = match &pass.lut {
                Some(key) => luts.get(key).ok_or_else(|| EffectsError::MissingLut { key: key.clone() })?,
                None => &self.dummy_lut,
            };
            let lut_view = lut_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let lut_bind_group = context.device().create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("effects-lut-bind-group"),
                layout: context.texture_sampler_bind_group_layout(),
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&lut_view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(context.linear_sampler()) },
                ],
            });

            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("effects-render-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &output_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                });
                render_pass.set_pipeline(pipeline);
                render_pass.set_vertex_buffer(0, context.fullscreen_quad().slice(..));
                render_pass.set_bind_group(0, &texture_bind_group, &[]);
                render_pass.set_bind_group(1, &uniform_bind_group, &[]);
                render_pass.set_bind_group(2, &lut_bind_group, &[]);
                render_pass.draw(0..6, 0..1);
            }
            drop(luts);

            current_texture = Some(output_texture);
        }

        current_texture.ok_or(EffectsError::MissingEffectPasses)
    }
}

/// Lays a pass's uniforms out as the shader declares them: each listed
/// number in its slot, `u_direction` (optional) in the direction field.
/// Unknown uniforms are an error, so a typo on either side shows up.
fn pack_effect_uniforms(
    pass: &EffectPass,
    width: u32,
    height: u32,
) -> Result<EffectUniformBuffer, EffectsError> {
    let spec = shader_spec(&pass.shader).ok_or_else(|| EffectsError::UnknownEffectShader {
        shader: pass.shader.clone(),
    })?;
    let mut scalars = [0.0f32; SCALAR_SLOTS];
    for (slot, name) in spec.numbers.iter().enumerate().take(SCALAR_SLOTS) {
        scalars[slot] = read_number_uniform(pass, name)?;
    }
    let direction = match pass.uniforms.get(DIRECTION_UNIFORM) {
        Some(_) => read_vec2_uniform(pass, DIRECTION_UNIFORM)?,
        None => [0.0, 0.0],
    };
    for uniform in pass.uniforms.keys() {
        if uniform == DIRECTION_UNIFORM || spec.numbers.contains(&uniform.as_str()) {
            continue;
        }
        return Err(EffectsError::UnsupportedUniform {
            shader: pass.shader.clone(),
            uniform: uniform.clone(),
        });
    }
    Ok(EffectUniformBuffer {
        resolution: [width as f32, height as f32],
        direction,
        scalars,
    })
}

fn read_number_uniform(pass: &EffectPass, uniform: &str) -> Result<f32, EffectsError> {
    let Some(value) = pass.uniforms.get(uniform) else {
        return Err(EffectsError::MissingUniform {
            shader: pass.shader.clone(),
            uniform: uniform.to_string(),
        });
    };
    match value {
        UniformValue::Number(value) => Ok(*value),
        UniformValue::Vector(_) => Err(EffectsError::InvalidNumberUniform {
            shader: pass.shader.clone(),
            uniform: uniform.to_string(),
        }),
    }
}

fn read_vec2_uniform(pass: &EffectPass, uniform: &str) -> Result<[f32; 2], EffectsError> {
    let Some(value) = pass.uniforms.get(uniform) else {
        return Err(EffectsError::MissingUniform {
            shader: pass.shader.clone(),
            uniform: uniform.to_string(),
        });
    };
    let UniformValue::Vector(values) = value else {
        return Err(EffectsError::InvalidVectorUniform {
            shader: pass.shader.clone(),
            uniform: uniform.to_string(),
            expected_length: 2,
        });
    };
    if values.len() != 2 {
        return Err(EffectsError::InvalidVectorUniform {
            shader: pass.shader.clone(),
            uniform: uniform.to_string(),
            expected_length: 2,
        });
    }
    Ok([values[0], values[1]])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pass(shader: &str, numbers: &[(&str, f32)]) -> EffectPass {
        EffectPass {
            shader: shader.to_owned(),
            uniforms: numbers
                .iter()
                .map(|(name, value)| ((*name).to_owned(), UniformValue::Number(*value)))
                .collect(),
            lut: None,
        }
    }

    #[test]
    fn numbers_fill_slots_in_the_declared_order() {
        let packed = pack_effect_uniforms(
            &pass(
                "color-correct",
                &[
                    ("u_brightness", 0.1),
                    ("u_contrast", 0.2),
                    ("u_saturation", 0.3),
                    ("u_hue", 0.4),
                    ("u_exposure", 0.5),
                    ("u_gamma", 1.2),
                    ("u_temperature", -0.1),
                    ("u_tint", 0.05),
                ],
            ),
            1920,
            1080,
        )
        .unwrap();
        assert_eq!(&packed.scalars[..8], &[0.1, 0.2, 0.3, 0.4, 0.5, 1.2, -0.1, 0.05]);
        assert_eq!(packed.direction, [0.0, 0.0]);
        assert_eq!(packed.resolution, [1920.0, 1080.0]);
    }

    #[test]
    fn a_missing_or_unknown_uniform_is_an_error() {
        assert!(matches!(
            pack_effect_uniforms(&pass("vignette", &[("u_amount", 1.0)]), 10, 10),
            Err(EffectsError::MissingUniform { .. })
        ));
        assert!(matches!(
            pack_effect_uniforms(
                &pass("sharpen", &[("u_amount", 1.0), ("u_other", 2.0)]),
                10,
                10
            ),
            Err(EffectsError::UnsupportedUniform { .. })
        ));
        assert!(matches!(
            pack_effect_uniforms(&pass("sepia", &[]), 10, 10),
            Err(EffectsError::UnknownEffectShader { .. })
        ));
    }

    #[test]
    fn every_shader_names_at_most_sixteen_numbers() {
        for spec in SHADERS {
            assert!(spec.numbers.len() <= SCALAR_SLOTS, "{}", spec.id);
            assert!(spec.source.contains("fragment_main"), "{}", spec.id);
        }
    }
}
