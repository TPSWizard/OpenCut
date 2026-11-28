//! FCPXML 1.10 (`.fcpxml`).
//!
//! Written: one `format` (frame duration, size) and an `asset` per file
//! (with a `media-rep` `src`), then `library` → `event` → `project` →
//! `sequence` → `spine`. The main video track is the spine: `asset-clip`s
//! and `gap`s back to back (`offset` in the spine's time, `start` where the
//! clip starts in its file, `duration`), with a `transition` centred on a
//! cut that has a dissolve (it overlaps the clips' handles and takes no
//! time). Every other track's clips are connected clips: an `asset-clip`
//! with `lane` (1, 2, … for the video tracks above; −1, −2, … for the
//! audio tracks) inside the spine item under its start, its `offset` in
//! that item's own time. A video clip whose sound is off has
//! `srcEnable="video"`; a video file's sound on an audio track has
//! `srcEnable="audio"`. A speed change is a `timeMap` with two `timept`s
//! (`time` the clip's time, `value` the file's). Times are rational
//! seconds (`1001/30000s`).
//!
//! Read: the same, plus `clip` elements wrapping `video` / `audio`
//! references, gaps with connected clips, secondary storylines (a `spine`
//! with a lane; its clips laid from its offset), assets with a timecode
//! start, and `src` on the asset itself (older versions). Titles,
//! generators, compound and multicam clips are left out with a note.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::{Clip, Media, Rate, Timeline, TrackKind, Transition};

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a.abs().max(1) } else { gcd(b, a % b) }
}

/// `frames` at `rate` as rational seconds: `0s`, `5s`, `1001/30000s`.
pub fn time_text(frames: i64, rate: Rate) -> String {
    if frames == 0 {
        return "0s".to_owned();
    }
    let (n, d) = (frames * rate.den as i64, rate.num as i64);
    let g = gcd(n, d);
    let (n, d) = (n / g, d / g);
    if d == 1 { format!("{n}s") } else { format!("{n}/{d}s") }
}

/// Rational or decimal seconds (`1001/30000s`, `5s`, `2.5s`).
pub fn parse_time(text: &str) -> Option<f64> {
    let text = text.trim().strip_suffix('s')?;
    match text.split_once('/') {
        Some((n, d)) => {
            let (n, d) = (n.trim().parse::<f64>().ok()?, d.trim().parse::<f64>().ok()?);
            (d != 0.0).then_some(n / d)
        }
        None => text.parse::<f64>().ok(),
    }
    .filter(|value| value.is_finite())
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

struct Connected<'a> {
    clip: &'a Clip,
    lane: i32,
    audio_only: bool,
}

/// The spine item a connected clip hangs from: its index, or past the end.
fn parent_of(spine: &[(i64, i64)], start: i64) -> Option<usize> {
    spine.iter().position(|&(from, to)| start >= from && start < to)
}

