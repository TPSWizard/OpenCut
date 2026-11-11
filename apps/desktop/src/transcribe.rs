//! Native speech-to-text on the desktop: whisper.cpp's `whisper-cli`
//! run as a subprocess (the way Shotcut does it), with ggml models fetched
//! from Hugging Face into `~/.opencut/models/` and checked by size on
//! download (the SHA-256 is recorded beside each file). A clip's audio is
//! read through FFmpeg at 16 kHz mono, written as WAV, run with one word
//! per segment, and the words become caption elements with the web app's
//! chunking (`editor::captions`).
//!
//! `whisper-cli` is looked for at `OPENCUT_WHISPER_CLI`, then under
//! `~/.opencut/tools/` (where `scripts/build-whisper-cli.sh` puts it), then
//! on `PATH`.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use editor::{AudioClipPlan, Project, TimedWord};
use media::{AudioReader, AudioSettings, Ffmpeg};
use sha2::{Digest, Sha256};

const WHISPER_SAMPLE_RATE: u32 = 16_000;
const MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// A ggml model the desktop can fetch. Sizes are the published file sizes;
/// a download that ends at another size is thrown away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ModelSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub file: &'static str,
    pub bytes: u64,
}

pub(crate) const MODELS: &[ModelSpec] = &[
    ModelSpec { id: "tiny", label: "Tiny (75 MB)", file: "ggml-tiny.bin", bytes: 77_691_713 },
    ModelSpec { id: "base", label: "Base (142 MB)", file: "ggml-base.bin", bytes: 147_951_465 },
    ModelSpec { id: "small", label: "Small (466 MB)", file: "ggml-small.bin", bytes: 487_601_967 },
];

pub(crate) fn model_spec(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|model| model.id == id)
}

/// The models directory next to the projects root.
pub(crate) fn models_dir(projects_root: &Path) -> PathBuf {
    projects_root
        .parent()
        .map(|parent| parent.join("models"))
        .unwrap_or_else(|| projects_root.join("models"))
}

/// The tools directory next to the projects root (`~/.opencut/tools`).
pub(crate) fn tools_dir(projects_root: &Path) -> PathBuf {
    projects_root
        .parent()
        .map(|parent| parent.join("tools"))
        .unwrap_or_else(|| projects_root.join("tools"))
}

pub(crate) fn model_path(dir: &Path, model: &ModelSpec) -> PathBuf {
    dir.join(model.file)
}

/// Where `whisper-cli` is, if anywhere.
pub(crate) fn whisper_cli(tools: &Path) -> Option<PathBuf> {
    let exe = if cfg!(windows) { "whisper-cli.exe" } else { "whisper-cli" };
    if let Some(path) = std::env::var_os("OPENCUT_WHISPER_CLI").map(PathBuf::from) {
        if path.is_file() {
            return Some(path);
        }
    }
    let candidates = [
        tools.join(exe),
        tools.join("whisper.cpp").join("build").join("bin").join("Release").join(exe),
        tools.join("whisper.cpp").join("build").join("bin").join(exe),
    ];
    if let Some(found) = candidates.into_iter().find(|path| path.is_file()) {
        return Some(found);
    }
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var).map(|dir| dir.join(exe)).find(|path| path.is_file())
}

