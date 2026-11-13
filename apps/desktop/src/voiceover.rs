//! Voice-over on the desktop with the computer's own voices: Windows speech
//! (System.Speech through PowerShell), macOS `say`, or `espeak-ng` where it
//! is installed on Linux. Each script line (`editor::split_script`) is
//! spoken to its own file, read back through FFmpeg, and the lines are
//! joined with their pauses into one WAV. The web editor speaks the same
//! lines with Kokoro instead.

use std::path::Path;
use std::process::Command;

use editor::ScriptLine;
use media::{AudioReader, AudioSettings, Ffmpeg};

/// Sample rate of the joined voice-over.
pub(crate) const VOICE_SAMPLE_RATE: u32 = 24_000;

/// The voices this computer can speak with, by the name each tool takes.
pub(crate) fn system_voices() -> Vec<String> {
    let output = if cfg!(windows) {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Add-Type -AssemblyName System.Speech; (New-Object System.Speech.Synthesis.SpeechSynthesizer).GetInstalledVoices() | Where-Object { $_.Enabled } | ForEach-Object { $_.VoiceInfo.Name }",
            ])
            .output()
    } else if cfg!(target_os = "macos") {
        Command::new("say").args(["-v", "?"]).output()
    } else {
        Command::new("espeak-ng").arg("--voices").output()
    };
    let Ok(output) = output else { return Vec::new() };
    if !output.status.success() {
        return Vec::new();
    }
    parse_voices(&String::from_utf8_lossy(&output.stdout))
}

/// Voice names from the tool's listing: one per line on Windows; the first
/// word of each line from `say -v ?`; the "VoiceName" column of espeak-ng.
fn parse_voices(listing: &str) -> Vec<String> {
    let lines = listing.lines().map(str::trim).filter(|line| !line.is_empty());
    if cfg!(windows) {
        lines.map(str::to_owned).collect()
    } else if cfg!(target_os = "macos") {
        lines.filter_map(|line| line.split_whitespace().next().map(str::to_owned)).collect()
    } else {
        lines.skip(1).filter_map(|line| line.split_whitespace().nth(3).map(str::to_owned)).collect()
    }
}

/// Speaks every line to `dir/line<N>.<ext>`; returns their paths in order.
fn speak_lines(dir: &Path, lines: &[ScriptLine], voice: Option<&str>, speed: f64) -> Result<Vec<std::path::PathBuf>, String> {
    let mut outputs = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        let text = dir.join(format!("line{index}.txt"));
        std::fs::write(&text, &line.text).map_err(|error| error.to_string())?;
        outputs.push(dir.join(format!("line{index}.{}", if cfg!(target_os = "macos") { "aiff" } else { "wav" })));
    }
    if cfg!(windows) {
        // One PowerShell for all lines; text and settings travel in files and
        // environment variables, so nothing needs quoting.
        let script = "Add-Type -AssemblyName System.Speech; \
            $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
            if ($env:OC_VOICE) { $s.SelectVoice($env:OC_VOICE) }; \
            $s.Rate = [int]$env:OC_RATE; \
            for ($i = 0; $i -lt [int]$env:OC_COUNT; $i++) { \
              $text = Get-Content -LiteralPath (Join-Path $env:OC_DIR \"line$i.txt\") -Raw -Encoding UTF8; \
              $s.SetOutputToWaveFile((Join-Path $env:OC_DIR \"line$i.wav\")); \
              $s.Speak($text) }; \
            $s.Dispose()";
        let rate = ((speed - 1.0) * 10.0).round().clamp(-10.0, 10.0) as i32;
        let output = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("OC_DIR", dir)
            .env("OC_COUNT", lines.len().to_string())
            .env("OC_RATE", rate.to_string())
            .env("OC_VOICE", voice.unwrap_or(""))
            .output()
            .map_err(|error| format!("Windows speech: {error}"))?;
        if !output.status.success() {
            return Err(format!("Windows speech failed: {}", String::from_utf8_lossy(&output.stderr).lines().last().unwrap_or("")));
        }
    } else {
        let words_per_minute = (175.0 * speed).round().clamp(80.0, 400.0) as i32;
        for (index, out) in outputs.iter().enumerate() {
            let text = dir.join(format!("line{index}.txt"));
            let mut command = if cfg!(target_os = "macos") {
                let mut command = Command::new("say");
                command.arg("-r").arg(words_per_minute.to_string()).arg("-o").arg(out).arg("-f").arg(&text);
                command
            } else {
                let mut command = Command::new("espeak-ng");
                command.arg("-s").arg(words_per_minute.to_string()).arg("-w").arg(out).arg("-f").arg(&text);
                command
            };
            if let Some(voice) = voice {
                command.arg("-v").arg(voice);
            }
            let status = command.status().map_err(|error| format!("text to speech: {error}"))?;
            if !status.success() {
                return Err("text to speech failed".to_owned());
            }
        }
    }
    Ok(outputs)
}

fn read_samples(ffmpeg: &Ffmpeg, path: &Path) -> Result<Vec<f32>, String> {
    let settings = AudioSettings { sample_rate: VOICE_SAMPLE_RATE, channels: 1 };
    let mut reader = AudioReader::open(ffmpeg, path, 0.0, settings).map_err(|error| error.to_string())?;
    let mut samples = Vec::new();
    let mut buffer = vec![0.0f32; 16_384];
    loop {
        let got = reader.read(&mut buffer).map_err(|error| error.to_string())?;
        if got == 0 {
            break;
        }
        samples.extend_from_slice(&buffer[..got]);
    }
    Ok(samples)
}

/// 16-bit mono WAV.
pub(crate) fn write_wav(path: &Path, samples: &[f32], sample_rate: u32) -> Result<(), String> {
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, bytes).map_err(|error| error.to_string())
}