/// The timeline as FCPXML 1.10.
pub fn write(timeline: &Timeline) -> String {
    let rate = timeline.rate;
    let t = |frames: i64| time_text(frames, rate);
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE fcpxml>\n<fcpxml version=\"1.10\">\n");
    let all_notes: Vec<String> = timeline.notes.iter().cloned().chain(crate::format_notes(timeline, crate::Format::Fcpxml)).collect();
    if !all_notes.is_empty() {
        let notes = all_notes.join(" ").replace("--", "-");
        let _ = writeln!(out, "    <!-- ConzaCut: {notes} -->");
    }
    out.push_str("    <resources>\n");
    let (width, height) = (if timeline.width > 0 { timeline.width } else { 1920 }, if timeline.height > 0 { timeline.height } else { 1080 });
    let _ = writeln!(out, "        <format id=\"r1\" frameDuration=\"{}\" width=\"{width}\" height=\"{height}\"/>", time_text(1, rate));
    let mut asset_ids: HashMap<&str, String> = HashMap::new();
    for (i, media) in timeline.media.iter().enumerate() {
        let id = format!("r{}", i + 2);
        let duration = if media.still { 0 } else { media.duration.unwrap_or(0) };
        let _ = writeln!(
            out,
            "        <asset id=\"{id}\" name=\"{}\" start=\"0s\" duration=\"{}\" hasVideo=\"{}\" hasAudio=\"{}\" format=\"r1\">",
            escape(&media.name),
            t(duration),
            u8::from(media.has_video),
            u8::from(media.has_audio),
        );
        let _ = writeln!(out, "            <media-rep kind=\"original-media\" src=\"{}\"/>", escape(&crate::path_to_url(&media.url)));
        out.push_str("        </asset>\n");
        asset_ids.insert(media.id.as_str(), id);
    }
    out.push_str("    </resources>\n    <library>\n        <event name=\"ConzaCut\">\n");
    let duration = timeline.duration();
    let _ = writeln!(out, "            <project name=\"{}\">", escape(&timeline.name));
    let _ = writeln!(
        out,
        "                <sequence format=\"r1\" duration=\"{}\" tcStart=\"0s\" tcFormat=\"{}\">",
        t(duration),
        if rate.drop_frame() { "DF" } else { "NDF" }
    );
    out.push_str("                    <spine>\n");

    // The spine: the main video track's clips with gaps between, to the end.
    let main: Vec<&Clip> = timeline.video_tracks().next().map(|track| track.clips.iter().collect()).unwrap_or_default();
    enum Item<'a> {
        Clip(&'a Clip),
        Gap,
    }
    let mut spine: Vec<(i64, i64, Item)> = Vec::new();
    let mut cursor = 0;
    for clip in &main {
        if clip.start > cursor {
            spine.push((cursor, clip.start, Item::Gap));
        }
        spine.push((clip.start, clip.end(), Item::Clip(clip)));
        cursor = clip.end();
    }
    let mut connected: Vec<Connected> = Vec::new();
    for (index, track) in timeline.video_tracks().enumerate().skip(1) {
        for clip in &track.clips {
            connected.push(Connected { clip, lane: index as i32, audio_only: false });
        }
    }
    for (index, track) in timeline.audio_tracks().enumerate() {
        for clip in &track.clips {
            connected.push(Connected { clip, lane: -(index as i32 + 1), audio_only: true });
        }
    }
    let end = connected.iter().map(|c| c.clip.end()).max().unwrap_or(0).max(cursor);
    if end > cursor {
        spine.push((cursor, end, Item::Gap));
    }
    let ranges: Vec<(i64, i64)> = spine.iter().map(|(a, b, _)| (*a, *b)).collect();
    let mut children: Vec<Vec<&Connected>> = vec![Vec::new(); spine.len()];
    for item in &connected {
        if let Some(parent) = parent_of(&ranges, item.clip.start) {
            children[parent].push(item);
        }
    }

    let clip_open = |clip: &Clip, lane: Option<i32>, offset: i64, src_enable: Option<&str>| -> String {
        let id = asset_ids.get(clip.media_id.as_str()).cloned().unwrap_or_default();
        let mut text = format!("<asset-clip ref=\"{id}\"");
        if let Some(lane) = lane {
            let _ = write!(text, " lane=\"{lane}\"");
        }
        let _ = write!(text, " offset=\"{}\" name=\"{}\" start=\"{}\" duration=\"{}\"", t(offset), escape(&clip.name), t(clip.source_start), t(clip.duration));
        if let Some(enable) = src_enable {
            let _ = write!(text, " srcEnable=\"{enable}\"");
        }
        if !clip.enabled {
            text.push_str(" enabled=\"0\"");
        }
        text
    };
    let time_map = |clip: &Clip, indent: &str| -> String {
        if (clip.speed - 1.0).abs() < 1e-9 {
            return String::new();
        }
        format!(
            "{indent}<timeMap>\n{indent}    <timept time=\"0s\" value=\"{}\" interp=\"linear\"/>\n{indent}    <timept time=\"{}\" value=\"{}\" interp=\"linear\"/>\n{indent}</timeMap>\n",
            t(clip.source_start),
            t(clip.duration),
            t(clip.source_start + clip.source_duration()),
        )
    };
    let media_of = |clip: &Clip| timeline.media(&clip.media_id);
    let indent = "                        ";
    for (i, (from, _to, item)) in spine.iter().enumerate() {
        // Where this item's own time starts, and how fast it runs.
        let (local_start, speed) = match item {
            Item::Clip(clip) => (clip.source_start, clip.speed),
            Item::Gap => (0, 1.0),
        };
        let mut inner = String::new();
        if let Item::Clip(clip) = item {
            inner.push_str(&time_map(clip, &format!("{indent}    ")));
        }
        for c in &children[i] {
            let offset = local_start + ((c.clip.start - from) as f64 * speed).round() as i64;
            let has_audio = media_of(c.clip).is_some_and(|m| m.has_audio);
            let has_video = media_of(c.clip).is_some_and(|m| m.has_video);
            let enable = if c.audio_only {
                has_video.then_some("audio")
            } else if has_audio && !c.clip.sound {
                Some("video")
            } else {
                None
            };
            let open = clip_open(c.clip, Some(c.lane), offset, enable);
            let map = time_map(c.clip, &format!("{indent}        "));
            if map.is_empty() {
                let _ = writeln!(inner, "{indent}    {open}/>");
            } else {
                let _ = writeln!(inner, "{indent}    {open}>\n{map}{indent}    </asset-clip>");
            }
        }
        let open = match item {
            Item::Clip(clip) => {
                let has_audio = media_of(clip).is_some_and(|m| m.has_audio);
                clip_open(clip, None, *from, (has_audio && !clip.sound).then_some("video"))
            }
            Item::Gap => format!("<gap name=\"Gap\" offset=\"{}\" start=\"0s\" duration=\"{}\"", t(*from), t(spine[i].1 - from)),
        };
        let tag = match item {
            Item::Clip(_) => "asset-clip",
            Item::Gap => "gap",
        };
        if inner.is_empty() {
            let _ = writeln!(out, "{indent}{open}/>");
        } else {
            let _ = writeln!(out, "{indent}{open}>\n{inner}{indent}</{tag}>");
        }
        if let Item::Clip(clip) = item
            && let Some(transition) = &clip.transition
            && main.iter().any(|next| next.start == clip.end())
        {
            let name = if transition.kind == "wipe" { "Wipe" } else { "Cross Dissolve" };
            let _ = writeln!(
                out,
                "{indent}<transition name=\"{name}\" offset=\"{}\" duration=\"{}\"/>",
                t(clip.end() - transition.before()),
                t(transition.frames)
            );
        }
    }
    out.push_str("                    </spine>\n                </sequence>\n            </project>\n        </event>\n    </library>\n</fcpxml>\n");
    out
}

