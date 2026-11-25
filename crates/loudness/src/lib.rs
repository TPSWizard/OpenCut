//! Loudness after ITU-R BS.1770-4 and EBU R128, shared by the desktop and
//! the web (through WASM):
//!
//! - [`measure_loudness`]: integrated loudness, loudness range, true peak
//!   and the loudest momentary and short-term windows of some audio.
//! - [`master`]: brings a whole rendered mix to a target (−14, −16, −23 …
//!   LUFS) with its true peaks held under a ceiling by a look-ahead
//!   limiter, as each editor does at export.
//! - [`Meter`]: momentary and short-term loudness and the peak while the
//!   preview plays.
//!
//! Checked against `fixtures/loudness/cases.json`, written by an
//! independent reference.

mod master;
mod measure;
mod meter;

pub use master::{LIMITER_LOOKAHEAD_SECONDS, LIMITER_MARGIN_DB, LIMITER_RELEASE_SECONDS, MasterReport, limit, master};
pub use measure::{
    ABSOLUTE_GATE_LUFS, LOUDNESS_TARGETS, LoudnessReport, LoudnessTarget, deinterleave, gated_loudness, loudness_range, measure_loudness,
    normalization_gain, oversampling_phases, true_peak, true_peak_at, true_peak_phases,
};
pub use meter::{Meter, MeterReadings};
