//! Timeline files on the desktop (row 111): reading an OpenTimelineIO,
//! FCPXML or EDL file and finding the files it uses that the project does
//! not have yet, where the timeline says they are or beside the timeline
//! file, probed with FFmpeg. The shared `interchange` crate reads and
//! writes the formats; `editor::project_timeline` and
//! `editor::timeline_import` map the project.

use std::path::Path;

use editor::MediaItem;
use interchange::{Rate, Timeline};
use media::Ffmpeg;

use crate::i18n::t;

/// The timeline in `path` (read at `rate` when the format does not say)
/// and, by the timeline's media id, the files it uses that are not among
/// `known` but were found on disk.
pub(crate) fn read_timeline_file(
    path: &Path,
    rate: Rate,
    known: &[MediaItem],
    ffmpeg: Option<&Ffmpeg>,
) -> Result<(Timeline, Vec<(String, MediaItem)>), String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let format = interchange::Format::sniff(&name, &text).ok_or_else(|| t("That is not an OpenTimelineIO, FCPXML or EDL file.").to_owned())?;
    let timeline = interchange::read(&text, format, rate)?;
    let folder = path.parent().map(Path::to_path_buf);
    let mut brought = Vec::new();
    for media in &timeline.media {
        if editor::find_media(known, media).is_some() {
            continue;
        }
        let given = std::path::PathBuf::from(interchange::url_to_path(&media.url));
        let beside = folder.as_ref().map(|f| f.join(&media.name));
        let found = [Some(given), beside].into_iter().flatten().find(|p| p.is_absolute() && p.is_file());
        if let Some(found) = found
            && let Ok(item) = crate::state::probe_media(ffmpeg, found)
        {
            brought.push((media.id.clone(), item));
        }
    }
    Ok((timeline, brought))
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor::{Command, Element, Project, TrackPlacement, apply};
    use time::MediaTime;

    fn seconds(value: f64) -> MediaTime {
        MediaTime::from_seconds_f64(value).unwrap()
    }

    /// A project with a clip of a real file is written in each format, then
    /// read into a new project that does not have the file: it is found
    /// beside the timeline, probed, and the clip comes back as it was.
    #[test]
    fn exported_timelines_come_back_with_their_files_found_beside_them() {
        let Ok(ffmpeg) = Ffmpeg::locate() else { return };
        let dir = tempfile::tempdir().unwrap();
        let clip = dir.path().join("take one.mp4");
        let made = std::process::Command::new(ffmpeg.ffmpeg_path())
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc2=s=320x180:r=30:d=6"])
            .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=6", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
            .arg(&clip)
            .output()
            .unwrap();
        assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
        let item = crate::state::probe_media(Some(&ffmpeg), clip.clone()).unwrap();

        let mut project = Project::new("Round trip");
        apply(&mut project, &Command::AddMedia(item.clone())).unwrap();
        let mut a = Element::from_media(&item, seconds(0.0));
        a.trim_start = seconds(1.0);
        a.duration = seconds(2.0);
        let mut b = Element::from_media(&item, seconds(2.0));
        b.trim_start = seconds(4.0);
        b.duration = seconds(1.0);
        for element in [a, b] {
            apply(&mut project, &Command::InsertElement { element, placement: TrackPlacement::Auto }).unwrap();
        }
        let timeline = editor::project_timeline(&project);
        // The file the timeline names is moved: only the copy beside the timeline is found.
        let elsewhere = dir.path().join("cuts");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::copy(&clip, elsewhere.join("take one.mp4")).unwrap();
        std::fs::remove_file(&clip).unwrap();
        for format in [interchange::Format::Otio, interchange::Format::Fcpxml, interchange::Format::Edl] {
            let file = elsewhere.join(format!("cut.{}", format.extension()));
            std::fs::write(&file, interchange::write(&timeline, format)).unwrap();
            let (back, brought) = read_timeline_file(&file, timeline.rate, &[], Some(&ffmpeg)).unwrap();
            assert_eq!(brought.len(), 1, "{format:?}: the file is found beside the timeline");
            assert!(brought[0].1.path.starts_with(&elsewhere), "{format:?}");
            let mut target = Project::new("Imported");
            let import = editor::timeline_import(&target, &back, |m| brought.iter().find(|(id, _)| *id == m.id).map(|(_, i)| i.clone()));
            assert!(import.missing.is_empty(), "{format:?}");
            apply(&mut target, &import.command.unwrap()).unwrap();
            let main = &target.tracks[target.main_track_index()].elements;
            let got: Vec<(f64, f64, f64)> =
                main.iter().map(|e| (e.start_time.to_seconds_f64(), e.trim_start.to_seconds_f64(), e.duration.to_seconds_f64())).collect();
            assert_eq!(got, vec![(0.0, 1.0, 2.0), (2.0, 4.0, 1.0)], "{format:?}");
            assert!(main.iter().all(|e| !e.params.bool("muted", false)), "{format:?}: the sound stays on");
        }
        let junk = elsewhere.join("notes.txt");
        std::fs::write(&junk, "hello").unwrap();
        assert!(read_timeline_file(&junk, timeline.rate, &[], None).is_err());
    }
}
