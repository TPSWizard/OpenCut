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


    /// The multicam grid through the real renderer (row 102): three solid
    /// colour "cameras" made with FFmpeg are grouped, cut to the third at
    /// 1 s, and the grid draws each in its own cell while the program shows
    /// only the live one. Needs FFmpeg and a GPU adapter; skips otherwise.
    #[test]
    fn the_grid_draws_every_angle_in_its_cell() {
        use editor::{Command, Element, MediaItem, MediaKind, Project, TrackPlacement, apply};
        use render::{Renderer, RendererOptions};
        use time::MediaTime;
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let Ok(mut renderer) = Renderer::new(RendererOptions { ffmpeg: Some(ffmpeg.clone()), ..Default::default() }) else {
            eprintln!("no GPU adapter; skipping");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let mut project = Project::new("Grid");
        project.settings.canvas = editor::CanvasSize { width: 320, height: 180 };
        let mut ids = Vec::new();
        for (index, colour) in ["red", "lime", "blue"].iter().enumerate() {
            let path = dir.path().join(format!("{colour}.mp4"));
            let made = std::process::Command::new(ffmpeg.ffmpeg_path())
                .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
                .arg(format!("color=c={colour}:size=320x180:rate=30:duration=3"))
                .args(["-c:v", "libx264", "-pix_fmt", "yuv420p"])
                .arg(&path)
                .output()
                .unwrap();
            assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
            let id = format!("cam{index}");
            apply(
                &mut project,
                &Command::AddMedia(MediaItem {
                    id: id.clone(),
                    name: format!("{colour}.mp4"),
                    kind: MediaKind::Video,
                    path,
                    duration: MediaTime::from_seconds_f64(3.0),
                    width: Some(320),
                    height: Some(180),
                    fps: Some(30.0),
                    has_audio: false,
                }),
            )
            .unwrap();
            let element = Element::from_media(project.media_item(&id).unwrap(), MediaTime::ZERO);
            ids.push(element.id.clone());
            apply(&mut project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
        }
        let lags: Vec<(String, f64)> = ids.iter().map(|id| (id.clone(), 0.0)).collect();
        let commands = editor::multicam_sync_commands(&project, &lags, "grid-test").unwrap();
        apply(&mut project, &Command::Batch(commands)).unwrap();
        let commands = editor::multicam_switch_commands(&project, &ids[0], 1.0, 2).unwrap();
        apply(&mut project, &Command::Batch(commands)).unwrap();

        let pixel = |frame: &render::RenderedFrame, x: f64, y: f64| {
            let index = ((y as usize) * frame.width as usize + x as usize) * 4;
            let bgra = &frame.bgra[index..index + 4];
            (bgra[2], bgra[1], bgra[0])
        };
        let near = |(r, g, b): (u8, u8, u8), want: (u8, u8, u8)| {
            (r as i32 - want.0 as i32).abs() < 40 && (g as i32 - want.1 as i32).abs() < 40 && (b as i32 - want.2 as i32).abs() < 40
        };
        let at = |seconds: f64| MediaTime::from_seconds_f64(seconds).unwrap();
        // The program: red before the cut, blue after.
        let before = renderer.render(&project, at(0.5), 320, 180).unwrap();
        let after = renderer.render(&project, at(1.5), 320, 180).unwrap();
        assert!(near(pixel(&before, 160.0, 90.0), (255, 0, 0)), "{:?}", pixel(&before, 160.0, 90.0));
        assert!(near(pixel(&after, 160.0, 90.0), (0, 0, 255)), "{:?}", pixel(&after, 160.0, 90.0));

        // The grid: every angle in its cell, whichever is live.
        let grid = editor::multicam_grid_project(&project, &ids[1]).unwrap();
        let frame = renderer.render(&grid, at(1.5), 320, 180).unwrap();
        let cells = ::multicam::grid_cells(3, (320.0, 180.0));
        for (cell, want) in cells.iter().zip([(255, 0, 0), (0, 255, 0), (0, 0, 255)]) {
            let centre = (cell[0] + cell[2] / 2.0, cell[1] + cell[3] / 2.0);
            let got = pixel(&frame, centre.0, centre.1);
            assert!(near(got, want), "cell {cell:?}: {got:?} vs {want:?}");
        }
        // The gap between the cells is not a camera.
        let gap = pixel(&frame, 1.0, 90.0);
        assert!(!near(gap, (255, 0, 0)) && !near(gap, (0, 255, 0)) && !near(gap, (0, 0, 255)), "{gap:?}");
        assert_eq!(editor::multicam_live_angle(&project, &ids[0], 1.5).unwrap().0, 2);
    }
}
