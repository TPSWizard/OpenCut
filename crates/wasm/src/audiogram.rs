//! Audiograms for the web editor (row 100): the shared `audiogram` crate
//! analyses a clip's sound and draws the bars, waveform, line or circle,
//! the same as the desktop.

use wasm_bindgen::prelude::wasm_bindgen;

/// The analysis settings and limits: JSON `{sampleRate, fps, bands, maxSeconds, maxRasterSide}`.
#[wasm_bindgen(js_name = audiogramConstants)]
pub fn audiogram_constants() -> String {
    serde_json::json!({
        "sampleRate": ::audiogram::SAMPLE_RATE,
        "fps": ::audiogram::FPS,
        "bands": ::audiogram::BANDS,
        "maxSeconds": ::audiogram::MAX_SECONDS,
        "maxRasterSide": ::audiogram::MAX_RASTER_SIDE,
    })
    .to_string()
}

/// The styles: JSON `[{id, label}]`; and the look's params with their defaults.
#[wasm_bindgen(js_name = audiogramStyles)]
pub fn audiogram_styles() -> String {
    let styles: Vec<serde_json::Value> =
        ::audiogram::Style::ALL.iter().map(|style| serde_json::json!({ "id": style.id(), "label": style.label() })).collect();
    let defaults: serde_json::Map<String, serde_json::Value> =
        ::audiogram::default_params().into_iter().map(|(key, value)| (key.to_owned(), value)).collect();
    serde_json::json!({ "styles": styles, "defaults": defaults }).to_string()
}

/// Analyses mono `samples` at `sample_rate` once per source time in
/// `times` (seconds): the `levels` param text.
#[wasm_bindgen(js_name = audiogramAnalyse)]
pub fn audiogram_analyse(samples: &[f32], sample_rate: f64, times: &[f64]) -> String {
    ::audiogram::analyse(samples, sample_rate, times, ::audiogram::BANDS, ::audiogram::FPS).encode()
}

/// The raster size for a canvas.
#[wasm_bindgen(js_name = audiogramRasterSize)]
pub fn audiogram_raster_size(canvas_width: u32, canvas_height: u32) -> Vec<u32> {
    let (w, h) = ::audiogram::raster_size(canvas_width, canvas_height, ::audiogram::MAX_RASTER_SIDE);
    vec![w, h]
}

/// An audiogram's decoded levels, kept so each frame draws without
/// decoding them again.
#[wasm_bindgen]
pub struct Audiogram {
    levels: Option<::audiogram::Levels>,
}

#[wasm_bindgen]
impl Audiogram {
    /// From the `levels` param text (draws nothing when it does not parse).
    #[wasm_bindgen(constructor)]
    pub fn new(levels: &str) -> Audiogram {
        Audiogram { levels: ::audiogram::Levels::parse(levels) }
    }

    /// Seconds of levels (0 when they did not parse).
    pub fn seconds(&self) -> f64 {
        self.levels.as_ref().map_or(0.0, |levels| levels.seconds())
    }

    /// Straight-alpha RGBA `width` x `height` `seconds` into the element,
    /// in the look of `params_json` (the element's params); empty when the
    /// levels did not parse.
    pub fn render(&self, params_json: &str, seconds: f64, width: u32, height: u32) -> Vec<u8> {
        match &self.levels {
            Some(levels) => {
                ::audiogram::render(levels, &::audiogram::Look::from_params_json(params_json), seconds, width as usize, height as usize)
            }
            None => Vec::new(),
        }
    }
}
