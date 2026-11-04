//! Clip-to-clip transitions: the gl-transitions collection (MIT, two BSD;
//! see `ATTRIBUTION.md`) drawn as a two-input fragment pass between the
//! `from` and `to` sides of a [`crate::TransitionDescriptor`].
//!
//! The `gl-transitions` crate supplies each transition as a complete WGSL
//! fragment shader (`main`, bindings 0 uniforms, 1 sampler, 2 from, 3 to in
//! group 0, uv origin bottom left) plus its uniform layout; this module adds
//! the vertex stage, builds one pipeline per transition on first use and
//! packs the project's parameter values into the uniform buffer.

use std::collections::HashMap;

use gl_transitions::{ParamType, Transition, Value};
use gpu::{GpuContext, wgpu};
use serde::Serialize;

use crate::frame::TransitionParamDescriptor;

/// Used when a project names a transition that is not in the catalogue.
pub const FALLBACK_TRANSITION: &str = "Fade";

/// A fullscreen quad whose uv has its origin at the bottom left, as the GL
/// Transition specification expects; the generated shaders flip it back when
/// they sample the (top-left origin) textures.
const VERTEX_SOURCE: &str = r#"
struct TransitionVertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}

@vertex
fn transition_vertex(@location(0) position: vec2f) -> TransitionVertexOutput {
    var output: TransitionVertexOutput;
    output.position = vec4f(position, 0.0, 1.0);
    output.uv = vec2f(position.x * 0.5 + 0.5, position.y * 0.5 + 0.5);
    return output;
}
"#;

/// What the editors show for a transition: its name, description, tags and
/// user-settable parameters with defaults and UI hints.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionInfo {
    pub name: &'static str,
    pub author: &'static str,
    pub license: &'static str,
    pub description: Option<&'static str>,
    pub tags: &'static [&'static str],
    pub params: Vec<TransitionParamInfo>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionParamInfo {
    pub name: &'static str,
    /// The uniform's GLSL type: `float`, `vec2`, `int`, `bool`, …
    pub kind: &'static str,
    pub default: TransitionParamDescriptor,
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub step: Option<f32>,
    /// The value is a colour (RGB or RGBA in 0..1).
    pub color: bool,
    pub description: Option<&'static str>,
}

/// Every transition the compositor can draw, sorted by name. Transitions
/// that need an extra input image (a luma matte or displacement map) are
/// left out: the project has nowhere to keep one yet.
pub fn transition_catalogue() -> Vec<TransitionInfo> {
    gl_transitions::all()
        .iter()
        .filter(|transition| is_drawable(transition))
        .map(info)
        .collect()
}

pub fn transition_info(name: &str) -> Option<TransitionInfo> {
    gl_transitions::get(name)
        .filter(|transition| is_drawable(transition))
        .map(info)
}

fn is_drawable(transition: &Transition) -> bool {
    transition.textures().is_empty() && transition.wgsl().is_some()
}

fn info(transition: &'static Transition) -> TransitionInfo {
    TransitionInfo {
        name: transition.name(),
        author: transition.author(),
        license: transition.license(),
        description: transition.description(),
        tags: transition.tags(),
        params: transition
            .params()
            .iter()
            .map(|param| TransitionParamInfo {
                name: param.name,
                kind: param.ty.glsl_name(),
                default: describe(param.default),
                min: param.min,
                max: param.max,
                step: param.step,
                color: param.color,
                description: param.description,
            })
            .collect(),
    }
}

fn describe(value: Value) -> TransitionParamDescriptor {
    match value {
        Value::Float(v) => TransitionParamDescriptor::Number(v as f64),
        Value::Int(v) => TransitionParamDescriptor::Number(v as f64),
        Value::Bool(v) => TransitionParamDescriptor::Bool(v),
        Value::Vec2(v) => TransitionParamDescriptor::Vector(v.iter().map(|&c| c as f64).collect()),
        Value::Vec3(v) => TransitionParamDescriptor::Vector(v.iter().map(|&c| c as f64).collect()),
        Value::Vec4(v) => TransitionParamDescriptor::Vector(v.iter().map(|&c| c as f64).collect()),
        Value::IVec2(v) => TransitionParamDescriptor::Vector(v.iter().map(|&c| c as f64).collect()),
        Value::IVec3(v) => TransitionParamDescriptor::Vector(v.iter().map(|&c| c as f64).collect()),
        Value::IVec4(v) => TransitionParamDescriptor::Vector(v.iter().map(|&c| c as f64).collect()),
        _ => TransitionParamDescriptor::Number(0.0),
    }
}

