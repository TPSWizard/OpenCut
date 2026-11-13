//! "Open in ConzaCut" on the desktop: a link (a bundle URL, or the web
//! app's `/open?bundle=<url>` / `conzacut://open?bundle=<url>`, read by
//! `editor::bundle_url_from_link`) is downloaded into
//! `~/.opencut/downloads/` and imported. A path to a bundle on disk is
//! imported as it is. Links come from the clipboard (the projects page's
//! *Open link* button) or the command line (`opencut-desktop <link>`).

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Bundles larger than this are refused.
const MAX_BUNDLE_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// What to open: a bundle to download, or one already on disk.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Target {
    Url(String),
    File(PathBuf),
}

/// Reads a link or a path; `None` when it is neither.
pub(crate) fn target_from(text: &str) -> Option<Target> {
    let text = text.trim().trim_matches('"');
    if let Some(url) = editor::bundle_url_from_link(text) {
        return Some(Target::Url(url));
    }
    let path = PathBuf::from(text);
    (path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("zip")) && path.is_file()).then_some(Target::File(path))
}

/// The host a URL names, for messages.
pub(crate) fn host_of(url: &str) -> &str {
    url.split("://").nth(1).and_then(|rest| rest.split(['/', '?', '#']).next()).unwrap_or(url)
}

/// The downloads directory next to the projects root.
pub(crate) fn downloads_dir(projects_root: &Path) -> PathBuf {
    projects_root
        .parent()
        .map(|parent| parent.join("downloads"))
        .unwrap_or_else(|| projects_root.join("downloads"))
}

/// Downloads a bundle into `dir` (streamed to disk, at most 8 GB).
pub(crate) fn download(url: &str, dir: &Path) -> Result<PathBuf, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .build()
        .into();
    let response = agent
        .get(url)
        .header("User-Agent", "ConzaCut desktop")
        .call()
        .map_err(|error| format!("{}: {error}", host_of(url)))?;
    if let Some(length) = response.headers().get("content-length").and_then(|value| value.to_str().ok()?.parse::<u64>().ok())
        && length > MAX_BUNDLE_BYTES
    {
        return Err("The bundle is too large".to_owned());
    }
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let path = dir.join(format!("link-{}.opencut.zip", editor::new_id()));
    let mut reader = response.into_body().into_reader().take(MAX_BUNDLE_BYTES + 1);
    let mut file = File::create(&path).map_err(|error| error.to_string())?;
    let mut buffer = vec![0u8; 1 << 20];
    let mut written: u64 = 0;
    loop {
        let read = reader.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        written += read as u64;
        if written > MAX_BUNDLE_BYTES {
            drop(file);
            let _ = std::fs::remove_file(&path);
            return Err("The bundle is too large".to_owned());
        }
        file.write_all(&buffer[..read]).map_err(|error| error.to_string())?;
    }
    Ok(path)
}

/// Opens a target: downloads it when needed, then imports it into `store`.
pub(crate) fn open(target: &Target, store: &editor::ProjectStore) -> Result<editor::ImportReport, String> {
    match target {
        Target::File(path) => editor::import_bundle(path, store).map_err(|error| error.to_string()),
        Target::Url(url) => {
            let path = download(url, &downloads_dir(store.root()))?;
            let result = editor::import_bundle(&path, store).map_err(|error| error.to_string());
            // The project now holds its own copy of the media.
            let _ = std::fs::remove_file(&path);
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_links_and_paths() {
        assert_eq!(
            target_from("https://conzacut.conzahub.com/open?bundle=https%3A%2F%2Fexample.com%2Fp.zip"),
            Some(Target::Url("https://example.com/p.zip".into()))
        );
        assert_eq!(target_from("http://example.com/p.zip"), None);
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("x.opencut.zip");
        std::fs::write(&bundle, b"zip").unwrap();
        assert_eq!(target_from(&format!("\"{}\"", bundle.display())), Some(Target::File(bundle.clone())));
        assert_eq!(target_from(&dir.path().join("missing.zip").display().to_string()), None);
        assert_eq!(host_of("https://a.example:8443/x?y"), "a.example:8443");
    }

    /// `OPENCUT_LINK_CHECK=<open link>` downloads and imports a real link
    /// (one network request) into a temporary store.
    #[test]
    fn opens_a_real_link() {
        let Ok(link) = std::env::var("OPENCUT_LINK_CHECK") else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let store = editor::ProjectStore::new(dir.path().join("projects"));
        let target = target_from(&link).expect("a link the desktop opens");
        let report = open(&target, &store).unwrap();
        eprintln!("LINK {} -> {} ({} track(s))", link, report.project.name, report.project.tracks.len());
        assert!(std::fs::read_dir(downloads_dir(store.root())).unwrap().next().is_none(), "the download is removed");
    }

    #[test]
    fn imports_a_bundle_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let mut project = editor::Project::new("Linked");
        project.id = "linked-project".into();
        let bundle = dir.path().join("linked.opencut.zip");
        editor::export_bundle(&project, &bundle).unwrap();
        let store = editor::ProjectStore::new(dir.path().join("projects"));
        let report = open(&Target::File(bundle), &store).unwrap();
        assert_eq!(report.project.name, "Linked");
    }
}
