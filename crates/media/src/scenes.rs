//! Scene-change scores from FFmpeg's `scdet` filter: the file (or a span
//! of it) is decoded small and every frame whose difference from the one
//! before passes the threshold is reported with its time and score.

use std::path::Path;

use crate::error::MediaError;
use crate::ffmpeg::Ffmpeg;

/// A frame FFmpeg flagged as a scene change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneScore {
    /// Source time in seconds.
    pub seconds: f64,
    /// `scdet`'s score, 0 to 100.
    pub score: f64,
}

/// Runs `scdet` over `[from, to)` seconds of the file at the given threshold
/// (0 to 100; `scdet` reports frames at or above it).
pub fn scene_scores(ffmpeg: &Ffmpeg, path: &Path, from: f64, to: f64, threshold: f64) -> Result<Vec<SceneScore>, MediaError> {
    let from = from.max(0.0);
    let mut command = ffmpeg.command();
    command.args(["-loglevel", "info"]);
    if from > 0.0 {
        command.arg("-ss").arg(format!("{from:.6}"));
    }
    if to > from {
        command.arg("-t").arg(format!("{:.6}", to - from));
    }
    command
        .arg("-i")
        .arg(path)
        .args(["-an", "-sn", "-dn"])
        .arg("-vf")
        .arg(format!("scale=160:-2,scdet=threshold={:.3}", threshold.clamp(0.0, 100.0)))
        .args(["-f", "null", "-"]);
    let output = command.output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(MediaError::ProgramFailed {
            program: "ffmpeg".to_owned(),
            message: crate::ffmpeg::last_stderr_line(&output.stderr),
        });
    }
    Ok(parse_scdet(&stderr)
        .into_iter()
        .map(|score| SceneScore { seconds: score.seconds + from, ..score })
        .collect())
}

/// The `lavfi.scd.score: S, lavfi.scd.time: T` lines of `scdet`'s log.
pub fn parse_scdet(log: &str) -> Vec<SceneScore> {
    let mut scores = Vec::new();
    for line in log.lines() {
        let Some(score_at) = line.find("lavfi.scd.score:") else { continue };
        let Some(time_at) = line.find("lavfi.scd.time:") else { continue };
        let score = line[score_at + "lavfi.scd.score:".len()..]
            .trim_start()
            .split(|c: char| c == ',' || c.is_whitespace())
            .next()
            .and_then(|text| text.parse::<f64>().ok());
        let seconds = line[time_at + "lavfi.scd.time:".len()..]
            .trim()
            .split_whitespace()
            .next()
            .and_then(|text| text.parse::<f64>().ok());
        if let (Some(score), Some(seconds)) = (score, seconds) {
            scores.push(SceneScore { seconds, score });
        }
    }
    scores.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
    scores
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scdet_lines() {
        let log = "[Parsed_scdet_1 @ 0x1] lavfi.scd.score: 30.865, lavfi.scd.time: 2\nnoise\n[Parsed_scdet_1 @ 0x1] lavfi.scd.score: 24.871, lavfi.scd.time: 4.04\n";
        assert_eq!(
            parse_scdet(log),
            vec![SceneScore { seconds: 2.0, score: 30.865 }, SceneScore { seconds: 4.04, score: 24.871 }]
        );
        assert!(parse_scdet("").is_empty());
    }

    /// `OPENCUT_SCENES_CHECK=<video>` scores that file and expects cuts
    /// near 2 s and 4 s (the `.cache/scenes.mp4` fixture).
    #[test]
    fn scores_a_file_with_scdet() {
        let Ok(path) = std::env::var("OPENCUT_SCENES_CHECK") else { return };
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let scores = scene_scores(&ffmpeg, std::path::Path::new(&path), 0.0, 0.0, 10.0).unwrap();
        eprintln!("{scores:?}");
        let times: Vec<f64> = scores.iter().map(|score| score.seconds).collect();
        assert!(times.iter().any(|t| (t - 2.0).abs() < 0.1) && times.iter().any(|t| (t - 4.0).abs() < 0.1), "{times:?}");
        let span = scene_scores(&ffmpeg, std::path::Path::new(&path), 1.0, 3.0, 10.0).unwrap();
        assert_eq!(span.len(), 1, "{span:?}");
        assert!((span[0].seconds - 2.0).abs() < 0.1, "{span:?}");
    }
}