/// A stored parameter value as the uniform's type, or `None` when it does
/// not fit (the default is kept then).
fn typed_value(kind: ParamType, value: &TransitionParamDescriptor) -> Option<Value> {
    fn floats<const N: usize>(values: &[f64]) -> Option<[f32; N]> {
        (values.len() == N).then(|| std::array::from_fn(|i| values[i] as f32))
    }
    fn ints<const N: usize>(values: &[f64]) -> Option<[i32; N]> {
        (values.len() == N).then(|| std::array::from_fn(|i| values[i].round() as i32))
    }
    match (kind, value) {
        (ParamType::Float, TransitionParamDescriptor::Number(v)) => Some(Value::Float(*v as f32)),
        (ParamType::Int, TransitionParamDescriptor::Number(v)) => Some(Value::Int(v.round() as i32)),
        (ParamType::Bool, TransitionParamDescriptor::Bool(v)) => Some(Value::Bool(*v)),
        (ParamType::Bool, TransitionParamDescriptor::Number(v)) => Some(Value::Bool(*v != 0.0)),
        (ParamType::Vec2, TransitionParamDescriptor::Vector(v)) => floats(v).map(Value::Vec2),
        (ParamType::Vec3, TransitionParamDescriptor::Vector(v)) => floats(v).map(Value::Vec3),
        (ParamType::Vec4, TransitionParamDescriptor::Vector(v)) => floats(v).map(Value::Vec4),
        (ParamType::IVec2, TransitionParamDescriptor::Vector(v)) => ints(v).map(Value::IVec2),
        (ParamType::IVec3, TransitionParamDescriptor::Vector(v)) => ints(v).map(Value::IVec3),
        (ParamType::IVec4, TransitionParamDescriptor::Vector(v)) => ints(v).map(Value::IVec4),
        _ => None,
    }
}

/// The transition to draw for a project's name: the named one when it is
/// in the catalogue and drawable, otherwise [`FALLBACK_TRANSITION`].
pub(crate) fn resolve(name: &str) -> &'static Transition {
    gl_transitions::get(name)
        .filter(|transition| is_drawable(transition))
        .or_else(|| gl_transitions::get(FALLBACK_TRANSITION))
        .expect("the fallback transition is in the catalogue")
}

/// The uniform buffer bytes for a transition at `progress`: the defaults
/// overridden by the stored parameters that fit, padded to 16 bytes as
/// WebGL requires.
pub(crate) fn uniform_bytes(
    transition: &'static Transition,
    progress: f32,
    ratio: f32,
    params: &HashMap<String, TransitionParamDescriptor>,
) -> Vec<u8> {
    let mut values = transition.default_params();
    for param in transition.params() {
        if let Some(stored) = params.get(param.name)
            && let Some(value) = typed_value(param.ty, stored)
        {
            // The type was matched above, so this cannot fail.
            let _ = values.set(param.name, value);
        }
    }
    let mut bytes = values
        .uniform_buffer(progress.clamp(0.0, 1.0), ratio)
        .unwrap_or_else(|| vec![0; 16]);
    let padded = bytes.len().div_ceil(16) * 16;
    bytes.resize(padded, 0);
    bytes
}

/// One pipeline per transition, built on first use from the generated WGSL.
pub(crate) struct TransitionPipelines {
    bind_group_layout: wgpu::BindGroupLayout,
    pipelines: HashMap<&'static str, wgpu::RenderPipeline>,
}

