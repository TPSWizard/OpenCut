//! OpenTimelineIO JSON (`.otio`).
//!
//! Written: `Timeline.1` → `Stack.1` of `Track.1` (video tracks first,
//! bottom up, as "the layers in a stack are iterated from the bottom (the
//! first entry in the stack)"; then an audio track with the sound of each
//! video track's clips, then the audio tracks). Clips are `Clip.2` with an
//! `ExternalReference.1` under `DEFAULT_MEDIA`, a `source_range` in media
//! time at the timeline's rate (its duration the frames the clip fills on
//! the track) and a `LinearTimeWarp.1` with `time_scalar` for a speed
//! change. Spaces are `Gap.1`. A dissolve is a `Transition.1`
//! (`SMPTE_Dissolve`) between the clips with `in_offset` the frames before
//! the cut and `out_offset` the frames after; it takes no time on the track.
//!
//! Read: the same, plus `Clip.1` (one `media_reference`), `MissingReference`,
//! rates other than the timeline's (converted, rounded to frames), and
//! nested stacks or tracks (left out, their time kept, with a note).

use serde_json::{Map, Value, json};

use crate::{Clip, Media, Rate, Timeline, Track, TrackKind, Transition};

fn time(value: i64, rate: Rate) -> Value {
    json!({ "OTIO_SCHEMA": "RationalTime.1", "rate": rate.fps(), "value": value as f64 })
}

fn range(start: i64, duration: i64, rate: Rate) -> Value {
    json!({ "OTIO_SCHEMA": "TimeRange.1", "duration": time(duration, rate), "start_time": time(start, rate) })
}

fn clip_value(clip: &Clip, media: Option<&Media>, rate: Rate) -> Value {
    let reference = match media {
        Some(media) => json!({
            "OTIO_SCHEMA": "ExternalReference.1",
            "available_image_bounds": Value::Null,
            "available_range": match (media.duration, media.still) {
                (Some(duration), false) => range(0, duration, rate),
                _ => Value::Null,
            },
            "metadata": {},
            "name": media.name,
            "target_url": crate::path_to_url(&media.url),
        }),
        None => json!({ "OTIO_SCHEMA": "MissingReference.1", "available_image_bounds": Value::Null, "available_range": Value::Null, "metadata": {}, "name": "" }),
    };
    let effects: Vec<Value> = if (clip.speed - 1.0).abs() > 1e-9 {
        vec![json!({
            "OTIO_SCHEMA": "LinearTimeWarp.1",
            "effect_name": "LinearTimeWarp",
            "metadata": {},
            "name": "",
            "time_scalar": clip.speed,
        })]
    } else {
        Vec::new()
    };
    json!({
        "OTIO_SCHEMA": "Clip.2",
        "active_media_reference_key": "DEFAULT_MEDIA",
        "effects": effects,
        "enabled": clip.enabled,
        "markers": [],
        "media_references": { "DEFAULT_MEDIA": reference },
        "metadata": {},
        "name": clip.name,
        "source_range": range(clip.source_start, clip.duration, rate),
    })
}

fn track_value(name: &str, kind: TrackKind, clips: &[&Clip], timeline: &Timeline) -> Value {
    let rate = timeline.rate;
    let mut children = Vec::new();
    let mut cursor = 0;
    for (i, clip) in clips.iter().enumerate() {
        if clip.start > cursor {
            children.push(json!({
                "OTIO_SCHEMA": "Gap.1",
                "effects": [],
                "enabled": true,
                "markers": [],
                "metadata": {},
                "name": "",
                "source_range": range(0, clip.start - cursor, rate),
            }));
        }
        children.push(clip_value(clip, timeline.media(&clip.media_id), rate));
        cursor = clip.end().max(cursor);
        if let Some(transition) = &clip.transition
            && clips.get(i + 1).is_some_and(|next| next.start == clip.end())
        {
            children.push(json!({
                "OTIO_SCHEMA": "Transition.1",
                "enabled": true,
                "in_offset": time(transition.before(), rate),
                "metadata": {},
                "name": if transition.kind == "wipe" { "Wipe" } else { "Cross Dissolve" },
                "out_offset": time(transition.after(), rate),
                "transition_type": "SMPTE_Dissolve",
            }));
        }
    }
    json!({
        "OTIO_SCHEMA": "Track.1",
        "children": children,
        "effects": [],
        "enabled": true,
        "kind": match kind { TrackKind::Video => "Video", TrackKind::Audio => "Audio" },
        "markers": [],
        "metadata": {},
        "name": name,
        "source_range": Value::Null,
    })
}