/// Fetches the model unless it is there already, verifying the size and
/// writing `<file>.sha256` and a provenance note beside it.
pub(crate) fn ensure_model(dir: &Path, model: &ModelSpec, progress: &dyn Fn(u64, u64)) -> Result<PathBuf, String> {
    let target = model_path(dir, model);
    if target.exists() {
        return Ok(target);
    }
    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let url = format!("{MODEL_BASE_URL}/{}", model.file);
    let response = ureq::get(&url).call().map_err(|error| format!("{url}: {error}"))?;
    let partial = dir.join(format!("{}.part", model.file));
    let mut file = fs::File::create(&partial).map_err(|error| error.to_string())?;
    let mut reader = response.into_body().into_reader();
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    let mut done = 0u64;
    loop {
        let got = reader.read(&mut buffer).map_err(|error| format!("{url}: {error}"))?;
        if got == 0 {
            break;
        }
        file.write_all(&buffer[..got]).map_err(|error| error.to_string())?;
        hasher.update(&buffer[..got]);
        done += got as u64;
        progress(done, model.bytes);
        if done > model.bytes + (1 << 20) {
            drop(file);
            fs::remove_file(&partial).ok();
            return Err(format!("{}: the download is larger than the published {} bytes", model.file, model.bytes));
        }
    }
    drop(file);
    if done != model.bytes {
        fs::remove_file(&partial).ok();
        return Err(format!("{}: got {done} bytes, expected {}", model.file, model.bytes));
    }
    let digest = format!("{:x}", hasher.finalize());
    fs::rename(&partial, &target).map_err(|error| error.to_string())?;
    fs::write(dir.join(format!("{}.sha256", model.file)), format!("{digest}  {}\n", model.file)).ok();
    fs::write(
        dir.join(format!("{}.provenance.json", model.file)),
        serde_json::to_string_pretty(&serde_json::json!({
            "file": model.file,
            "source": url,
            "bytes": model.bytes,
            "sha256": digest,
            "license": "MIT (ggml conversions of OpenAI Whisper, which is MIT)",
        }))
        .unwrap_or_default(),
    )
    .ok();
    Ok(target)
}

/// The clip's audio as 16 kHz mono samples, as whisper wants it.
pub(crate) fn clip_samples(ffmpeg: &Arc<Ffmpeg>, clip: &AudioClipPlan) -> Result<Vec<f32>, String> {
    let settings = AudioSettings { sample_rate: WHISPER_SAMPLE_RATE, channels: 1 };
    let start = clip.source_start.to_seconds_f64();
    let end = clip.source_end.to_seconds_f64();
    let mut reader = AudioReader::open(ffmpeg, &clip.path, start, settings).map_err(|error| error.to_string())?;
    let mut remaining = ((end - start).max(0.0) * WHISPER_SAMPLE_RATE as f64).round() as usize;
    let mut samples = Vec::with_capacity(remaining);
    let mut buffer = vec![0.0f32; 16_384];
    while remaining > 0 {
        let want = remaining.min(buffer.len());
        let got = reader.read(&mut buffer[..want]).map_err(|error| error.to_string())?;
        if got == 0 {
            break;
        }
        samples.extend_from_slice(&buffer[..got]);
        remaining -= got;
    }
    Ok(samples)
}

/// Writes samples as a 16-bit mono WAV file.
pub(crate) fn write_wav(path: &Path, samples: &[f32]) -> Result<(), String> {
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&WHISPER_SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(WHISPER_SAMPLE_RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    fs::write(path, bytes).map_err(|error| error.to_string())
}

/// Runs `whisper-cli` on the samples and returns each spoken word with
/// absolute times in seconds (offset by the clip's timeline start).
pub(crate) fn transcribe(
    cli: &Path,
    model: &Path,
    samples: &[f32],
    language: Option<&str>,
    offset_seconds: f64,
) -> Result<Vec<TimedWord>, String> {
    if samples.len() < WHISPER_SAMPLE_RATE as usize / 2 {
        return Ok(Vec::new());
    }
    let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
    let wav = dir.path().join("clip.wav");
    write_wav(&wav, samples)?;
    let out_base = dir.path().join("clip");
    let output = Command::new(cli)
        .arg("-m")
        .arg(model)
        .arg("-f")
        .arg(&wav)
        .args(["-l", language.unwrap_or("auto")])
        .args(["-ml", "1", "-sow", "-oj", "-np", "-nt"])
        .arg("-of")
        .arg(&out_base)
        .output()
        .map_err(|error| format!("{}: {error}", cli.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("whisper-cli failed: {}", stderr.lines().last().unwrap_or("no output")));
    }
    let json = fs::read_to_string(out_base.with_extension("json")).map_err(|error| error.to_string())?;
    Ok(merge_split_tokens(parse_whisper_json(&json, offset_seconds)?))
}

/// The words in `whisper-cli`'s JSON output (`-oj`): one entry per segment
/// with millisecond offsets, which `-ml 1 -sow` makes one word each.
pub(crate) fn parse_whisper_json(json: &str, offset_seconds: f64) -> Result<Vec<TimedWord>, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|error| error.to_string())?;
    let entries = value
        .get("transcription")
        .and_then(|t| t.as_array())
        .ok_or_else(|| "whisper-cli output has no transcription".to_owned())?;
    let mut words = Vec::new();
    for entry in entries {
        let text = entry.get("text").and_then(|t| t.as_str()).unwrap_or("").to_owned();
        if text.trim().is_empty() || text.trim().starts_with('[') || text.trim().starts_with('(') {
            continue;
        }
        let from = entry.pointer("/offsets/from").and_then(|v| v.as_f64()).unwrap_or(0.0) / 1000.0;
        let to = entry.pointer("/offsets/to").and_then(|v| v.as_f64()).unwrap_or(from * 1000.0) / 1000.0;
        words.push(TimedWord { text, start: offset_seconds + from, end: offset_seconds + to.max(from) });
    }
    Ok(words)
}