impl TransitionPipelines {
    pub(crate) fn new(context: &GpuContext) -> Self {
        let texture_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let bind_group_layout =
            context
                .device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("compositor-transition-layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        texture_entry(2),
                        texture_entry(3),
                    ],
                });
        Self {
            bind_group_layout,
            pipelines: HashMap::new(),
        }
    }

    fn pipeline(&mut self, context: &GpuContext, transition: &'static Transition) -> &wgpu::RenderPipeline {
        self.pipelines.entry(transition.name()).or_insert_with(|| {
            let device = context.device();
            let source = format!(
                "{}\n{}",
                transition.wgsl().expect("drawable transitions have WGSL"),
                VERTEX_SOURCE
            );
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("compositor-transition-shader"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("compositor-transition-pipeline-layout"),
                bind_group_layouts: &[Some(&self.bind_group_layout)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("compositor-transition-pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("transition_vertex"),
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
                    module: &module,
                    entry_point: Some("main"),
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
            })
        })
    }

    /// Draws `from` and `to` (both frame-sized, straight alpha) blended by
    /// the transition at `progress` into `target`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw(
        &mut self,
        context: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        name: &str,
        progress: f32,
        params: &HashMap<String, TransitionParamDescriptor>,
        from: &wgpu::Texture,
        to: &wgpu::Texture,
        target: &wgpu::Texture,
        width: u32,
        height: u32,
    ) {
        let transition = resolve(name);
        let ratio = width as f32 / height.max(1) as f32;
        let bytes = uniform_bytes(transition, progress, ratio, params);
        let device = context.device();
        let uniform_buffer = wgpu::util::DeviceExt::create_buffer_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("compositor-transition-uniform-buffer"),
                contents: &bytes,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            },
        );
        let from_view = from.create_view(&wgpu::TextureViewDescriptor::default());
        let to_view = to.create_view(&wgpu::TextureViewDescriptor::default());
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compositor-transition-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(context.linear_sampler()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&from_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&to_view),
                },
            ],
        });
        let pipeline = self.pipeline(context, transition);
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("compositor-transition-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
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
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..6, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_is_drawable_and_sorted() {
        let catalogue = transition_catalogue();
        assert!(catalogue.len() > 150, "{} transitions", catalogue.len());
        assert!(catalogue.windows(2).all(|pair| pair[0].name < pair[1].name));
        assert!(catalogue.iter().any(|t| t.name == "Fade"));
        assert!(catalogue.iter().any(|t| t.name == "DirectionalWipe"));
        // The two that need a second input image are left out.
        assert!(catalogue.iter().all(|t| t.name != "LumaWipe" && t.name != "DisplacementMap"));
        assert!(catalogue.iter().all(|t| t.license.starts_with("MIT") || t.license.contains("BSD")));
    }

    #[test]
    fn parameters_are_described_with_their_defaults() {
        let wipe = transition_info("DirectionalWipe").unwrap();
        let direction = wipe.params.iter().find(|p| p.name == "direction").unwrap();
        assert_eq!(direction.kind, "vec2");
        assert_eq!(direction.default, TransitionParamDescriptor::Vector(vec![1.0, -1.0]));
        let smoothness = wipe.params.iter().find(|p| p.name == "smoothness").unwrap();
        assert_eq!(smoothness.kind, "float");
        assert_eq!(smoothness.default, TransitionParamDescriptor::Number(0.5));
        assert!(transition_info("NoSuchTransition").is_none());
    }

    #[test]
    fn stored_values_override_defaults_when_they_fit() {
        let wipe = resolve("DirectionalWipe");
        let mut params = HashMap::new();
        params.insert("direction".to_owned(), TransitionParamDescriptor::Vector(vec![0.0, 1.0]));
        params.insert("smoothness".to_owned(), TransitionParamDescriptor::Vector(vec![9.0])); // wrong type: ignored
        params.insert("nonsense".to_owned(), TransitionParamDescriptor::Number(1.0)); // unknown: ignored
        let bytes = uniform_bytes(wipe, 0.25, 16.0 / 9.0, &params);
        assert_eq!(bytes.len() % 16, 0);
        let floats: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        let layout = wipe.uniform_layout().unwrap();
        let offset = |name: &str| layout.fields.iter().find(|f| f.name == name).unwrap().offset / 4;
        assert_eq!(floats[offset("progress")], 0.25);
        assert!((floats[offset("ratio")] - 16.0 / 9.0).abs() < 1e-6);
        assert_eq!(&floats[offset("direction")..offset("direction") + 2], &[0.0, 1.0]);
        assert_eq!(floats[offset("smoothness")], 0.5, "the default survives a value of the wrong type");
    }

    #[test]
    fn unknown_names_fall_back_to_fade() {
        assert_eq!(resolve("NoSuchTransition").name(), "Fade");
        assert_eq!(resolve("LumaWipe").name(), "Fade", "needs a luma texture");
        assert_eq!(resolve("CrossZoom").name(), "CrossZoom");
    }
}
