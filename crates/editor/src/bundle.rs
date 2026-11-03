//! Project bundles: a zip with the web app's project JSON and the media
//! files it uses, the one format both editors read and write.
//!
//! ```text
//! project.json      {"format": "opencut-project-bundle", "formatVersion": 1,
//!                    "project": <the web app's serialized project>,
//!                    "media": [<web media metadata> + "file": "media/<id>.<ext>"]}
//! media/<id>.<ext>  the files
//! ```
//!
//! The web app keeps several scenes per project, each with overlay, main
//! and audio tracks; this editor keeps one flat track list, so importing
//! takes the current scene and exporting writes a single scene. Elements
//! the native editor does not have (stickers, graphics, effect tracks,
//! library sounds) are left out of an import and reported.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::{FrameRate, MediaTime};

use crate::document::{
    Background, CanvasSize, Element, ElementKind, Lane, MediaItem, MediaKind, Project,
    ProjectSettings, Track, TrackKind,
};
use crate::store::ProjectStore;

pub const BUNDLE_FORMAT: &str = "opencut-project-bundle";
pub const BUNDLE_FORMAT_VERSION: u32 = 1;
/// The web app's `CURRENT_PROJECT_VERSION` (apps/web/src/services/storage/migrations).
pub const WEB_PROJECT_VERSION: u32 = 31;
const PROJECT_ENTRY: &str = "project.json";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Bundle {
    format: String,
    format_version: u32,
    project: WebProject,
    #[serde(default)]
    media: Vec<WebMedia>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebProject {
    metadata: WebMetadata,
    #[serde(default)]
    scenes: Vec<WebScene>,
    #[serde(default)]
    current_scene_id: String,
    settings: WebSettings,
    version: u32,
    #[serde(flatten)]
    rest: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebMetadata {
    id: String,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    thumbnail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    duration: Option<MediaTime>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebScene {
    id: String,
    name: String,
    #[serde(default)]
    is_main: bool,
    tracks: WebTracks,
    #[serde(default)]
    bookmarks: Vec<Value>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebTracks {
    #[serde(default)]
    overlay: Vec<Value>,
    main: Value,
    #[serde(default)]
    audio: Vec<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebSettings {
    fps: FrameRate,
    canvas_size: CanvasSize,
    #[serde(default)]
    background: Background,
    #[serde(flatten)]
    rest: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebMedia {
    id: String,
    name: String,
    #[serde(rename = "type")]
    kind: MediaKind,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    last_modified: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    height: Option<u32>,
    /// Seconds, as the web app stores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    duration: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fps: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    has_audio: Option<bool>,
    /// Entry name inside the bundle.
    file: String,
}

/// What an import produced, and what it had to leave out.
#[derive(Debug)]
pub struct ImportReport {
    pub project: Project,
    /// One line per element or track that has no native equivalent.
    pub skipped: Vec<String>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Reads a bundle, copies its media next to the project under the store's
/// root and saves the project there. A project id already in the store gets
/// a fresh one, so importing never overwrites.
pub fn import_bundle(path: &Path, store: &ProjectStore) -> io::Result<ImportReport> {
    let file = File::open(path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| invalid(error.to_string()))?;
    let bundle: Bundle = {
        let mut entry = archive
            .by_name(PROJECT_ENTRY)
            .map_err(|_| invalid(format!("not an OpenCut project bundle (no {PROJECT_ENTRY})")))?;
        let mut json = String::new();
        entry.read_to_string(&mut json)?;
        serde_json::from_str(&json).map_err(|error| invalid(format!("{PROJECT_ENTRY}: {error}")))?
    };
    if bundle.format != BUNDLE_FORMAT {
        return Err(invalid(format!("unknown bundle format {:?}", bundle.format)));
    }
    if bundle.format_version > BUNDLE_FORMAT_VERSION {
        return Err(invalid(format!(
            "bundle format version {} is newer than this editor understands ({BUNDLE_FORMAT_VERSION})",
            bundle.format_version
        )));
    }

    let web = bundle.project;
    let mut id = web.metadata.id.clone();
    if id.is_empty() || store.load(&id).is_ok() {
        id = crate::new_id();
    }
    let media_dir = store.project_dir(&id).join("media");
    fs::create_dir_all(&media_dir)?;

    let mut media = Vec::with_capacity(bundle.media.len());
    for item in &bundle.media {
        let file_name = Path::new(&item.file)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| invalid(format!("media {} has no file name", item.id)))?;
        let target = media_dir.join(&file_name);
        {
            let mut entry = archive
                .by_name(&item.file)
                .map_err(|_| invalid(format!("bundle is missing {}", item.file)))?;
            let mut out = File::create(&target)?;
            io::copy(&mut entry, &mut out)?;
        }
        media.push(MediaItem {
            id: item.id.clone(),
            name: item.name.clone(),
            kind: item.kind,
            path: target,
            duration: item.duration.and_then(MediaTime::from_seconds_f64),
            width: item.width,
            height: item.height,
            fps: item.fps,
            has_audio: item.has_audio.unwrap_or(item.kind != MediaKind::Image),
        });
    }

    let mut skipped = Vec::new();
    let scene = web
        .scenes
        .iter()
        .find(|scene| scene.id == web.current_scene_id)
        .or_else(|| web.scenes.iter().find(|scene| scene.is_main))
        .or_else(|| web.scenes.first());
    let mut tracks = Vec::new();
    if let Some(scene) = scene {
        if web.scenes.len() > 1 {
            skipped.push(format!(
                "{} other scene(s); only \"{}\" was imported",
                web.scenes.len() - 1,
                scene.name
            ));
        }
        for track in &scene.tracks.overlay {
            if let Some(track) = convert_track(track, Lane::Overlay, &mut skipped) {
                tracks.push(track);
            }
        }
        match convert_track(&scene.tracks.main, Lane::Main, &mut skipped) {
            Some(track) => tracks.push(track),
            None => tracks.push(Track::new(TrackKind::Video, Lane::Main, "Main")),
        }
        for track in &scene.tracks.audio {
            if let Some(track) = convert_track(track, Lane::Audio, &mut skipped) {
                tracks.push(track);
            }
        }
    } else {
        tracks.push(Track::new(TrackKind::Video, Lane::Main, "Main"));
    }
    // Elements whose media is not in the bundle cannot be shown.
    for track in &mut tracks {
        track.elements.retain(|element| match element.kind.media_id() {
            Some(media_id) if !media.iter().any(|item| item.id == media_id) => {
                skipped.push(format!("\"{}\": its media is not in the bundle", element.name));
                false
            }
            _ => true,
        });
    }

    let mut project = Project::new(web.metadata.name.clone());
    project.id = id;
    project.created_at = parse_iso_millis(&web.metadata.created_at).unwrap_or(project.created_at);
    project.updated_at = parse_iso_millis(&web.metadata.updated_at).unwrap_or(project.updated_at);
    project.settings = ProjectSettings {
        fps: web.settings.fps,
        canvas: web.settings.canvas_size,
        background: web.settings.background,
    };
    project.media = media;
    project.tracks = tracks;
    store.save(&project)?;
    Ok(ImportReport { project, skipped })
}

/// A web track as a native one, or `None` for a kind that has no native
/// equivalent (graphic and effect tracks).
fn convert_track(track: &Value, lane: Lane, skipped: &mut Vec<String>) -> Option<Track> {
    let kind = match track.get("type").and_then(Value::as_str) {
        Some("video") => TrackKind::Video,
        Some("text") => TrackKind::Text,
        Some("audio") => TrackKind::Audio,
        other => {
            let count = track
                .get("elements")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            skipped.push(format!(
                "{} track \"{}\" ({count} element(s))",
                other.unwrap_or("unknown"),
                track.get("name").and_then(Value::as_str).unwrap_or("")
            ));
            return None;
        }
    };
    let name = track
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(kind.label())
        .to_owned();
    let mut native = Track::new(kind, lane, name);
    if let Some(id) = track.get("id").and_then(Value::as_str)
        && !id.is_empty()
    {
        native.id = id.to_owned();
    }
    native.muted = track.get("muted").and_then(Value::as_bool).unwrap_or(false);
    native.hidden = track.get("hidden").and_then(Value::as_bool).unwrap_or(false);
    for element in track.get("elements").and_then(Value::as_array).into_iter().flatten() {
        let name = element.get("name").and_then(Value::as_str).unwrap_or("").to_owned();
        let kind = element.get("type").and_then(Value::as_str).unwrap_or("");
        let supported = match kind {
            "video" | "image" | "text" => true,
            "audio" => {
                element.get("sourceType").and_then(Value::as_str) != Some("library")
                    && element.get("mediaId").is_some()
            }
            _ => false,
        };
        if !supported {
            skipped.push(format!("{kind} element \"{name}\""));
            continue;
        }
        match serde_json::from_value::<Element>(element.clone()) {
            Ok(element) => native.elements.push(element),
            Err(error) => skipped.push(format!("{kind} element \"{name}\": {error}")),
        }
    }
    Some(native)
}

/// Writes the project and copies of its media files as a bundle.
pub fn export_bundle(project: &Project, path: &Path) -> io::Result<()> {
    let file = File::create(path)?;
    let mut writer = zip::ZipWriter::new(file);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let stored = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .large_file(true);

    let mut media = Vec::with_capacity(project.media.len());
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    for item in &project.media {
        let extension = item
            .path
            .extension()
            .map(|ext| format!(".{}", ext.to_string_lossy()))
            .unwrap_or_default();
        let entry = format!("media/{}{extension}", item.id);
        let metadata = fs::metadata(&item.path)?;
        let last_modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or(0);
        media.push(WebMedia {
            id: item.id.clone(),
            name: item.name.clone(),
            kind: item.kind,
            size: metadata.len(),
            last_modified,
            width: item.width,
            height: item.height,
            duration: item.duration.map(|duration| duration.to_seconds_f64()),
            fps: item.fps,
            has_audio: Some(item.has_audio),
            file: entry.clone(),
        });
        entries.push((entry, item.path.clone()));
    }

    let (overlay, main, audio) = web_tracks(project);
    let now = iso_from_millis(crate::document::now_millis());
    let scene = WebScene {
        id: crate::new_id(),
        name: "Main scene".to_owned(),
        is_main: true,
        tracks: WebTracks { overlay, main, audio },
        bookmarks: Vec::new(),
        created_at: iso_from_millis(project.created_at),
        updated_at: now.clone(),
    };
    let bundle = Bundle {
        format: BUNDLE_FORMAT.to_owned(),
        format_version: BUNDLE_FORMAT_VERSION,
        project: WebProject {
            metadata: WebMetadata {
                id: project.id.clone(),
                name: project.name.clone(),
                thumbnail: None,
                duration: Some(project.duration()),
                created_at: iso_from_millis(project.created_at),
                updated_at: now,
            },
            current_scene_id: scene.id.clone(),
            scenes: vec![scene],
            settings: WebSettings {
                fps: project.settings.fps,
                canvas_size: project.settings.canvas,
                background: project.settings.background.clone(),
                rest: BTreeMap::from([("canvasSizeMode".to_owned(), json!("preset"))]),
            },
            version: WEB_PROJECT_VERSION,
            rest: BTreeMap::new(),
        },
        media,
    };
    let json = serde_json::to_vec_pretty(&bundle).map_err(io::Error::other)?;
    writer.start_file(PROJECT_ENTRY, deflated)?;
    writer.write_all(&json)?;
    for (entry, source) in entries {
        writer.start_file(&entry, stored)?;
        let mut input = File::open(&source)?;
        io::copy(&mut input, &mut writer)?;
    }
    writer.finish()?;
    Ok(())
}

/// The project's tracks as the web app's overlay, main and audio tracks.
fn web_tracks(project: &Project) -> (Vec<Value>, Value, Vec<Value>) {
    let mut overlay = Vec::new();
    let mut audio = Vec::new();
    let mut main = None;
    for track in &project.tracks {
        let value = web_track(track);
        match track.lane {
            Lane::Overlay => overlay.push(value),
            Lane::Main if main.is_none() => main = Some(value),
            // A second main track cannot exist on the web; stack it as an overlay.
            Lane::Main => overlay.push(value),
            Lane::Audio => audio.push(value),
        }
    }
    let main = main.unwrap_or_else(|| web_track(&Track::new(TrackKind::Video, Lane::Main, "Main")));
    (overlay, main, audio)
}

fn web_track(track: &Track) -> Value {
    let elements: Vec<Value> = track
        .elements
        .iter()
        .map(|element| {
            let mut value = serde_json::to_value(element).unwrap_or(Value::Null);
            if let (ElementKind::Audio { .. }, Some(object)) = (&element.kind, value.as_object_mut()) {
                object.insert("sourceType".to_owned(), json!("upload"));
            }
            value
        })
        .collect();
    let mut value = json!({
        "id": track.id,
        "name": track.name,
        "type": track.kind.label(),
        "elements": elements,
    });
    let object = value.as_object_mut().expect("object");
    match track.kind {
        TrackKind::Video => {
            object.insert("muted".to_owned(), json!(track.muted));
            object.insert("hidden".to_owned(), json!(track.hidden));
        }
        TrackKind::Text => {
            object.insert("hidden".to_owned(), json!(track.hidden));
        }
        TrackKind::Audio => {
            object.insert("muted".to_owned(), json!(track.muted));
        }
    }
    value
}

/// `2024-05-06T07:08:09.010Z` → milliseconds since the epoch (UTC only,
/// which is what the web app writes).
fn parse_iso_millis(text: &str) -> Option<u64> {
    let text = text.trim().trim_end_matches('Z');
    let (date, time) = text.split_once('T')?;
    let mut date = date.split('-').map(|part| part.parse::<i64>().ok());
    let (year, month, day) = (date.next()??, date.next()??, date.next()??);
    let mut time = time.split(':');
    let hour: i64 = time.next()?.parse().ok()?;
    let minute: i64 = time.next()?.parse().ok()?;
    let seconds: f64 = time.next()?.parse().ok()?;
    let days = days_from_civil(year, month, day);
    let millis = ((days * 24 + hour) * 60 + minute) as f64 * 60_000.0 + seconds * 1000.0;
    (millis >= 0.0).then_some(millis.round() as u64)
}

fn iso_from_millis(millis: u64) -> String {
    let seconds = (millis / 1000) as i64;
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60,
        millis % 1000
    )
}

// Howard Hinnant's date algorithms.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, TrackPlacement, apply};

    fn seconds(value: f64) -> MediaTime {
        MediaTime::from_seconds_f64(value).unwrap()
    }

    #[test]
    fn dates_round_trip() {
        assert_eq!(iso_from_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso_from_millis(1_715_000_000_123), "2024-05-06T12:53:20.123Z");
        assert_eq!(parse_iso_millis("2024-05-06T12:53:20.123Z"), Some(1_715_000_000_123));
        assert_eq!(parse_iso_millis("nope"), None);
        for millis in [1u64, 951_782_400_000, 4_102_444_800_000] {
            assert_eq!(parse_iso_millis(&iso_from_millis(millis)), Some(millis));
        }
    }

    #[test]
    fn export_then_import_keeps_the_timeline_and_media() {
        let dir = tempfile::tempdir().unwrap();
        let image_path = dir.path().join("square.png");
        std::fs::write(&image_path, b"\x89PNG not really").unwrap();
        let mut project = Project::new("Bundle me");
        project.settings.canvas = CanvasSize { width: 640, height: 360 };
        project.settings.background = Background::Blur { blur_intensity: 200.0 };
        apply(
            &mut project,
            &Command::AddMedia(MediaItem {
                id: "img".to_owned(),
                name: "square.png".to_owned(),
                kind: MediaKind::Image,
                path: image_path,
                duration: None,
                width: Some(10),
                height: Some(10),
                fps: None,
                has_audio: false,
            }),
        )
        .unwrap();
        let mut image = Element::from_media(project.media_item("img").unwrap(), seconds(0.0));
        image.masks.push(crate::document::Mask::ellipse());
        apply(&mut project, &Command::InsertElement { element: image, placement: TrackPlacement::Auto }).unwrap();
        let mut text = Element::text("Hi", seconds(1.0));
        text.params.set("fontSize", 20.0);
        text.animations
            .entry("opacity".to_owned())
            .or_default()
            .upsert(seconds(0.0), 0.0, None);
        apply(&mut project, &Command::InsertElement { element: text, placement: TrackPlacement::Auto }).unwrap();

        let bundle_path = dir.path().join("out.opencut.zip");
        export_bundle(&project, &bundle_path).unwrap();

        // The bundle holds the web shape.
        let mut archive = zip::ZipArchive::new(File::open(&bundle_path).unwrap()).unwrap();
        let mut json = String::new();
        archive.by_name("project.json").unwrap().read_to_string(&mut json).unwrap();
        let value: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["format"], BUNDLE_FORMAT);
        assert_eq!(value["project"]["version"], WEB_PROJECT_VERSION);
        assert_eq!(value["project"]["settings"]["canvasSize"]["width"], 640);
        assert_eq!(value["project"]["scenes"][0]["tracks"]["main"]["type"], "video");
        assert_eq!(value["project"]["scenes"][0]["tracks"]["overlay"][0]["type"], "text");
        assert_eq!(value["media"][0]["file"], "media/img.png");
        assert!(archive.by_name("media/img.png").is_ok());

        let store = ProjectStore::new(dir.path().join("projects"));
        let report = import_bundle(&bundle_path, &store).unwrap();
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
        let imported = report.project;
        assert_eq!(imported.id, project.id, "a free id is kept");
        assert_eq!(imported.name, "Bundle me");
        assert_eq!(imported.settings, project.settings);
        assert_eq!(imported.tracks.len(), 2);
        assert_eq!(imported.tracks[1].lane, Lane::Main);
        assert_eq!(imported.tracks[1].elements[0].masks.len(), 1);
        assert_eq!(imported.tracks[0].elements[0].params.number("fontSize", 0.0), 20.0);
        assert_eq!(imported.tracks[0].elements[0].animations["opacity"].keys.len(), 1);
        assert_eq!(imported.media.len(), 1);
        assert!(imported.media[0].path.starts_with(store.project_dir(&imported.id)));
        assert_eq!(std::fs::read(&imported.media[0].path).unwrap(), b"\x89PNG not really");
        assert!(store.load(&imported.id).is_ok(), "saved into the store");

        // Importing again does not overwrite: the copy gets a new id.
        let again = import_bundle(&bundle_path, &store).unwrap().project;
        assert_ne!(again.id, imported.id);
    }

    #[test]
    fn web_only_content_is_skipped_and_reported() {
        let dir = tempfile::tempdir().unwrap();
        let bundle_path = dir.path().join("web.opencut.zip");
        let project = json!({
            "format": BUNDLE_FORMAT,
            "formatVersion": 1,
            "project": {
                "metadata": {"id": "w1", "name": "From the web", "duration": 600000,
                              "createdAt": "2024-05-06T12:53:20.123Z", "updatedAt": "2024-05-06T12:53:20.123Z"},
                "scenes": [{
                    "id": "s1", "name": "Main scene", "isMain": true, "bookmarks": [],
                    "createdAt": "2024-05-06T12:53:20.123Z", "updatedAt": "2024-05-06T12:53:20.123Z",
                    "tracks": {
                        "overlay": [
                            {"id": "g", "name": "Graphics", "type": "graphic", "hidden": false, "elements": [
                                {"id": "st", "name": "Sticker", "type": "sticker", "stickerId": "x", "startTime": 0, "duration": 120000, "trimStart": 0, "trimEnd": 0, "params": {}}
                            ]},
                            {"id": "t", "name": "Text", "type": "text", "hidden": false, "elements": [
                                {"id": "tx", "name": "Title", "type": "text", "startTime": 0, "duration": 600000, "trimStart": 0, "trimEnd": 0,
                                 "params": {"content": "Hello", "fontSize": 15}}
                            ]}
                        ],
                        "main": {"id": "m", "name": "Main", "type": "video", "muted": false, "hidden": false, "elements": [
                            {"id": "v", "name": "clip", "type": "video", "mediaId": "missing", "startTime": 0, "duration": 600000, "trimStart": 0, "trimEnd": 0, "params": {}, "retime": {"rate": 2}}
                        ]},
                        "audio": [
                            {"id": "a", "name": "Audio", "type": "audio", "muted": false, "elements": [
                                {"id": "lib", "name": "Library sound", "type": "audio", "sourceType": "library", "sourceUrl": "https://x", "startTime": 0, "duration": 600000, "trimStart": 0, "trimEnd": 0, "params": {}}
                            ]}
                        ]
                    }
                }],
                "currentSceneId": "s1",
                "settings": {"fps": {"numerator": 30, "denominator": 1}, "canvasSize": {"width": 1920, "height": 1080},
                             "canvasSizeMode": "preset", "background": {"type": "color", "color": "#000000"}},
                "version": 31
            },
            "media": []
        });
        let mut writer = zip::ZipWriter::new(File::create(&bundle_path).unwrap());
        writer.start_file("project.json", zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(project.to_string().as_bytes()).unwrap();
        writer.finish().unwrap();

        let store = ProjectStore::new(dir.path().join("projects"));
        let report = import_bundle(&bundle_path, &store).unwrap();
        assert_eq!(report.project.created_at, 1_715_000_000_123);
        assert_eq!(report.project.tracks.len(), 3, "text, main and audio tracks");
        assert_eq!(report.project.tracks[0].elements.len(), 1, "the title");
        assert!(report.project.tracks[1].elements.is_empty(), "the clip's media is missing");
        assert!(report.project.tracks[2].elements.is_empty(), "library sound skipped");
        let skipped = report.skipped.join("\n");
        assert!(skipped.contains("graphic track"), "{skipped}");
        assert!(skipped.contains("Library sound"), "{skipped}");
        assert!(skipped.contains("not in the bundle"), "{skipped}");

        // A file that is not a bundle is refused with a clear message.
        let other = dir.path().join("other.zip");
        let mut writer = zip::ZipWriter::new(File::create(&other).unwrap());
        writer.start_file("readme.txt", zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(b"hi").unwrap();
        writer.finish().unwrap();
        let error = import_bundle(&other, &store).unwrap_err();
        assert!(error.to_string().contains("not an OpenCut project bundle"), "{error}");
    }
}
