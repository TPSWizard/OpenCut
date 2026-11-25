//! Whole-mix loudness and the live meter for the web editor: the shared
//! `loudness` crate, so the web masters an export and meters the preview
//! exactly as the desktop does. Numbers come back in `Float64Array`s, which
//! keep `-Infinity` (silence) as it is.

use wasm_bindgen::prelude::wasm_bindgen;

/// Masters a mix in place (the left and right channels; pass an empty
/// right for mono) to `target_lufs` with true peaks under `ceiling_dbtp`:
/// `[inputLufs, inputTruePeakDbtp, gainDb, outputLufs, outputTruePeakDbtp,
/// limited (0 or 1), passes]`.
#[wasm_bindgen(js_name = masterMix)]
pub fn master_mix(left: &mut [f32], right: &mut [f32], sample_rate: u32, target_lufs: f64, ceiling_dbtp: f64) -> Vec<f64> {
    let mut planes = vec![left.to_vec()];
    if !right.is_empty() {
        planes.push(right.to_vec());
    }
    let report = ::loudness::master(&mut planes, sample_rate, target_lufs, ceiling_dbtp);
    left.copy_from_slice(&planes[0]);
    if let Some(mastered) = planes.get(1) {
        right.copy_from_slice(mastered);
    }
    vec![
        report.input_lufs,
        report.input_true_peak_dbtp,
        report.gain_db,
        report.output_lufs,
        report.output_true_peak_dbtp,
        if report.limited { 1.0 } else { 0.0 },
        report.passes as f64,
    ]
}

/// The preview's live loudness meter (`loudness::Meter`).
#[wasm_bindgen]
pub struct LoudnessMeter {
    inner: ::loudness::Meter,
}

#[wasm_bindgen]
impl LoudnessMeter {
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: u32, channels: usize) -> LoudnessMeter {
        LoudnessMeter { inner: ::loudness::Meter::new(sample_rate, channels) }
    }

    /// One block as played: the left and right channels (an empty right for mono).
    #[wasm_bindgen(js_name = pushPlanar)]
    pub fn push_planar(&mut self, left: &[f32], right: &[f32]) {
        if right.is_empty() {
            self.inner.push_planar(&[left]);
        } else {
            self.inner.push_planar(&[left, right]);
        }
    }

    /// `[momentaryLufs, shortTermLufs, peakDbfs]`.
    pub fn readings(&self) -> Vec<f64> {
        let readings = self.inner.readings();
        vec![readings.momentary_lufs, readings.short_term_lufs, readings.peak_dbfs]
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }
}
