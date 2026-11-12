//! The part of the processed-files folder that can be made again: video
//! proxies (`proxy-*.mp4`) and retimed audio (`retime-*.wav`). Cleaned-up
//! audio (noise reduction) also lives there but projects point at it as
//! media, so it is never counted or removed here.

use std::path::Path;

/// Whether a file in the cache folder is one the editor can make again.
pub fn is_regenerable(file_name: &str) -> bool {
    let name = file_name.to_ascii_lowercase();
    (name.starts_with("proxy-") && name.ends_with(".mp4")) || (name.starts_with("retime-") && name.ends_with(".wav"))
}

/// How many regenerable files `dir` holds and their total size in bytes.
pub fn regenerable_cache(dir: &Path) -> (usize, u64) {
    let Ok(entries) = std::fs::read_dir(dir) else { return (0, 0) };
    entries
        .flatten()
        .filter(|entry| is_regenerable(&entry.file_name().to_string_lossy()))
        .filter_map(|entry| entry.metadata().ok().filter(|meta| meta.is_file()))
        .fold((0, 0), |(count, bytes), meta| (count + 1, bytes + meta.len()))
}

/// Deletes the regenerable files in `dir`; returns how many went and how
/// many bytes they held. Files in use stay and are skipped.
pub fn clear_regenerable_cache(dir: &Path) -> (usize, u64) {
    let Ok(entries) = std::fs::read_dir(dir) else { return (0, 0) };
    let mut cleared = (0, 0);
    for entry in entries.flatten() {
        if !is_regenerable(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_file() && std::fs::remove_file(entry.path()).is_ok() {
            cleared.0 += 1;
            cleared.1 += meta.len();
        }
    }
    cleared
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clears_only_what_can_be_made_again() {
        let dir = tempfile::tempdir().unwrap();
        for (name, bytes) in [("proxy-ab.mp4", 10), ("retime-cd.wav", 20), ("denoise-ef.wav", 40), ("notes.txt", 5)] {
            std::fs::write(dir.path().join(name), vec![0u8; bytes]).unwrap();
        }
        assert_eq!(regenerable_cache(dir.path()), (2, 30));
        assert_eq!(clear_regenerable_cache(dir.path()), (2, 30));
        assert_eq!(regenerable_cache(dir.path()), (0, 0));
        assert!(dir.path().join("denoise-ef.wav").is_file(), "media a project uses stays");
        assert_eq!(regenerable_cache(&dir.path().join("missing")), (0, 0));
    }
}
