//! Native speech-to-text on the desktop: whisper.cpp's `whisper-cli`
//! run as a subprocess (the way Shotcut does it), with ggml models fetched
//! from Hugging Face into `~/.opencut/models/` and checked by size on
//! download (the SHA-256 is recorded beside each file). A clip's audio is
//! read through FFmpeg at 16 kHz mono, written as WAV, run with DTW
//! token timestamps (flash attention off, which DTW needs), and the tokens
//! are grouped into words that become caption elements with the web app's
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
    model: &ModelSpec,
    model_file: &Path,
    samples: &[f32],
    language: Option<&str>,
    offset_seconds: f64,
    prompt: Option<&str>,
) -> Result<Vec<TimedWord>, String> {
    if samples.len() < WHISPER_SAMPLE_RATE as usize / 2 {
        return Ok(Vec::new());
    }
    let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
    let wav = dir.path().join("clip.wav");
    write_wav(&wav, samples)?;
    let out_base = dir.path().join("clip");
    // Full JSON carries per-token DTW times; DTW needs flash attention off.
    let mut command = Command::new(cli);
    command
        .arg("-m")
        .arg(model_file)
        .arg("-f")
        .arg(&wav)
        .args(["-l", language.unwrap_or("auto")])
        .args(["-ojf", "-np", "-nt", "-nfa", "--dtw", model.id]);
    if let Some(prompt) = prompt {
        command.args(["--prompt", prompt]);
    }
    let output = command
        .arg("-of")
        .arg(&out_base)
        .output()
        .map_err(|error| format!("{}: {error}", cli.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("whisper-cli failed: {}", stderr.lines().last().unwrap_or("no output")));
    }
    let json = fs::read_to_string(out_base.with_extension("json")).map_err(|error| error.to_string())?;
    parse_whisper_json(&json, offset_seconds)
}