/// Speaks the lines and writes them, joined with their pauses, to `output`.
/// Returns each line's speech length in seconds.
pub(crate) fn speak_to_wav(
    ffmpeg: &Ffmpeg,
    lines: &[ScriptLine],
    voice: Option<&str>,
    speed: f64,
    output: &Path,
) -> Result<Vec<f64>, String> {
    let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
    let files = speak_lines(dir.path(), lines, voice, speed)?;
    let mut joined = Vec::new();
    let mut durations = Vec::with_capacity(lines.len());
    for (line, file) in lines.iter().zip(&files) {
        let samples = read_samples(ffmpeg, file)?;
        durations.push(samples.len() as f64 / VOICE_SAMPLE_RATE as f64);
        joined.extend_from_slice(&samples);
        joined.extend(std::iter::repeat_n(0.0f32, (line.pause_after * VOICE_SAMPLE_RATE as f64).round() as usize));
    }
    write_wav(output, &joined, VOICE_SAMPLE_RATE)?;
    Ok(durations)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `OPENCUT_VOICE_CHECK=1` speaks two lines with the default system
    /// voice and checks the joined file's length against the line lengths
    /// and pauses.
    #[test]
    fn speaks_lines_into_one_wav() {
        if std::env::var("OPENCUT_VOICE_CHECK").is_err() {
            return;
        }
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let voices = system_voices();
        eprintln!("VOICES {voices:?}");
        assert!(!voices.is_empty());
        let lines = editor::split_script("Hello there. This is a voice-over test.", editor::MAX_LINE_CHARS);
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("vo.wav");
        let durations = speak_to_wav(&ffmpeg, &lines, voices.first().map(String::as_str), 1.0, &out).unwrap();
        assert_eq!(durations.len(), 2);
        assert!(durations.iter().all(|d| *d > 0.3), "{durations:?}");
        let (_, total) = editor::layout_lines(&lines, &durations);
        let samples = read_samples(&ffmpeg, &out).unwrap();
        let seconds = samples.len() as f64 / VOICE_SAMPLE_RATE as f64;
        eprintln!("VOICEOVER {durations:?} total {total:.3} file {seconds:.3}");
        assert!((seconds - total).abs() < 0.05, "file {seconds} vs layout {total}");
    }
}