struct Asset {
    media_id: String,
    start: f64,
    has_video: bool,
    has_audio: bool,
}

struct Reader {
    rate: Rate,
    assets: HashMap<String, Asset>,
    timeline: Timeline,
    /// Clips by track key: `V1`, `V2`, … and `A1`, `A2`, ….
    tracks: Vec<(String, TrackKind, Vec<Clip>)>,
    /// The track the spine's last clip went on (a spine transition follows it).
    last_main: Option<String>,
}

/// Where an element's own time lands on the timeline.
#[derive(Clone, Copy)]
struct Frame {
    /// Timeline seconds of the element's own time `start`.
    record: f64,
    start: f64,
    speed: f64,
}

impl Frame {
    fn at(&self, local: f64) -> f64 {
        self.record + (local - self.start) / self.speed
    }
}

fn attr_time(node: roxmltree::Node, name: &str) -> Option<f64> {
    node.attribute(name).and_then(parse_time)
}

fn speed_of(node: roxmltree::Node) -> f64 {
    let Some(map) = node.children().find(|c| c.has_tag_name("timeMap")) else { return 1.0 };
    let points: Vec<(f64, f64)> = map
        .children()
        .filter(|c| c.has_tag_name("timept"))
        .filter_map(|p| Some((attr_time(p, "time")?, attr_time(p, "value")?)))
        .collect();
    match (points.first(), points.last()) {
        (Some(a), Some(b)) if b.0 > a.0 && (b.1 - a.1).abs() > 0.0 => (b.1 - a.1) / (b.0 - a.0),
        _ => 1.0,
    }
}

impl Reader {
    fn frames(&self, seconds: f64) -> i64 {
        self.rate.frames(seconds)
    }

    fn push(&mut self, key: String, kind: TrackKind, clip: Clip) {
        match self.tracks.iter_mut().find(|(k, _, _)| *k == key) {
            Some((_, _, clips)) => clips.push(clip),
            None => self.tracks.push((key, kind, vec![clip])),
        }
    }