/// The timeline as OpenTimelineIO JSON.
pub fn write(timeline: &Timeline) -> String {
    let mut tracks = Vec::new();
    for track in timeline.video_tracks() {
        tracks.push(track_value(&track.name, TrackKind::Video, &track.clips.iter().collect::<Vec<_>>(), timeline));
    }
    for track in timeline.video_tracks() {
        let sounding: Vec<&Clip> = track
            .clips
            .iter()
            .filter(|clip| clip.sound && timeline.media(&clip.media_id).is_some_and(|media| media.has_audio))
            .collect();
        if !sounding.is_empty() {
            tracks.push(track_value(&format!("{} sound", track.name), TrackKind::Audio, &sounding, timeline));
        }
    }
    for track in timeline.audio_tracks() {
        tracks.push(track_value(&track.name, TrackKind::Audio, &track.clips.iter().collect::<Vec<_>>(), timeline));
    }
    let value = json!({
        "OTIO_SCHEMA": "Timeline.1",
        "global_start_time": Value::Null,
        "metadata": { "conzacut": { "width": timeline.width, "height": timeline.height, "notes": timeline.notes } },
        "name": timeline.name,
        "tracks": {
            "OTIO_SCHEMA": "Stack.1",
            "children": tracks,
            "effects": [],
            "enabled": true,
            "markers": [],
            "metadata": {},
            "name": "tracks",
            "source_range": Value::Null,
        },
    });
    serde_json::to_string_pretty(&value).unwrap_or_default() + "\n"
}

fn schema(value: &Value) -> &str {
    value.get("OTIO_SCHEMA").and_then(Value::as_str).map(|s| s.split('.').next().unwrap_or(s)).unwrap_or("")
}

fn rational(value: &Value) -> Option<(f64, f64)> {
    let rate = value.get("rate")?.as_f64()?;
    let v = value.get("value")?.as_f64()?;
    (rate > 0.0 && rate.is_finite() && v.is_finite()).then_some((v, rate))
}

/// The timeline's rate: the first rational time's rate found in the tracks.
fn find_rate(value: &Value) -> Option<f64> {
    match value {
        Value::Object(map) => {
            if schema(value) == "RationalTime"
                && let Some((_, rate)) = rational(value)
            {
                return Some(rate);
            }
            ["tracks", "children", "source_range", "duration", "start_time"]
                .iter()
                .filter_map(|key| map.get(*key))
                .find_map(find_rate)
        }
        Value::Array(items) => items.iter().find_map(find_rate),
        _ => None,
    }
}

struct Reader {
    rate: Rate,
    timeline: Timeline,
}

impl Reader {
    fn frames(&self, value: Option<&Value>) -> i64 {
        value.and_then(rational).map_or(0, |(v, rate)| (v * self.rate.fps() / rate).round() as i64)
    }

    /// (start, duration) of an item's `source_range`.
    fn source_range(&self, item: &Value) -> Option<(i64, i64)> {
        let range = item.get("source_range").filter(|r| !r.is_null())?;
        Some((self.frames(range.get("start_time")), self.frames(range.get("duration"))))
    }

    /// The time an item takes on its track.
    fn duration(&self, item: &Value) -> i64 {
        if let Some((_, duration)) = self.source_range(item) {
            return duration;
        }
        let children = item.get("children").and_then(Value::as_array);
        match schema(item) {
            "Track" => children.map_or(0, |c| c.iter().filter(|c| schema(c) != "Transition").map(|c| self.duration(c)).sum()),
            "Stack" => children.map_or(0, |c| c.iter().map(|c| self.duration(c)).max().unwrap_or(0)),
            "Clip" => self.reference(item).and_then(|r| r.get("available_range")).and_then(|r| r.get("duration")).map_or(0, |d| self.frames(Some(d))),
            _ => 0,
        }
    }