/// Whisper sometimes splits a word into pieces without a leading space;
/// pieces are joined back, keeping the first start and the last end.
fn merge_split_tokens(words: Vec<TimedWord>) -> Vec<TimedWord> {
    let mut merged: Vec<TimedWord> = Vec::with_capacity(words.len());
    for word in words {
        let joins = merged
            .last()
            .map(|previous| {
                !word.text.starts_with(' ')
                    && word.text.chars().next().is_some_and(|c| c.is_alphanumeric())
                    && previous.text.chars().last().is_some_and(|c| c.is_alphanumeric())
                    && (word.start - previous.end).abs() < 0.001
            })
            .unwrap_or(false);
        if joins {
            let previous = merged.last_mut().unwrap();
            previous.text.push_str(word.text.trim());
            previous.end = word.end.max(previous.end);
        } else {
            merged.push(TimedWord { text: word.text.trim().to_owned(), ..word });
        }
    }
    merged
}

/// The audio clip plan for an element, whatever its track's mute state.
pub(crate) fn clip_plan(project: &Project, element_id: &str) -> Option<AudioClipPlan> {
    let (_, element) = project.element(element_id)?;
    let media_id = element.kind.media_id()?;
    let media = project.media_item(media_id)?;
    if !media.has_audio {
        return None;
    }
    Some(AudioClipPlan {
        element_id: element.id.clone(),
        media_id: media_id.to_owned(),
        path: media.path.clone(),
        timeline_start: element.start_time,
        source_start: element.trim_start,
        source_end: element.trim_start + element.duration,
        volume_db: 0.0,
        volume: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_tokens_join_when_they_touch() {
        let words = vec![
            TimedWord { text: "Cap".into(), start: 0.0, end: 0.2 },
            TimedWord { text: "tions".into(), start: 0.2, end: 0.4 },
            TimedWord { text: " follow".into(), start: 0.5, end: 0.8 },
        ];
        let merged = merge_split_tokens(words);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0], TimedWord { text: "Captions".into(), start: 0.0, end: 0.4 });
        assert_eq!(merged[1].text, "follow");
    }

    #[test]
    fn whisper_json_becomes_words_with_offsets() {
        let json = r#"{"transcription":[
            {"timestamps":{"from":"00:00:00,000","to":"00:00:00,500"},"offsets":{"from":0,"to":500},"text":" The"},
            {"timestamps":{"from":"00:00:00,500","to":"00:00:00,900"},"offsets":{"from":500,"to":900},"text":" quick"},
            {"timestamps":{"from":"00:00:01,000","to":"00:00:01,200"},"offsets":{"from":1000,"to":1200},"text":" [BLANK_AUDIO]"}
        ]}"#;
        let words = parse_whisper_json(json, 10.0).unwrap();
        assert_eq!(words.len(), 2);
        assert_eq!(words[0], TimedWord { text: " The".into(), start: 10.0, end: 10.5 });
        assert_eq!(words[1].end, 10.9);
    }

    #[test]
    fn wav_files_carry_the_header_and_samples() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.wav");
        write_wav(&path, &[0.0, 0.5, -0.5]).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(bytes.len(), 44 + 6);
        assert_eq!(i16::from_le_bytes([bytes[46], bytes[47]]), 16383);
    }

    #[test]
    fn the_models_dir_sits_beside_the_projects() {
        assert_eq!(models_dir(Path::new("/home/x/.opencut/projects")), PathBuf::from("/home/x/.opencut/models"));
        assert_eq!(model_spec("tiny").unwrap().file, "ggml-tiny.bin");
        assert!(model_spec("huge").is_none());
    }
}
