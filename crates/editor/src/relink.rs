//! Media whose file has moved or gone: finding it, and looking for files of
//! the same name in a folder the user points at. The relink itself is
//! [`crate::Command::RelinkMedia`], so it can be undone.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::document::Project;

/// Ids of the project's media items whose file is not there.
pub fn missing_media(project: &Project) -> Vec<String> {
    project
        .media
        .iter()
        .filter(|media| !media.path.is_file())
        .map(|media| media.id.clone())
        .collect()
}

/// How deep [`find_relink_candidates`] looks below the folder it is given.
pub const RELINK_SEARCH_DEPTH: usize = 4;
/// Stops a search through a huge folder tree.
const RELINK_SEARCH_ENTRY_LIMIT: usize = 50_000;

/// For each missing media item, a file under `dir` with the same file name
/// (ignoring case), searching up to [`RELINK_SEARCH_DEPTH`] folders down.
/// When several match, the shallowest one wins.
pub fn find_relink_candidates(project: &Project, missing: &[String], dir: &Path) -> HashMap<String, PathBuf> {
    let wanted: HashMap<String, Vec<String>> = missing
        .iter()
        .filter_map(|id| project.media_item(id))
        .filter_map(|media| {
            let name = media.path.file_name()?.to_string_lossy().to_lowercase();
            Some((name, media.id.clone()))
        })
        .fold(HashMap::new(), |mut map, (name, id)| {
            map.entry(name).or_default().push(id);
            map
        });
    let mut found = HashMap::new();
    if wanted.is_empty() {
        return found;
    }
    let mut level = vec![dir.to_path_buf()];
    let mut seen = 0usize;
    for _ in 0..=RELINK_SEARCH_DEPTH {
        let mut next = Vec::new();
        for folder in &level {
            let Ok(entries) = std::fs::read_dir(folder) else { continue };
            for entry in entries.flatten() {
                seen += 1;
                if seen > RELINK_SEARCH_ENTRY_LIMIT {
                    return found;
                }
                let path = entry.path();
                let Ok(kind) = entry.file_type() else { continue };
                if kind.is_dir() {
                    next.push(path);
                } else if kind.is_file() {
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if let Some(ids) = wanted.get(&name) {
                        for id in ids {
                            found.entry(id.clone()).or_insert_with(|| path.clone());
                        }
                    }
                }
            }
        }
        if found.len() == missing.len() || next.is_empty() {
            break;
        }
        next.sort();
        level = next;
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, MediaItem, MediaKind, apply};

    fn media(id: &str, path: PathBuf) -> MediaItem {
        MediaItem {
            id: id.to_owned(),
            name: path.file_name().unwrap().to_string_lossy().into_owned(),
            kind: MediaKind::Video,
            path,
            duration: None,
            width: Some(1920),
            height: Some(1080),
            fps: Some(30.0),
            has_audio: true,
        }
    }

    #[test]
    fn finds_missing_files_and_relinks_them() {
        let dir = tempfile::tempdir().unwrap();
        let here = dir.path().join("here.mp4");
        std::fs::write(&here, b"x").unwrap();
        let gone = dir.path().join("old").join("Clip A.MP4");
        let mut project = Project::new("Relink");
        apply(&mut project, &Command::AddMedia(media("here", here))).unwrap();
        apply(&mut project, &Command::AddMedia(media("gone", gone.clone()))).unwrap();
        apply(&mut project, &Command::AddMedia(media("nowhere", dir.path().join("nowhere.mp4")))).unwrap();

        let missing = missing_media(&project);
        assert_eq!(missing, vec!["gone".to_owned(), "nowhere".to_owned()]);

        // The moved file sits two folders down with different case.
        let moved = dir.path().join("new").join("deeper").join("clip a.mp4");
        std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
        std::fs::write(&moved, b"y").unwrap();
        let found = find_relink_candidates(&project, &missing, &dir.path().join("new"));
        assert_eq!(found.get("gone"), Some(&moved));
        assert!(!found.contains_key("nowhere"));

        let relinked = MediaItem { width: Some(1280), height: Some(720), ..media("gone", moved.clone()) };
        apply(&mut project, &Command::RelinkMedia(relinked)).unwrap();
        let item = project.media_item("gone").unwrap();
        assert_eq!(item.path, moved);
        assert_eq!(item.width, Some(1280));
        assert_eq!(missing_media(&project), vec!["nowhere".to_owned()]);

        let image = MediaItem { kind: MediaKind::Image, ..media("nowhere", dir.path().join("x.png")) };
        assert!(apply(&mut project, &Command::RelinkMedia(image)).is_err(), "a video stays a video");
        assert!(apply(&mut project, &Command::RelinkMedia(media("unknown", dir.path().join("y.mp4")))).is_err());
    }
}