    fn reference<'a>(&self, item: &'a Value) -> Option<&'a Value> {
        if let Some(references) = item.get("media_references").and_then(Value::as_object) {
            let key = item.get("active_media_reference_key").and_then(Value::as_str).unwrap_or("DEFAULT_MEDIA");
            return references.get(key).or_else(|| references.values().next());
        }
        item.get("media_reference").filter(|r| !r.is_null())
    }

    fn media_for(&mut self, item: &Value) -> String {
        let reference = self.reference(item);
        let url = reference.and_then(|r| r.get("target_url")).and_then(Value::as_str).unwrap_or("").to_owned();
        let reference_name = reference.and_then(|r| r.get("name")).and_then(Value::as_str).unwrap_or("");
        let clip_name = item.get("name").and_then(Value::as_str).unwrap_or("");
        let name = if !url.is_empty() {
            crate::file_name(&url)
        } else if !reference_name.is_empty() {
            reference_name.to_owned()
        } else {
            clip_name.to_owned()
        };
        let key = if url.is_empty() { name.clone() } else { crate::url_to_path(&url) };
        if let Some(media) = self.timeline.media.iter().find(|media| media.url == key || (url.is_empty() && media.name == name)) {
            return media.id.clone();
        }
        let available = reference.and_then(|r| r.get("available_range")).filter(|r| !r.is_null());
        let id = format!("m{}", self.timeline.media.len() + 1);
        self.timeline.media.push(Media {
            id: id.clone(),
            name,
            url: key,
            duration: available.map(|r| self.frames(r.get("duration"))),
            has_video: false,
            has_audio: false,
            still: false,
            width: None,
            height: None,
        });
        id
    }

    fn available_start(&self, item: &Value) -> i64 {
        self.reference(item)
            .and_then(|r| r.get("available_range"))
            .filter(|r| !r.is_null())
            .map_or(0, |r| self.frames(r.get("start_time")))
    }

    fn track(&mut self, track: &Value, kind: TrackKind) -> Track {
        let name = track.get("name").and_then(Value::as_str).unwrap_or("").to_owned();
        let mut out = Track { kind, name, clips: Vec::new() };
        let mut cursor = 0;
        for child in track.get("children").and_then(Value::as_array).into_iter().flatten() {
            match schema(child) {
                "Clip" => {
                    let (start, duration) = self.source_range(child).unwrap_or((0, self.duration(child)));
                    let media_id = self.media_for(child);
                    let mut speed = 1.0;
                    for effect in child.get("effects").and_then(Value::as_array).into_iter().flatten() {
                        match schema(effect) {
                            "LinearTimeWarp" => {
                                speed = effect.get("time_scalar").and_then(Value::as_f64).filter(|s| s.is_finite() && *s != 0.0).unwrap_or(1.0)
                            }
                            "FreezeFrame" => self.timeline.note("Freeze frames came in as ordinary clips."),
                            _ => self.timeline.note("Effects on clips were left out."),
                        }
                    }
                    let name = child.get("name").and_then(Value::as_str).unwrap_or("").to_owned();
                    let source_start = start - self.available_start(child);
                    if let Some(media) = self.timeline.media.iter_mut().find(|m| m.id == media_id) {
                        match kind {
                            TrackKind::Video => media.has_video = true,
                            TrackKind::Audio => media.has_audio = true,
                        }
                    }
                    out.clips.push(Clip {
                        name,
                        media_id,
                        start: cursor,
                        duration,
                        source_start,
                        speed,
                        sound: kind == TrackKind::Audio,
                        enabled: child.get("enabled").and_then(Value::as_bool).unwrap_or(true),
                        transition: None,
                    });
                    cursor += duration;
                }
                "Gap" => cursor += self.duration(child),
                "Transition" => {
                    let frames = self.frames(child.get("in_offset")) + self.frames(child.get("out_offset"));
                    let name = child.get("name").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
                    if let Some(last) = out.clips.last_mut()
                        && last.end() == cursor
                        && frames > 0
                    {
                        last.transition = Some(Transition { frames, kind: if name.contains("wipe") { "wipe" } else { "dissolve" }.to_owned() });
                    }
                }
                "Stack" | "Track" => {
                    self.timeline.note("Nested stacks and tracks (compound clips) were left out; their time is kept as a gap.");
                    cursor += self.duration(child);
                }
                _ => {}
            }
        }
        out
    }
}

/// Reads OpenTimelineIO JSON.
pub fn read(text: &str) -> Result<Timeline, String> {
    let root: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|error| format!("Not valid OpenTimelineIO JSON: {error}"))?;
    let timeline = match schema(&root) {
        "Timeline" => root,
        "SerializableCollection" => root
            .get("children")
            .and_then(Value::as_array)
            .and_then(|children| children.iter().find(|c| schema(c) == "Timeline"))
            .cloned()
            .ok_or("The collection holds no timeline")?,
        other => return Err(format!("Expected an OpenTimelineIO timeline, found {:?}", if other.is_empty() { "something else" } else { other })),
    };
    let rate = Rate::from_fps(find_rate(&timeline).unwrap_or(24.0));
    let name = timeline.get("name").and_then(Value::as_str).unwrap_or("Timeline").to_owned();
    let ours = timeline.get("metadata").and_then(|m| m.get("conzacut"));
    let dimension = |key: &str| ours.and_then(|o| o.get(key)).and_then(Value::as_u64).unwrap_or(0) as u32;
    let mut reader = Reader {
        rate,
        timeline: Timeline { name, rate, width: dimension("width"), height: dimension("height"), media: Vec::new(), tracks: Vec::new(), notes: Vec::new() },
    };
    let stack = timeline.get("tracks").cloned().unwrap_or(Value::Object(Map::new()));
    for track in stack.get("children").and_then(Value::as_array).cloned().unwrap_or_default() {
        if schema(&track) != "Track" {
            continue;
        }
        let kind = match track.get("kind").and_then(Value::as_str).unwrap_or("Video") {
            "Audio" => TrackKind::Audio,
            "Video" => TrackKind::Video,
            _ => {
                reader.timeline.note("Tracks other than video and audio were left out.");
                continue;
            }
        };
        let track = reader.track(&track, kind);
        reader.timeline.tracks.push(track);
    }
    let mut timeline = reader.timeline;
    crate::fold_sound(&mut timeline);
    Ok(timeline)
}
