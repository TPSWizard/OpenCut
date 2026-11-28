//! CMX 3600 EDL (`.edl`).
//!
//! An EDL holds one video channel and audio channels. Written: `TITLE:`,
//! `FCM:` (`DROP FRAME` at 29.97 and 59.94, else `NON-DROP FRAME`), then one
//! event per clip: `NNN  REEL     CHAN  C        src-in src-out rec-in rec-out`
//! with the main video track's clips as `V`, the first two sound channels
//! (the main track's own sound, then the audio tracks) as `A` and `A2`, the
//! record timecode starting at 01:00:00:00 and each file's timecode at
//! 00:00:00:00. A reel name is the file's name in capitals, letters and
//! digits, at most 8; `* FROM CLIP NAME:` and `* SOURCE FILE:` comment lines
//! carry the clip's name and file. A speed change adds an `M2` line (reel,
//! the playing speed in frames a second, the source in). A dissolve into a
//! clip is two lines under one event number: a zero-length cut on the
//! outgoing clip where the dissolve starts, then `D` and its length in
//! frames on the incoming clip, whose source in is moved back by the frames
//! before the cut.
//!
//! Read: the same, whatever the spacing, with `B` and `AA/V` (video and
//! sound; a stereo pair `AA` is one sound), `A1`, and wipes (`W` and a
//! pattern number; written as `W001`); keys and other
//! channels are left out with a note. The frame rate is not in the file:
//! the reader is given it (`FCM: DROP FRAME` picks 29.97 or 59.94 when the
//! given rate counts 30 or 60).

use std::fmt::Write as _;

use crate::{Clip, Media, Rate, Timeline, TrackKind, Transition, frames_to_timecode, timecode_to_frames};

/// A reel name: letters and digits of the file's name (without its
/// extension), in capitals, at most 8; `AX` when none are left.
pub fn reel_name(file: &str) -> String {
    let stem = match file.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => file,
    };
    let reel: String = stem.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).take(8).collect();
    if reel.is_empty() { "AX".to_owned() } else { reel }
}

struct Event<'a> {
    clip: &'a Clip,
    channel: &'static str,
    /// A dissolve from the clip before into this one, with that clip.
    dissolve: Option<(&'a Clip, &'a Transition)>,
}

/// The timeline as a CMX 3600 EDL.
pub fn write(timeline: &Timeline) -> String {
    let rate = timeline.rate;
    let tc = |frames: i64| frames_to_timecode(frames, rate);
    let record_base = timecode_to_frames("01:00:00:00", rate).unwrap_or(0);
    let mut out = String::new();
    let _ = writeln!(out, "TITLE: {}", timeline.name.replace(['\r', '\n'], " "));
    let _ = writeln!(out, "FCM: {}", if rate.drop_frame() { "DROP FRAME" } else { "NON-DROP FRAME" });
    let _ = writeln!(out);

    let main: Vec<&Clip> = timeline.video_tracks().next().map(|t| t.clips.iter().collect()).unwrap_or_default();
    let has_audio = |clip: &Clip| timeline.media(&clip.media_id).is_some_and(|m| m.has_audio);
    let mut channels: Vec<Vec<&Clip>> = Vec::new();
    let main_sound: Vec<&Clip> = main.iter().copied().filter(|c| c.sound && has_audio(c)).collect();
    if !main_sound.is_empty() {
        channels.push(main_sound);
    }
    for track in timeline.audio_tracks() {
        channels.push(track.clips.iter().collect());
    }
    let mut events: Vec<Event> = Vec::new();
    for (clips, channel) in std::iter::once((main.clone(), "V")).chain(channels.iter().take(2).cloned().zip(["A", "A2"])) {
        // A hidden clip has no place in an EDL.
        let clips: Vec<&Clip> = clips.into_iter().filter(|c| c.enabled).collect();
        for (i, clip) in clips.iter().enumerate() {
            let dissolve = i
                .checked_sub(1)
                .map(|p| clips[p])
                .filter(|previous| previous.end() == clip.start)
                .and_then(|previous| previous.transition.as_ref().map(|t| (previous, t)));
            events.push(Event { clip, channel, dissolve });
        }
    }
    events.sort_by_key(|e| (e.clip.start, ["V", "A", "A2"].iter().position(|c| *c == e.channel)));

    let reel_of = |clip: &Clip| reel_name(&timeline.media(&clip.media_id).map_or(clip.name.clone(), |m| m.name.clone()));
    let mut number = 0;
    for (index, event) in events.iter().enumerate() {
        number += 1;
        let clip = event.clip;
        let reel = reel_of(clip);
        // A clip with a dissolve out of it ends where the dissolve starts.
        let trimmed_end = events
            .iter()
            .skip(index + 1)
            .find(|later| later.channel == event.channel && later.dissolve.is_some_and(|(from, _)| std::ptr::eq(from, clip)))
            .and_then(|later| later.dissolve.map(|(_, t)| t.before()))
            .unwrap_or(0);
        let speed_frames = |frames: i64| (frames as f64 * clip.speed.abs()).round() as i64;
        let (rec_in, mut src_in, trans) = match event.dissolve {
            Some((_, t)) => (clip.start - t.before(), clip.source_start - speed_frames(t.before()), Some(t)),
            None => (clip.start, clip.source_start, None),
        };
        if src_in < 0 {
            src_in = 0;
        }
        let rec_out = clip.end() - trimmed_end;
        let src_out = src_in + speed_frames(rec_out - rec_in);
        if let (Some(t), Some((from, _))) = (trans, event.dissolve) {
            let from_src = from.source_start + speed_frames(from.duration - t.before());
            let _ = writeln!(
                out,
                "{number:03}  {:<8} {:<5} {:<4} {:>3} {} {} {} {}",
                reel_of(from),
                event.channel,
                "C",
                "",
                tc(from_src),
                tc(from_src),
                tc(record_base + rec_in),
                tc(record_base + rec_in)
            );
            let _ = writeln!(
                out,
                "{number:03}  {:<8} {:<5} {:<4} {:03} {} {} {} {}",
                reel,
                event.channel,
                if t.kind == "wipe" { "W001" } else { "D" },
                t.frames,
                tc(src_in),
                tc(src_out),
                tc(record_base + rec_in),
                tc(record_base + rec_out)
            );
        } else {
            let _ = writeln!(
                out,
                "{number:03}  {:<8} {:<5} {:<4} {:>3} {} {} {} {}",
                reel,
                event.channel,
                "C",
                "",
                tc(src_in),
                tc(src_out),
                tc(record_base + rec_in),
                tc(record_base + rec_out)
            );
        }
        if (clip.speed - 1.0).abs() > 1e-9 {
            let _ = writeln!(out, "M2   {:<8}       {:05.1}                {}", reel, rate.fps() * clip.speed, tc(src_in));
        }
        let _ = writeln!(out, "* FROM CLIP NAME: {}", clip.name);
        if let Some(media) = timeline.media(&clip.media_id) {
            let _ = writeln!(out, "* SOURCE FILE: {}", media.url);
        }
        let _ = writeln!(out);
    }
    for note in timeline.notes.iter().chain(&crate::format_notes(timeline, crate::Format::Edl)) {
        let _ = writeln!(out, "* NOTE: {note}");
    }
    out
}

