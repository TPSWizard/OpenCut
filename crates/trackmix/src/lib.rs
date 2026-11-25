//! Track mixing, shared by both editors (the web through WASM): each track
//! that carries sound (audio and video tracks) has a mute, a solo, a volume
//! in dB and a pan, applied after each clip's own volume and pan.
//!
//! - A muted track is silent. When any sounding track is soloed, only the
//!   soloed tracks are heard; mute wins over solo.
//! - Volume runs from −60 dB (the bottom of the fader: silent) to +12 dB.
//! - Pan runs from −1 (left) to 1 (right) and pans the clip's stereo after
//!   its own pan, the way Web Audio's two-channel StereoPannerNode does
//!   ([`pan_stereo`]), so the desktop sounds like the web preview.
//!
//! Checked against `fixtures/trackmix/cases.json`.

use serde::{Deserialize, Serialize};

/// The quietest track volume; at or below it the track is silent.
pub const TRACK_VOLUME_MIN_DB: f64 = -60.0;
/// The loudest track volume.
pub const TRACK_VOLUME_MAX_DB: f64 = 12.0;

/// A track's mixer settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TrackSettings {
    pub muted: bool,
    pub solo: bool,
    /// dB; 0 leaves it as it is.
    pub volume: f64,
    /// −1 left to 1 right.
    pub pan: f64,
    /// Whether the track carries sound (audio and video tracks); others
    /// neither play nor count for solo.
    pub sounding: bool,
}

/// How a track plays.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackMix {
    pub audible: bool,
    /// Linear gain (0 when not audible).
    pub gain: f64,
    pub pan: f64,
}

/// A volume within the fader's range (non-numbers are 0 dB).
pub fn clamp_volume(db: f64) -> f64 {
    if db.is_finite() { db.clamp(TRACK_VOLUME_MIN_DB, TRACK_VOLUME_MAX_DB) } else { 0.0 }
}

/// A pan within −1 to 1 (non-numbers are centred).
pub fn clamp_pan(pan: f64) -> f64 {
    if pan.is_finite() { pan.clamp(-1.0, 1.0) } else { 0.0 }
}

/// The linear gain of a track volume; silent at the bottom of the fader.
pub fn volume_gain(db: f64) -> f64 {
    let db = clamp_volume(db);
    if db <= TRACK_VOLUME_MIN_DB { 0.0 } else { 10f64.powf(db / 20.0) }
}

/// How each track plays, in the same order.
pub fn track_mix(tracks: &[TrackSettings]) -> Vec<TrackMix> {
    let soloing = tracks.iter().any(|track| track.sounding && track.solo && !track.muted);
    tracks
        .iter()
        .map(|track| {
            let audible = track.sounding && !track.muted && (!soloing || track.solo);
            let gain = if audible { volume_gain(track.volume) } else { 0.0 };
            TrackMix { audible: audible && gain > 0.0, gain, pan: clamp_pan(track.pan) }
        })
        .collect()
}

/// Pans one stereo frame as Web Audio's StereoPannerNode does for two
/// channels (equal power; at 0 nothing changes).
pub fn pan_stereo(pan: f64, left: f64, right: f64) -> (f64, f64) {
    if pan == 0.0 {
        return (left, right);
    }
    let x = if pan <= 0.0 { pan + 1.0 } else { pan } * std::f64::consts::FRAC_PI_2;
    let (gain_left, gain_right) = (x.cos(), x.sin());
    if pan <= 0.0 { (left + right * gain_left, right * gain_right) } else { (left * gain_left, right + left * gain_right) }
}

/// A frame through the clip's pan, then the track's.
pub fn pan_clip_and_track(clip_pan: f64, track_pan: f64, left: f64, right: f64) -> (f64, f64) {
    let (left, right) = pan_stereo(clip_pan, left, right);
    pan_stereo(track_pan, left, right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(muted: bool, solo: bool, volume: f64) -> TrackSettings {
        TrackSettings { muted, solo, volume, pan: 0.0, sounding: true }
    }

    #[test]
    fn solo_silences_the_rest_and_mute_wins() {
        let mix = track_mix(&[track(false, false, 0.0), track(false, true, -6.0), track(true, true, 0.0)]);
        assert_eq!(mix.iter().map(|m| m.audible).collect::<Vec<_>>(), vec![false, true, false]);
        assert!((mix[1].gain - 0.501187).abs() < 1e-6);
        // A soloed but muted track does not silence the others.
        let mix = track_mix(&[track(false, false, 0.0), track(true, true, 0.0)]);
        assert!(mix[0].audible);
        // Silent tracks (text) neither play nor count.
        let mut text = track(false, true, 0.0);
        text.sounding = false;
        assert!(track_mix(&[track(false, false, 0.0), text])[0].audible);
        assert_eq!(volume_gain(-60.0), 0.0);
        assert_eq!(volume_gain(f64::NAN), 1.0);
        assert!((volume_gain(40.0) - 10f64.powf(0.6)).abs() < 1e-12);
    }

    #[test]
    fn pans_stack_clip_then_track() {
        assert_eq!(pan_stereo(0.0, 0.3, 0.7), (0.3, 0.7));
        let (left, right) = pan_stereo(1.0, 1.0, 0.0);
        assert!(left.abs() < 1e-12 && (right - 1.0).abs() < 1e-12);
        let (left, right) = pan_clip_and_track(-1.0, 1.0, 1.0, 1.0);
        assert!(left.abs() < 1e-12 && (right - 2.0).abs() < 1e-12, "{left} {right}");
    }
}
