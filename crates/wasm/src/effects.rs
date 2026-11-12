#![cfg(target_arch = "wasm32")]

use effects::{ApplyEffectsOptions, EffectPass, UniformValue};
use gpu::wgpu;
use js_sys::Object;
use serde::Deserialize;
use wasm_bindgen::{JsCast, JsValue, prelude::wasm_bindgen};

use crate::gpu::{
    import_canvas_texture, read_offscreen_canvas_property, read_serde_property, read_u32_property,
    render_texture_to_canvas, with_gpu_runtime,
};

struct ApplyEffectPassesOptions {
    source: wgpu::web_sys::OffscreenCanvas,
    width: u32,
    height: u32,
    passes: Vec<EffectPassInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EffectPassInput {
    shader: String,
    uniforms: Vec<EffectUniformInput>,
    #[serde(default)]
    lut: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EffectUniformInput {
    name: String,
    value: Vec<f32>,
}

#[wasm_bindgen(js_name = applyEffectPasses)]
pub fn apply_effect_passes(options: JsValue) -> Result<wgpu::web_sys::OffscreenCanvas, JsValue> {
    let ApplyEffectPassesOptions {
        source,
        width,
        height,
        passes,
    } = parse_apply_effect_passes_options(options)?;

    with_gpu_runtime(|runtime| {
        let source_texture = import_canvas_texture(
            &runtime.context,
            &source,
            width,
            height,
            "effects-input-texture",
        );
        let effect_passes = map_effect_passes(passes);
        let result_texture = runtime
            .effects
            .apply(
                &runtime.context,
                ApplyEffectsOptions {
                    source: &source_texture,
                    width,
                    height,
                    passes: &effect_passes,
                },
            )
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        render_texture_to_canvas(&runtime.context, &result_texture, width, height)
    })
}

fn map_effect_passes(effect_passes: Vec<EffectPassInput>) -> Vec<EffectPass> {
    effect_passes
        .into_iter()
        .map(|pass| EffectPass {
            lut: pass.lut,
            shader: pass.shader,
            uniforms: pass
                .uniforms
                .into_iter()
                .map(|uniform| {
                    let value = if uniform.value.len() == 1 {
                        UniformValue::Number(uniform.value[0])
                    } else {
                        UniformValue::Vector(uniform.value)
                    };
                    (uniform.name, value)
                })
                .collect(),
        })
        .collect()
}

/// Uploads a look-up table for the `lut` effect: `{ key, size, data }`
/// with `data` a Uint8Array of `size³` RGBA pixels laid out `size²` wide
/// (x = r + size·b) and `size` high (y = g). Passes then name the `key`.
#[wasm_bindgen(js_name = registerEffectLut)]
pub fn register_effect_lut(options: JsValue) -> Result<(), JsValue> {
    let object: Object = options
        .dyn_into()
        .map_err(|_| JsValue::from_str("registerEffectLut expects an options object"))?;
    let key = js_sys::Reflect::get(&object, &JsValue::from_str("key"))?
        .as_string()
        .ok_or_else(|| JsValue::from_str("Property 'key' must be a string"))?;
    let size = read_u32_property(&object, "size")?;
    let data: js_sys::Uint8Array = js_sys::Reflect::get(&object, &JsValue::from_str("data"))?
        .dyn_into()
        .map_err(|_| JsValue::from_str("Property 'data' must be a Uint8Array"))?;
    let pixels = data.to_vec();
    if size < 2 || pixels.len() != (size * size * size * 4) as usize {
        return Err(JsValue::from_str("LUT data must hold size³ RGBA pixels"));
    }
    with_gpu_runtime(|runtime| {
        runtime.effects.register_lut(&runtime.context, &key, size, &pixels);
        crate::compositor::register_lut_with_compositor(&runtime.context, &key, size, &pixels);
        Ok(())
    })?;
    // Kept so a compositor created later gets the table too.
    LUT_STORE.with(|store| {
        store.borrow_mut().insert(key, (size, pixels));
    });
    Ok(())
}

thread_local! {
    /// Every table registered so far, replayed into a new compositor.
    pub(crate) static LUT_STORE: std::cell::RefCell<std::collections::HashMap<String, (u32, Vec<u8>)>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

fn parse_apply_effect_passes_options(value: JsValue) -> Result<ApplyEffectPassesOptions, JsValue> {
    let object: Object = value
        .dyn_into()
        .map_err(|_| JsValue::from_str("applyEffectPasses expects an options object"))?;

    Ok(ApplyEffectPassesOptions {
        source: read_offscreen_canvas_property(&object, "source")?,
        width: read_u32_property(&object, "width")?,
        height: read_u32_property(&object, "height")?,
        passes: read_serde_property(&object, "passes")?,
    })
}