struct Line {
    number: String,
    reel: String,
    channel: String,
    transition: String,
    frames: i64,
    src_in: i64,
    src_out: i64,
    rec_in: i64,
    rec_out: i64,
    name: Option<String>,
    file: Option<String>,
    speed: Option<f64>,
}

/// Reads a CMX 3600 EDL at `rate`.
pub fn read(text: &str, rate: Rate) -> Result<Timeline, String> {
    let mut rate = rate;
    let mut title = "Timeline".to_owned();
    let mut lines: Vec<Line> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    for raw in text.trim_start_matches('\u{feff}').lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("TITLE:") {
            title = rest.trim().to_owned();
            continue;
        }
        if let Some(rest) = line.strip_prefix("FCM:") {
            let base = rate.timecode_base();
            if rest.trim().eq_ignore_ascii_case("DROP FRAME") && matches!(base, 30 | 60) {
                rate = Rate::new(base * 1000, 1001);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix('*') {
            let rest = rest.trim();
            let upper = rest.to_ascii_uppercase();
            if let Some(last) = lines.last_mut() {
                if upper.starts_with("FROM CLIP NAME:") {
                    last.name = Some(rest["FROM CLIP NAME:".len()..].trim().to_owned());
                } else if upper.starts_with("SOURCE FILE:") {
                    last.file = Some(rest["SOURCE FILE:".len()..].trim().to_owned());
                }
            }
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.first() == Some(&"M2") {
            // M2 reel speed source-in: applies to the event with that reel and source in.
            if tokens.len() >= 4
                && let Ok(fps) = tokens[2].parse::<f64>()
                && let Some(src) = timecode_to_frames(tokens[3], rate)
            {
                let reel = tokens[1];
                if let Some(event) = lines.iter_mut().rev().find(|e| e.reel == reel && e.src_in == src) {
                    event.speed = Some(fps / rate.fps());
                }
            }
            continue;
        }
        if tokens.len() < 8 || !tokens[0].chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let (reel, channel, transition) = (tokens[1], tokens[2], tokens[3]);
        let has_frames = !transition.eq_ignore_ascii_case("C");
        let times = &tokens[if has_frames { 5 } else { 4 }..];
        if times.len() < 4 {
            continue;
        }
        let parse = |t: &str| timecode_to_frames(t, rate);
        let (Some(src_in), Some(src_out), Some(rec_in), Some(rec_out)) = (parse(times[0]), parse(times[1]), parse(times[2]), parse(times[3])) else {
            continue;
        };
        lines.push(Line {
            number: tokens[0].to_owned(),
            reel: reel.to_owned(),
            channel: channel.to_ascii_uppercase(),
            transition: transition.to_ascii_uppercase(),
            frames: if has_frames { tokens[4].parse().unwrap_or(0) } else { 0 },
            src_in,
            src_out,
            rec_in,
            rec_out,
            name: None,
            file: None,
            speed: None,
        });
    }
    if lines.is_empty() {
        return Err("No edit events were found in the EDL".to_owned());
    }
    // The record timecode starts at a whole hour (01:00:00:00 as a rule).
    let hour = rate.timecode_base() as i64 * 3600;
    let hour = timecode_to_frames("01:00:00:00", rate).filter(|_| hour > 0).unwrap_or(hour);
    let first = lines.iter().map(|l| l.rec_in).min().unwrap_or(0);
    let origin = if hour > 0 { (first / hour) * hour } else { 0 };

    let mut timeline = Timeline { name: title, rate, width: 0, height: 0, media: Vec::new(), tracks: Vec::new(), notes: Vec::new() };
    let media_for = |timeline: &mut Timeline, line: &Line| -> String {
        let name = line.file.as_deref().map(crate::file_name).or_else(|| line.name.clone()).unwrap_or_else(|| line.reel.clone());
        let url = line.file.clone().unwrap_or_else(|| name.clone());
        if let Some(media) = timeline.media.iter().find(|m| m.url == url) {
            return media.id.clone();
        }
        let id = format!("m{}", timeline.media.len() + 1);
        timeline.media.push(Media { id: id.clone(), name, url, duration: None, has_video: false, has_audio: false, still: false, width: None, height: None });
        id
    };
    let mut video: Vec<Clip> = Vec::new();
    let mut audio: [Vec<Clip>; 2] = [Vec::new(), Vec::new()];
    for (index, line) in lines.iter().enumerate() {
        // The cut line of a dissolve pair only says where the outgoing clip is.
        if line.transition == "C"
            && line.rec_in == line.rec_out
            && lines.get(index + 1).is_some_and(|next| next.number == line.number && next.transition != "C")
        {
            continue;
        }
        let (to_video, to_audio): (bool, Vec<usize>) = match line.channel.as_str() {
            "V" => (true, vec![]),
            "A" | "A1" => (false, vec![0]),
            "A2" => (false, vec![1]),
            // A stereo pair is one sound.
            "AA" => (false, vec![0]),
            "B" | "A/V" | "V/A" => (true, vec![0]),
            "AA/V" | "V/AA" => (true, vec![0]),
            _ => {
                notes.push("Events on channels other than video and the first two audio channels were left out.".to_owned());
                continue;
            }
        };
        if line.transition.starts_with('K') {
            notes.push("Keys were left out.".to_owned());
            continue;
        }
        let media_id = media_for(&mut timeline, line);
        if let Some(media) = timeline.media.iter_mut().find(|m| m.id == media_id) {
            media.has_video |= to_video;
            media.has_audio |= !to_audio.is_empty();
        }
        // Without an M2 line, a source longer or shorter than the record is a speed change.
        let (source, record) = (line.src_out - line.src_in, line.rec_out - line.rec_in);
        let speed = line.speed.unwrap_or(if record > 0 && source > 0 && source != record && line.transition == "C" {
            source as f64 / record as f64
        } else {
            1.0
        });
        let mut clip = Clip {
            name: line.name.clone().unwrap_or_else(|| line.reel.clone()),
            media_id,
            start: line.rec_in - origin,
            duration: line.rec_out - line.rec_in,
            source_start: line.src_in,
            speed,
            sound: false,
            enabled: true,
            transition: None,
        };
        let dissolve = line.transition != "C" && line.frames > 0;
        let place = |clips: &mut Vec<Clip>, mut clip: Clip| {
            if dissolve {
                let t = Transition { frames: line.frames, kind: if line.transition.starts_with('W') { "wipe" } else { "dissolve" }.to_owned() };
                let before = t.before();
                if let Some(previous) = clips.last_mut().filter(|p| p.end() == clip.start) {
                    // Back to a cut at the dissolve's centre.
                    previous.duration += before;
                    previous.transition = Some(t);
                    clip.start += before;
                    clip.duration -= before;
                    clip.source_start += (before as f64 * clip.speed.abs()).round() as i64;
                }
            }
            clips.push(clip);
        };
        if to_video {
            place(&mut video, clip.clone());
        }
        for channel in to_audio {
            clip.sound = true;
            place(&mut audio[channel], clip.clone());
        }
    }
    let mut tracks = vec![crate::Track { kind: TrackKind::Video, name: "V1".to_owned(), clips: video }];
    for (i, clips) in audio.into_iter().enumerate() {
        if !clips.is_empty() {
            tracks.push(crate::Track { kind: TrackKind::Audio, name: format!("A{}", i + 1), clips });
        }
    }
    timeline.tracks = tracks;
    for note in notes {
        timeline.note(note);
    }
    crate::fold_sound(&mut timeline);
    Ok(timeline)
}