    /// The asset an `asset-clip` or `clip` (through its `video` / `audio`) uses.
    fn asset_ref<'a>(&self, node: roxmltree::Node<'a, 'a>) -> Option<(String, Option<roxmltree::Node<'a, 'a>>)> {
        if node.has_tag_name("asset-clip") {
            return node.attribute("ref").map(|r| (r.to_owned(), None));
        }
        let inner = node.children().find(|c| c.has_tag_name("video")).or_else(|| node.children().find(|c| c.has_tag_name("audio")))?;
        inner.attribute("ref").map(|r| (r.to_owned(), Some(inner)))
    }

    /// One story element at `lane` placed by `parent`; returns its end on the timeline.
    fn element(&mut self, node: roxmltree::Node, parent: Frame, lane: i32) -> f64 {
        let offset = attr_time(node, "offset").unwrap_or(parent.start);
        let record = parent.at(offset);
        let duration = attr_time(node, "duration").unwrap_or(0.0);
        let start = attr_time(node, "start").unwrap_or(0.0);
        let tag = node.tag_name().name();
        let mut own = Frame { record, start, speed: 1.0 };
        match tag {
            "asset-clip" | "clip" => {
                let speed = speed_of(node);
                own.speed = speed;
                match self.asset_ref(node) {
                    Some((reference, inner)) if self.assets.contains_key(&reference) => {
                        let asset = &self.assets[&reference];
                        // A `clip`'s media starts where its `video` element says.
                        let inner_start = inner.and_then(|i| attr_time(i, "start")).unwrap_or(0.0);
                        let inner_offset = inner.and_then(|i| attr_time(i, "offset")).unwrap_or(0.0);
                        let source = start - inner_offset + inner_start - asset.start;
                        let enable = node.attribute("srcEnable").unwrap_or("all");
                        let shows_video = asset.has_video && enable != "audio" && lane >= 0;
                        let sounds = asset.has_audio && enable != "video";
                        let clip = Clip {
                            name: node.attribute("name").unwrap_or("").to_owned(),
                            media_id: asset.media_id.clone(),
                            start: self.frames(record),
                            duration: self.frames(duration),
                            source_start: self.frames(source),
                            speed,
                            sound: sounds,
                            enabled: node.attribute("enabled") != Some("0"),
                            transition: None,
                        };
                        let key = if shows_video {
                            Some((format!("V{}", lane + 1), TrackKind::Video))
                        } else if sounds {
                            Some((format!("A{}", if lane < 0 { -lane } else { lane + 1 }), TrackKind::Audio))
                        } else {
                            None
                        };
                        if let Some((key, kind)) = key {
                            if lane == 0 && parent.record == 0.0 && parent.start == 0.0 && parent.speed == 1.0 {
                                self.last_main = Some(key.clone());
                            }
                            self.push(key, kind, clip);
                        }
                    }
                    _ => self.timeline.note("Compound, multicam and synchronised clips were left out."),
                }
            }
            "gap" => {}
            "spine" => {
                // A secondary storyline: its clips one after another from its offset.
                let mut cursor = record;
                for child in node.children().filter(|c| c.is_element()) {
                    if child.has_tag_name("transition") {
                        self.transition(child, &format!("V{}", lane + 1), cursor);
                        continue;
                    }
                    let length = attr_time(child, "duration").unwrap_or(0.0);
                    let at = Frame { record: cursor, start: attr_time(child, "offset").unwrap_or(0.0), speed: 1.0 };
                    self.element(child, at, lane);
                    cursor += length;
                }
                return cursor;
            }
            "title" | "video" | "audio" | "ref-clip" | "mc-clip" | "sync-clip" | "audition" => {
                self.timeline.note(match tag {
                    "title" => "Titles were left out.",
                    "video" => "Generators and stills without an asset clip were left out.",
                    _ => "Compound, multicam and synchronised clips were left out.",
                });
            }
            _ => return record,
        }
        // Connected clips hang from this element, in its own time.
        for child in node.children().filter(|c| c.is_element() && c.attribute("lane").is_some()) {
            let child_lane: i32 = child.attribute("lane").and_then(|l| l.parse().ok()).unwrap_or(1);
            self.element(child, own, lane + child_lane);
        }
        if node.children().any(|c| c.has_tag_name("filter-video") || c.has_tag_name("filter-audio") || c.has_tag_name("adjust-transform")) {
            self.timeline.note("Effects and adjustments on clips were left out.");
        }
        record + duration
    }

    /// A transition at timeline second `cut_hint` (its centre) on `key`.
    fn transition(&mut self, node: roxmltree::Node, key: &str, _cursor: f64) {
        let Some(duration) = attr_time(node, "duration") else { return };
        let frames = self.frames(duration);
        let name = node.attribute("name").unwrap_or("").to_ascii_lowercase();
        let Some((_, _, clips)) = self.tracks.iter_mut().find(|(k, _, _)| k == key) else { return };
        if let Some(last) = clips.last_mut()
            && frames > 0
        {
            last.transition = Some(Transition { frames, kind: if name.contains("wipe") { "wipe" } else { "dissolve" }.to_owned() });
        }
    }
}

