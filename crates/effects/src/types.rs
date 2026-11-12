use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct EffectPass {
    pub shader: String,
    pub uniforms: HashMap<String, UniformValue>,
    /// The key of a look-up table registered with the pipeline, for shaders
    /// that read one (`lut`).
    pub lut: Option<String>,
}

#[derive(Clone, Debug)]
pub enum UniformValue {
    Number(f32),
    Vector(Vec<f32>),
}