/// The words in `whisper-cli`'s full JSON output (`-ojf`): tokens grouped
/// into words at leading spaces, each word starting at its first token's
/// DTW time and ending where the next word starts (the last word keeps a
/// short tail). Without DTW times (`t_dtw` of -1) the tokens' own offsets
/// are used instead.
pub(crate) fn parse_whisper_json(json: &str, offset_seconds: f64) -> Result<Vec<TimedWord>, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|error| error.to_string())?;
    let segments = value
        .get("transcription")
        .and_then(|t| t.as_array())
        .ok_or_else(|| "whisper-cli output has no transcription".to_owned())?;
    let mut pieces: Vec<(String, f64, f64)> = Vec::new();
    for segment in segments {
        let Some(tokens) = segment.get("tokens").and_then(|t| t.as_array()) else {
            // Plain output: one entry per segment.
            let text = segment.get("text").and_then(|t| t.as_str()).unwrap_or("").to_owned();
            let from = segment.pointer("/offsets/from").and_then(|v| v.as_f64()).unwrap_or(0.0) / 1000.0;
            let to = segment.pointer("/offsets/to").and_then(|v| v.as_f64()).unwrap_or(from * 1000.0) / 1000.0;
            if !text.trim().is_empty() {
                pieces.push((text, from, to.max(from)));
            }
            continue;
        };
        for token in tokens {
            let text = token.get("text").and_then(|t| t.as_str()).unwrap_or("");
            if text.is_empty() || text.starts_with("[_") || text.starts_with("<|") {
                continue;
            }
            let dtw = token.get("t_dtw").and_then(|v| v.as_f64()).unwrap_or(-1.0);
            let (start, end) = if dtw >= 0.0 {
                (dtw / 100.0, dtw / 100.0)
            } else {
                (
                    token.pointer("/offsets/from").and_then(|v| v.as_f64()).unwrap_or(0.0) / 1000.0,
                    token.pointer("/offsets/to").and_then(|v| v.as_f64()).unwrap_or(0.0) / 1000.0,
                )
            };
            pieces.push((text.to_owned(), start, end.max(start)));
        }
    }
    // Group tokens into words: a token with a leading space starts one.
    let mut words: Vec<TimedWord> = Vec::new();
    for (text, start, end) in pieces {
        if text.starts_with(' ') || words.is_empty() {
            words.push(TimedWord { text: text.trim().to_owned(), start, end });
        } else if let Some(last) = words.last_mut() {
            last.text.push_str(text.trim());
            last.end = last.end.max(end);
        }
    }
    // A word lasts until the next one starts; the last keeps a short tail.
    for index in 0..words.len() {
        let next_start = words.get(index + 1).map(|next| next.start);
        let word = &mut words[index];
        word.end = match next_start {
            Some(next) if next > word.start => next,
            _ => word.end.max(word.start + 0.3),
        };
    }
    Ok(words
        .into_iter()
        .filter(|word| word.text.chars().any(|c| c.is_alphanumeric()))
        .map(|word| TimedWord { text: word.text, start: offset_seconds + word.start, end: offset_seconds + word.end })
        .collect())
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
        timeline_duration: element.duration,
        source_start: element.trim_start,
        source_end: element.trim_start + element.duration,
        volume_db: 0.0,
        volume: Default::default(),
        retime: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_json_tokens_become_words_with_dtw_times() {
        let json = r#"{"transcription":[{"offsets":{"from":0,"to":30000},"text":" The quick dog. Captions.","tokens":[
            {"text":" The","offsets":{"from":130,"to":1160},"t_dtw":8},
            {"text":" quick","offsets":{"from":1200,"to":2490},"t_dtw":52},
            {"text":" dog","offsets":{"from":6650,"to":6650},"t_dtw":264},
            {"text":".","offsets":{"from":6650,"to":6650},"t_dtw":360},
            {"text":" Capt","offsets":{"from":6650,"to":6650},"t_dtw":382},
            {"text":"ions","offsets":{"from":6650,"to":6650},"t_dtw":400},
            {"text":".","offsets":{"from":6650,"to":6650},"t_dtw":662},
            {"text":"[_EOT_]","offsets":{"from":30000,"to":30000},"t_dtw":-1}
        ]}]}"#;
        let words = parse_whisper_json(json, 2.0).unwrap();
        let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, ["The", "quick", "dog.", "Captions."]);
        assert!((words[0].start - 2.08).abs() < 1e-9);
        assert!((words[0].end - 2.52).abs() < 1e-9, "a word ends where the next starts");
        assert!((words[2].end - words[3].start).abs() < 1e-9);
        assert!(words[3].end > words[3].start);
    }

    #[test]
    fn plain_json_falls_back_to_token_offsets() {
        let json = r#"{"transcription":[{"offsets":{"from":0,"to":30000},"text":" Hi there","tokens":[
            {"text":" Hi","offsets":{"from":100,"to":400},"t_dtw":-1},
            {"text":" there","offsets":{"from":500,"to":900},"t_dtw":-1}
        ]}]}"#;
        let words = parse_whisper_json(json, 0.0).unwrap();
        assert_eq!(words.len(), 2);
        assert!((words[0].start - 0.1).abs() < 1e-9);
        assert!((words[0].end - 0.5).abs() < 1e-9);
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

    /// End to end against the real tool and model: fetches the tiny model if
    /// needed, transcribes `OPENCUT_TRANSCRIBE_CHECK` (a speech file) and
    /// prints the words and captions. Skipped unless that variable is set,
    /// since it downloads 75 MB and needs whisper-cli and FFmpeg.
    #[test]
    fn transcribes_a_speech_file_end_to_end() {
        let Ok(path) = std::env::var("OPENCUT_TRANSCRIBE_CHECK") else {
            return;
        };
        let root = editor::ProjectStore::default_root();
        let cli = whisper_cli(&tools_dir(&root)).expect("whisper-cli built");
        let model = ensure_model(&models_dir(&root), model_spec("tiny").unwrap(), &|done, total| {
            if done % (16 << 20) < (1 << 20) {
                eprintln!("model: {} / {} MB", done >> 20, total >> 20);
            }
        })
        .expect("model fetched");
        let ffmpeg = Arc::new(Ffmpeg::locate().expect("ffmpeg"));
        let info = media::probe(&ffmpeg, Path::new(&path)).expect("probe");
        let clip = AudioClipPlan {
            element_id: "e".into(),
            media_id: "m".into(),
            path: PathBuf::from(&path),
            timeline_start: time::MediaTime::from_seconds_f64(2.0).unwrap(),
            timeline_duration: info.duration.unwrap(),
            source_start: time::MediaTime::ZERO,
            source_end: info.duration.unwrap(),
            volume_db: 0.0,
            volume: Default::default(),
            retime: None,
        };
        let samples = clip_samples(&ffmpeg, &clip).expect("samples");
        let started = std::time::Instant::now();
        let words = transcribe(&cli, model_spec("tiny").unwrap(), &model, &samples, None, 2.0, None).expect("transcribe");
        eprintln!("{} words in {:.1} s:", words.len(), started.elapsed().as_secs_f64());
        for word in &words {
            eprintln!("  {:6.2}-{:6.2} {}", word.start, word.end, word.text);
        }
        let text = words.iter().map(|w| w.text.to_lowercase()).collect::<Vec<_>>().join(" ");
        assert!(text.contains("quick") && text.contains("fox"), "{text}");
        assert!(words.iter().all(|w| w.start >= 2.0 && w.end >= w.start), "offset applied");
        let chunks = editor::build_caption_chunks(&words, 3, 0.8, 1.0);
        for chunk in &chunks {
            eprintln!("caption {:6.2} +{:4.2}: {}", chunk.start, chunk.duration, chunk.text);
        }
        assert!(chunks.len() >= 3);

        // Put the captions on a project with the speech as its audio clip and
        // write it where the headless export test can render it.
        if let Ok(out_dir) = std::env::var("OPENCUT_TRANSCRIBE_PROJECT") {
            let mut project = Project::new("Transcribed");
            project.settings.canvas = editor::CanvasSize { width: 1280, height: 720 };
            editor::apply(
                &mut project,
                &editor::Command::AddMedia(editor::MediaItem {
                    id: "speech".to_owned(),
                    name: "speech.wav".to_owned(),
                    kind: editor::MediaKind::Audio,
                    path: PathBuf::from(&path),
                    duration: info.duration,
                    width: None,
                    height: None,
                    fps: None,
                    has_audio: true,
                }),
            )
            .unwrap();
            let mut clip_element = editor::Element::from_media(project.media_item("speech").unwrap(), clip.timeline_start);
            clip_element.start_time = clip.timeline_start;
            editor::apply(&mut project, &editor::Command::InsertElement { element: clip_element, placement: editor::TrackPlacement::Auto }).unwrap();
            let elements: Vec<editor::Element> = chunks
                .iter()
                .enumerate()
                .map(|(index, chunk)| editor::caption_element(index, chunk, project.settings.canvas))
                .collect();
            editor::apply(&mut project, &editor::Command::InsertCaptions { elements }).unwrap();
            assert_eq!(project.tracks[0].name, "Captions");
            assert_eq!(project.tracks[0].elements.len(), chunks.len());
            let dir = PathBuf::from(out_dir);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("project.json"), serde_json::to_string_pretty(&project).unwrap()).unwrap();
            eprintln!("wrote {}", dir.join("project.json").display());
        }
    }

    #[test]
    fn the_models_dir_sits_beside_the_projects() {
        assert_eq!(models_dir(Path::new("/home/x/.opencut/projects")), PathBuf::from("/home/x/.opencut/models"));
        assert_eq!(model_spec("tiny").unwrap().file, "ggml-tiny.bin");
        assert!(model_spec("huge").is_none());
    }

    /// `OPENCUT_FILLER_CHECK=<speech with "um" and "uh">` transcribes that
    /// file with the hesitation prompt and expects filler words to be found.
    #[test]
    fn prompted_transcription_keeps_fillers() {
        let Ok(path) = std::env::var("OPENCUT_FILLER_CHECK") else { return };
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let tools = tools_dir(&editor::ProjectStore::default_root());
        let Some(cli) = whisper_cli(&tools) else { return };
        let models = models_dir(&editor::ProjectStore::default_root());
        let model = ensure_model(&models, model_spec("tiny").unwrap(), &|_, _| {}).expect("model");
        let settings = media::AudioSettings { sample_rate: WHISPER_SAMPLE_RATE, channels: 1 };
        let mut reader = media::AudioReader::open(&ffmpeg, Path::new(&path), 0.0, settings).expect("open");
        let mut samples = Vec::new();
        let mut buffer = vec![0.0f32; 16_000];
        loop {
            let got = reader.read(&mut buffer).expect("read");
            if got == 0 {
                break;
            }
            samples.extend_from_slice(&buffer[..got]);
        }
        let words = transcribe(&cli, model_spec("tiny").unwrap(), &model, &samples, Some("en"), 0.0, Some(editor::FILLER_PROMPT)).expect("transcribe");
        let text: Vec<String> = words.iter().map(|w| w.text.clone()).collect();
        let fillers = editor::find_fillers(&words, "en", &editor::FillerOptions::default());
        let intervals = editor::filler_intervals(&words, &fillers, &editor::FillerOptions::default());
        eprintln!("words: {}
fillers: {:?}
intervals: {intervals:?}", text.join(" "), fillers.iter().map(|h| (&h.word.text, h.word.start)).collect::<Vec<_>>());
        assert!(!fillers.is_empty(), "no fillers among: {}", text.join(" "));
    }
}
