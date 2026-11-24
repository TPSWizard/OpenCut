//! Multicam on the desktop: each selected angle's sound over the part of
//! its file it plays is read with FFmpeg (mono, 8 kHz), turned into onsets
//! and lined up with the first angle's by the shared `multicam` crate. The
//! web editor does the same (`apps/web/src/multicam`).

use std::path::PathBuf;

use media::{AudioReader, AudioSettings, Ffmpeg};

/// Sound is read at this rate for syncing.
pub(crate) const SYNC_RATE: u32 = 8_000;
/// At most this much of each angle is listened to.
pub(crate) const SYNC_MAX_SECONDS: f64 = 600.0;

/// One angle to listen to.
pub(crate) struct SyncAngle {
    pub element_id: String,
    pub name: String,
    pub path: PathBuf,
    /// Source seconds the clip plays from, and for how long.
    pub from: f64,
    pub length: f64,
}

/// Each angle's lag from the first (0 for the first), or why it could not
/// be synced.
pub(crate) fn sync_lags(ffmpeg: &Ffmpeg, angles: &[SyncAngle]) -> Result<Vec<(String, f64)>, String> {
    let mut onsets = Vec::new();
    for angle in angles {
        let length = angle.length.min(SYNC_MAX_SECONDS);
        let mut reader = AudioReader::open(ffmpeg, &angle.path, angle.from, AudioSettings { sample_rate: SYNC_RATE, channels: 1 })
            .map_err(|error| error.to_string())?;
        let wanted = (length * SYNC_RATE as f64) as usize;
        let mut samples = vec![0.0f32; wanted];
        let mut filled = 0;
        while filled < wanted {
            let read = reader.read(&mut samples[filled..]).map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            filled += read;
        }
        samples.truncate(filled);
        if samples.len() < SYNC_RATE as usize {
            return Err(format!("{} has less than a second of sound", angle.name));
        }
        onsets.push(multicam::onsets(&samples, SYNC_RATE as f64));
    }
    let max_lag = angles.iter().map(|angle| angle.length.min(SYNC_MAX_SECONDS)).fold(0.0, f64::max);
    let mut lags = vec![(angles[0].element_id.clone(), 0.0)];
    for (angle, other) in angles.iter().zip(&onsets).skip(1) {
        let result = multicam::sync_lag(&onsets[0], other, max_lag).ok_or_else(|| format!("{} could not be compared", angle.name))?;
        if result.score < multicam::MIN_SYNC_SCORE {
            return Err(format!("{}'s sound does not match the first clip's well enough to sync", angle.name));
        }
        lags.push((angle.element_id.clone(), result.lag));
    }
    Ok(lags)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two "cameras" made with FFmpeg from one sound: the second starts
    /// 0.75 s into it, so it lines up 0.75 s later than the first.
    #[test]
    fn lines_up_two_recordings_of_one_sound() {
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let dir = tempfile::tempdir().unwrap();
        let sound = "aevalsrc='0.6*sin(2*PI*440*t)*between(mod(t*1.7,1.3),0,0.12)+0.4*sin(2*PI*660*t)*between(mod(t*2.3,1.1),0,0.08)':s=8000:d=8";
        let make = |name: &str, filter: &str| {
            let path = dir.path().join(name);
            let made = std::process::Command::new(ffmpeg.ffmpeg_path())
                .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", sound, "-af", filter])
                .arg(&path)
                .output()
                .unwrap();
            assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
            path
        };
        let first = make("a.wav", "volume=1");
        let second = make("b.wav", "atrim=start=0.75,asetpts=PTS-STARTPTS,volume=0.5");
        let angles = [
            SyncAngle { element_id: "a".into(), name: "a".into(), path: first, from: 0.0, length: 6.0 },
            SyncAngle { element_id: "b".into(), name: "b".into(), path: second, from: 0.0, length: 6.0 },
        ];
        let lags = sync_lags(&ffmpeg, &angles).unwrap();
        assert!((lags[1].1 - 0.75).abs() < 0.01, "{lags:?}");
    }
}