/// Reads FCPXML.
pub fn read(text: &str) -> Result<Timeline, String> {
    let options = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let document = roxmltree::Document::parse_with_options(text.trim_start_matches('\u{feff}'), options)
        .map_err(|error| format!("Not valid FCPXML: {error}"))?;
    let root = document.root_element();
    if !root.has_tag_name("fcpxml") {
        return Err("Expected an fcpxml document".to_owned());
    }
    let mut formats: HashMap<&str, (Rate, u32, u32)> = HashMap::new();
    for format in root.descendants().filter(|n| n.has_tag_name("format")) {
        let rate = attr_time(format, "frameDuration").filter(|d| *d > 0.0).map(|d| Rate::from_fps(1.0 / d)).unwrap_or(Rate::new(30, 1));
        let size = |name: &str| format.attribute(name).and_then(|v| v.parse().ok()).unwrap_or(0);
        if let Some(id) = format.attribute("id") {
            formats.insert(id, (rate, size("width"), size("height")));
        }
    }
    let sequence = root.descendants().find(|n| n.has_tag_name("sequence")).ok_or("The FCPXML holds no sequence")?;
    let (rate, width, height) = sequence.attribute("format").and_then(|f| formats.get(f).copied()).unwrap_or((Rate::new(30, 1), 0, 0));
    let name = sequence
        .parent_element()
        .filter(|p| p.has_tag_name("project"))
        .and_then(|p| p.attribute("name"))
        .unwrap_or("Timeline")
        .to_owned();
    let mut reader = Reader {
        rate,
        assets: HashMap::new(),
        timeline: Timeline { name, rate, width, height, media: Vec::new(), tracks: Vec::new(), notes: Vec::new() },
        tracks: Vec::new(),
        last_main: None,
    };
    for asset in root.descendants().filter(|n| n.has_tag_name("asset")) {
        let Some(id) = asset.attribute("id") else { continue };
        let src = asset
            .children()
            .find(|c| c.has_tag_name("media-rep"))
            .and_then(|m| m.attribute("src"))
            .or_else(|| asset.attribute("src"))
            .unwrap_or("");
        let file = if src.is_empty() { asset.attribute("name").unwrap_or(id).to_owned() } else { crate::file_name(src) };
        let duration = attr_time(asset, "duration");
        let has_video = asset.attribute("hasVideo") == Some("1");
        let has_audio = asset.attribute("hasAudio") == Some("1");
        let media_id = format!("m{}", reader.timeline.media.len() + 1);
        let size = asset.attribute("format").and_then(|f| formats.get(f)).map(|(_, w, h)| (*w, *h));
        reader.timeline.media.push(Media {
            id: media_id.clone(),
            name: file.clone(),
            url: if src.is_empty() { file } else { crate::url_to_path(src) },
            duration: duration.filter(|d| *d > 0.0).map(|d| rate.frames(d)),
            has_video,
            has_audio,
            still: has_video && duration == Some(0.0),
            width: size.map(|s| s.0).filter(|w| *w > 0),
            height: size.map(|s| s.1).filter(|h| *h > 0),
        });
        reader.assets.insert(id.to_owned(), Asset { media_id, start: attr_time(asset, "start").unwrap_or(0.0), has_video, has_audio });
    }
    if root.descendants().any(|n| n.has_tag_name("media")) {
        reader.timeline.note("Compound, multicam and synchronised clips were left out.");
    }
    let spine = sequence.children().find(|c| c.has_tag_name("spine")).ok_or("The sequence has no spine")?;
    let top = Frame { record: 0.0, start: 0.0, speed: 1.0 };
    for child in spine.children().filter(|c| c.is_element()) {
        if child.has_tag_name("transition") {
            // Into the next clip from the spine's last clip.
            if let Some(key) = reader.last_main.clone() {
                reader.transition(child, &key, 0.0);
            }
            continue;
        }
        reader.element(child, top, 0);
    }
    let mut tracks: Vec<(String, TrackKind, Vec<Clip>)> = reader.tracks;
    let number = |key: &str| key[1..].parse::<i32>().unwrap_or(0);
    tracks.sort_by_key(|(key, kind, _)| (*kind == TrackKind::Audio, number(key)));
    reader.timeline.tracks = tracks.into_iter().map(|(key, kind, clips)| crate::Track { kind, name: key, clips }).collect();
    Ok(reader.timeline)
}
