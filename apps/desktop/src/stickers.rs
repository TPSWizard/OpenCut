//! Sticker files for the desktop renderer. The web app draws stickers from
//! URLs (icon sets through the Iconify API, flags from its own files); the
//! desktop fetches each sticker's SVG once into `~/.opencut/stickers/` and
//! the renderer rasterises it from there, so a project bundle's stickers
//! show here too. Shapes and graphics are drawn by the web app's own code
//! and stay web-only.

use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use render::sticker_file_name;

/// Where the web app serves the flag files from.
const WEB_ASSETS_ORIGIN: &str = "https://conzacut.conzahub.com";
const ICONIFY_API: &str = "https://api.iconify.design";
/// Icon sets that keep their own colours; every other set is monochrome
/// and rendered white, as the web app does.
const PALETTE_PREFIXES: &[&str] = &[
    "fluent-emoji",
    "fluent-emoji-flat",
    "twemoji",
    "noto",
    "emojione",
    "openmoji",
    "fxemoji",
    "streamline-emojis",
    "logos",
    "skill-icons",
    "devicon",
    "vscode-icons",
    "flat-color-icons",
    "circle-flags",
    "flagpack",
    "flag",
];
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// The stickers directory next to the projects root.
pub(crate) fn stickers_dir(projects_root: &Path) -> PathBuf {
    projects_root
        .parent()
        .map(|parent| parent.join("stickers"))
        .unwrap_or_else(|| projects_root.join("stickers"))
}

/// The URL a sticker id's SVG can be fetched from, or none for ids the
/// desktop cannot draw (shapes, graphics, unknown providers).
pub(crate) fn sticker_url(sticker_id: &str) -> Option<String> {
    let (provider, value) = sticker_id.split_once(':')?;
    match provider {
        "icons" => {
            let (prefix, name) = value.split_once(':')?;
            if prefix.is_empty() || name.is_empty() {
                return None;
            }
            let colour = if PALETTE_PREFIXES.contains(&prefix) { "" } else { "?color=%23ffffff" };
            Some(format!("{ICONIFY_API}/{prefix}/{name}.svg{colour}"))
        }
        "flags" => {
            let code = value.to_ascii_lowercase();
            (!code.is_empty() && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
                .then(|| format!("{WEB_ASSETS_ORIGIN}/flags/{code}.svg"))
        }
        _ => None,
    }
}

/// Fetches the sticker's SVG into the directory unless it is there already.
pub(crate) fn fetch_sticker(dir: &Path, sticker_id: &str) -> Result<PathBuf, String> {
    let url = sticker_url(sticker_id).ok_or_else(|| format!("{sticker_id}: the desktop cannot draw this sticker"))?;
    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let target = dir.join(sticker_file_name(sticker_id));
    if target.exists() {
        return Ok(target);
    }
    let response = ureq::get(&url).call().map_err(|error| format!("{url}: {error}"))?;
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_FILE_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{url}: {error}"))?;
    if !bytes.windows(4).any(|window| window == b"<svg") {
        return Err(format!("{url}: not an SVG"));
    }
    fs::write(&target, bytes).map_err(|error| error.to_string())?;
    Ok(target)
}

/// The sticker ids of a project that are not on disk yet, skipping ones
/// the desktop cannot draw and ones already tried.
pub(crate) fn missing_stickers(project: &editor::Project, dir: &Path, tried: &HashSet<String>) -> Vec<String> {
    let mut missing = Vec::new();
    // Stickers inside compound clips are drawn too.
    let expanded = editor::expand_compounds(project);
    for (_, element) in expanded.elements() {
        if let editor::ElementKind::Sticker { sticker_id, .. } = &element.kind {
            if tried.contains(sticker_id) || missing.contains(sticker_id) {
                continue;
            }
            if sticker_url(sticker_id).is_none() || dir.join(sticker_file_name(sticker_id)).exists() {
                continue;
            }
            missing.push(sticker_id.clone());
        }
    }
    missing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_follow_the_web_apps_providers() {
        assert_eq!(
            sticker_url("icons:mdi:home").as_deref(),
            Some("https://api.iconify.design/mdi/home.svg?color=%23ffffff")
        );
        assert_eq!(sticker_url("icons:twemoji:cat").as_deref(), Some("https://api.iconify.design/twemoji/cat.svg"));
        assert_eq!(sticker_url("flags:US").as_deref(), Some("https://conzacut.conzahub.com/flags/us.svg"));
        assert_eq!(sticker_url("shapes:star"), None);
        assert_eq!(sticker_url("icons:mdi"), None);
        assert_eq!(sticker_url("flags:../x"), None);
    }

    #[test]
    fn the_stickers_dir_sits_beside_the_projects() {
        assert_eq!(stickers_dir(Path::new("/home/x/.opencut/projects")), PathBuf::from("/home/x/.opencut/stickers"));
    }
}
