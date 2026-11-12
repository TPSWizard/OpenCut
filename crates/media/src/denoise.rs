//! Noise reduction through FFmpeg's `afftdn` filter (an FFT denoiser with a
//! fixed noise floor measured from the clip's quiet windows; its own noise
//! tracking is left off, since it gave up on clips with pauses), writing
//! the result as a 16-bit WAV.

use std::path::Path;

use crate::error::MediaError;
use crate::ffmpeg::Ffmpeg;

/// The level of the quietest tenth of 50 ms windows, in dBFS: where the
/// steady background sits when the clip has pauses.
pub fn noise_floor_db(ffmpeg: &Ffmpeg, source: &Path) -> Result<f64, MediaError> {
    let settings = crate::audio::AudioSettings { sample_rate: 16_000, channels: 1 };
    let mut reader = crate::audio::AudioReader::open(ffmpeg, source, 0.0, settings)?;
    let window = 800;
    let mut buffer = vec![0.0f32; window];
    let mut levels = Vec::new();
    loop {
        let got = reader.read(&mut buffer)?;
        if got == 0 {
            break;
        }
        let mean_square = buffer[..got].iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>() / got as f64;
        levels.push(10.0 * (mean_square + 1e-12).log10());
    }
    if levels.is_empty() {
        return Ok(-50.0);
    }
    levels.sort_by(|a, b| a.total_cmp(b));
    Ok(levels[(levels.len() / 10).min(levels.len() - 1)])
}

/// Writes the audio of `source` with its steady noise reduced by about
/// `reduction_db` decibels (clamped to afftdn's 0.01–97 range) to `output`
/// as PCM WAV. The filter's noise floor is set from the clip's quietest
/// windows (`noise_floor_db`, +8 dB, kept between −80 and −15), so the
/// profile fits the recording. Video streams are dropped.
pub fn denoise_to_wav(ffmpeg: &Ffmpeg, source: &Path, output: &Path, reduction_db: f64) -> Result<(), MediaError> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let reduction = reduction_db.clamp(0.01, 97.0);
    let floor = (noise_floor_db(ffmpeg, source)? + 8.0).clamp(-80.0, -15.0);
    let mut command = ffmpeg.command();
    command
        .arg("-y")
        .arg("-i")
        .arg(source)
        .args(["-vn", "-sn", "-dn"])
        .arg("-af")
        .arg(format!("afftdn=nr={reduction:.2}:nf={floor:.1}"))
        .args(["-c:a", "pcm_s16le"])
        .arg(output);
    let result = command.output()?;
    if !result.status.success() {
        return Err(MediaError::ProgramFailed {
            program: "ffmpeg".to_owned(),
            message: crate::ffmpeg::last_stderr_line(&result.stderr),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioReader, AudioSettings};

    /// The level of the loudest tenth of 50 ms windows: the signal, which must survive.
    fn signal_db(ffmpeg: &Ffmpeg, path: &Path) -> f64 {
        let settings = AudioSettings { sample_rate: 16_000, channels: 1 };
        let mut reader = AudioReader::open(ffmpeg, path, 0.0, settings).unwrap();
        let mut buffer = vec![0.0f32; 800];
        let mut levels = Vec::new();
        loop {
            let got = reader.read(&mut buffer).unwrap();
            if got == 0 {
                break;
            }
            let mean_square = buffer[..got].iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>() / got as f64;
            levels.push(10.0 * (mean_square + 1e-12).log10());
        }
        levels.sort_by(|a, b| a.total_cmp(b));
        levels[levels.len() - levels.len() / 10 - 1]
    }

    /// `OPENCUT_DENOISE_CHECK=<noisy audio with pauses>` denoises that file
    /// into the scratch directory and expects the pauses at least 10 dB
    /// quieter with the loud parts within 2 dB.
    #[test]
    fn afftdn_takes_the_hiss_down() {
        let Ok(path) = std::env::var("OPENCUT_DENOISE_CHECK") else { return };
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let source = Path::new(&path);
        let output = std::env::temp_dir().join("opencut-denoise-check.wav");
        denoise_to_wav(&ffmpeg, source, &output, 20.0).unwrap();
        let (floor_before, floor_after) = (noise_floor_db(&ffmpeg, source).unwrap(), noise_floor_db(&ffmpeg, &output).unwrap());
        let (signal_before, signal_after) = (signal_db(&ffmpeg, source), signal_db(&ffmpeg, &output));
        eprintln!("floor {floor_before:.1} -> {floor_after:.1} dB, signal {signal_before:.1} -> {signal_after:.1} dB");
        assert!(floor_before - floor_after > 10.0, "{floor_before} -> {floor_after}");
        assert!((signal_before - signal_after).abs() < 2.0, "{signal_before} -> {signal_after}");
    }
}
