//! Voice effects on a file's audio: decoded through FFmpeg at 48 kHz in its
//! own channel layout (mono stays mono), each channel run through the
//! shared `voice` crate (the web runs the same code as WebAssembly), the
//! channels kept together under full scale, and written as 16-bit WAV.

use std::io::Write;
use std::path::Path;

use crate::audio::{AudioReader, AudioSettings};
use crate::error::MediaError;
use crate::ffmpeg::Ffmpeg;

pub const VOICE_SAMPLE_RATE: u32 = 48_000;

/// Writes `source`'s audio through `effect` (an id of `voice::VOICE_EFFECTS`) to `output`.
pub fn voice_effect_to_wav(ffmpeg: &Ffmpeg, source: &Path, output: &Path, effect: &str, channels: u16) -> Result<(), MediaError> {
    if !voice::is_voice_effect(effect) {
        return Err(MediaError::ProgramFailed { program: "voice".to_owned(), message: format!("unknown voice effect {effect}") });
    }
    let channels = channels.clamp(1, 2);
    let settings = AudioSettings { sample_rate: VOICE_SAMPLE_RATE, channels };
    let mut reader = AudioReader::open(ffmpeg, source, 0.0, settings)?;
    let mut interleaved: Vec<f32> = Vec::new();
    let mut buffer = vec![0f32; 48_000 * channels as usize];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        interleaved.extend_from_slice(&buffer[..read]);
    }
    if interleaved.is_empty() {
        return Err(MediaError::NoStream { path: source.to_path_buf(), stream: "audio" });
    }
    let count = channels as usize;
    let mut planes: Vec<Vec<f32>> = (0..count)
        .map(|channel| {
            let plane: Vec<f32> = interleaved.iter().skip(channel).step_by(count).copied().collect();
            voice::apply_voice_effect(effect, &plane, VOICE_SAMPLE_RATE as f32).unwrap_or(plane)
        })
        .collect();
    voice::normalize_peak(&mut planes, 0.98);
    write_wav(output, &planes, VOICE_SAMPLE_RATE)
}

/// 16-bit PCM WAV of the channel planes (equal lengths).
pub fn write_wav(output: &Path, planes: &[Vec<f32>], sample_rate: u32) -> Result<(), MediaError> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let channels = planes.len() as u16;
    let frames = planes.iter().map(Vec::len).min().unwrap_or(0);
    let data_len = (frames * planes.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * channels as u32 * 2).to_le_bytes());
    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for frame in 0..frames {
        for plane in planes {
            let value = (plane[frame].clamp(-1.0, 1.0) * 32767.0).round() as i16;
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut file = std::fs::File::create(output)?;
    file.write_all(&bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robot_voice_from_a_lavfi_tone() {
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("tone.wav");
        let status = ffmpeg
            .command()
            .args(["-f", "lavfi", "-i", "sine=frequency=300:duration=1:sample_rate=44100", "-y"])
            .arg(&source)
            .output()
            .unwrap();
        assert!(status.status.success());
        let output = dir.path().join("out").join("robot.wav");
        voice_effect_to_wav(&ffmpeg, &source, &output, "robot", 1).unwrap();
        let bytes = std::fs::read(&output).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 1, "mono stays mono");
        let frames = (bytes.len() - 44) / 2;
        assert!((frames as i64 - 48_000).abs() < 200, "one second at 48 kHz: {frames}");
        assert!(voice_effect_to_wav(&ffmpeg, &source, &output, "nope", 1).is_err());
    }
}
