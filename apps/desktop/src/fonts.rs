//! Fonts from Fontsource for the desktop: a family's TTF files are fetched
//! from jsDelivr into `~/.opencut/fonts/<id>/` with the package's licence
//! file beside them, and the renderer's font database reloads so text
//! elements that name the family render the same as on the web.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

const API: &str = "https://api.fontsource.org/v1/fonts";
/// Weights fetched for a family, as the web app declares them.
const WEIGHTS: [u32; 2] = [400, 700];
/// Largest font file accepted, so a wrong URL cannot fill the disk.
const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;

/// The fonts directory next to the projects root.
pub(crate) fn fonts_dir(projects_root: &Path) -> PathBuf {
    projects_root
        .parent()
        .map(|parent| parent.join("fonts"))
        .unwrap_or_else(|| projects_root.join("fonts"))
}

/// Fontsource ids are the family lower-cased with runs of other characters
/// as dashes, the same slug the web app derives.
pub(crate) fn fontsource_id(family: &str) -> String {
    let mut id = String::new();
    let mut dash = false;
    for ch in family.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            id.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !id.is_empty() {
            id.push('-');
            dash = true;
        }
    }
    id.trim_end_matches('-').to_owned()
}

#[derive(Deserialize)]
struct FamilyDetail {
    family: String,
    #[serde(default)]
    license: String,
    #[serde(default = "latin")]
    #[serde(rename = "defSubset")]
    def_subset: String,
    #[serde(default)]
    weights: Vec<u32>,
    #[serde(default)]
    version: String,
    #[serde(default)]
    variants: serde_json::Value,
}

fn latin() -> String {
    "latin".to_owned()
}

/// One file to fetch: the weight and style it provides and its URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FontFile {
    pub weight: u32,
    pub style: String,
    pub url: String,
}

/// The TTF files for the wanted weights in both styles, from the default
/// subset; a missing weight falls back to the nearest the family has.
pub(crate) fn select_files(detail_json: &str) -> Result<(FamilyDetail, Vec<FontFile>), String> {
    let detail: FamilyDetail = serde_json::from_str(detail_json).map_err(|error| error.to_string())?;
    let available: Vec<u32> = if detail.weights.is_empty() {
        detail
            .variants
            .as_object()
            .map(|map| map.keys().filter_map(|key| key.parse().ok()).collect())
            .unwrap_or_default()
    } else {
        detail.weights.clone()
    };
    let mut chosen: Vec<u32> = Vec::new();
    for wanted in WEIGHTS {
        if let Some(nearest) = available.iter().copied().min_by_key(|weight| weight.abs_diff(wanted)) {
            if !chosen.contains(&nearest) {
                chosen.push(nearest);
            }
        }
    }
    let mut files = Vec::new();
    for weight in chosen {
        for style in ["normal", "italic"] {
            let url = detail
                .variants
                .get(weight.to_string())
                .and_then(|by_style| by_style.get(style))
                .and_then(|by_subset| by_subset.get(&detail.def_subset).or_else(|| by_subset.get("latin")))
                .and_then(|subset| subset.get("url"))
                .and_then(|urls| urls.get("ttf"))
                .and_then(|value| value.as_str());
            if let Some(url) = url {
                files.push(FontFile { weight, style: style.to_owned(), url: url.to_owned() });
            }
        }
    }
    if files.is_empty() {
        return Err(format!("{} has no TTF files to fetch", detail.family));
    }
    Ok((detail, files))
}

fn get(url: &str) -> Result<Vec<u8>, String> {
    let response = ureq::get(url).call().map_err(|error| format!("{url}: {error}"))?;
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_FILE_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{url}: {error}"))?;
    Ok(bytes)
}

/// What a fetch left on disk.
pub(crate) struct FetchedFamily {
    pub family: String,
    pub license: String,
    pub dir: PathBuf,
    pub files: usize,
}

/// Fetches `family` from Fontsource into `fonts_dir`: its TTF files, the
/// package's LICENSE and a provenance note. Files already there are kept.
pub(crate) fn fetch_family(fonts_dir: &Path, family: &str) -> Result<FetchedFamily, String> {
    let id = fontsource_id(family);
    if id.is_empty() {
        return Err("No font family named".to_owned());
    }
    let detail_json = get(&format!("{API}/{id}"))?;
    let detail_json = String::from_utf8(detail_json).map_err(|error| error.to_string())?;
    let (detail, files) = select_files(&detail_json)?;
    let dir = fonts_dir.join(&id);
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let mut written = 0;
    for file in &files {
        let target = dir.join(format!("{}-{}-{}.ttf", detail.def_subset, file.weight, file.style));
        if target.exists() {
            written += 1;
            continue;
        }
        let bytes = get(&file.url)?;
        fs::write(&target, bytes).map_err(|error| error.to_string())?;
        written += 1;
    }
    let license_path = dir.join("LICENSE");
    if !license_path.exists() {
        let version = if detail.version.is_empty() { "latest".to_owned() } else { detail.version.clone() };
        if let Ok(text) = get(&format!("https://cdn.jsdelivr.net/npm/@fontsource/{id}@{version}/LICENSE")) {
            fs::write(&license_path, text).ok();
        }
    }
    let provenance = serde_json::json!({
        "family": detail.family,
        "id": id,
        "license": detail.license,
        "version": detail.version,
        "source": format!("{API}/{id}"),
        "files": files.iter().map(|file| file.url.clone()).collect::<Vec<_>>(),
    });
    fs::write(dir.join("provenance.json"), serde_json::to_string_pretty(&provenance).unwrap_or_default())
        .map_err(|error| error.to_string())?;
    Ok(FetchedFamily { family: detail.family, license: detail.license, dir, files: written })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_match_fontsource_slugs() {
        assert_eq!(fontsource_id("Inter"), "inter");
        assert_eq!(fontsource_id("Noto Sans JP"), "noto-sans-jp");
        assert_eq!(fontsource_id("  Press Start 2P "), "press-start-2p");
    }

    #[test]
    fn picks_ttf_files_for_the_wanted_weights() {
        let json = r#"{
            "id": "inter", "family": "Inter", "license": "OFL-1.1", "defSubset": "latin",
            "weights": [400, 700, 900], "version": "5.3.0",
            "variants": {
                "400": {
                    "normal": {"latin": {"url": {"woff2": "a.woff2", "ttf": "https://cdn/inter@5.3.0/latin-400-normal.ttf"}}},
                    "italic": {"latin": {"url": {"ttf": "https://cdn/inter@5.3.0/latin-400-italic.ttf"}}}
                },
                "700": {"normal": {"latin": {"url": {"woff2": "only.woff2"}}}},
                "900": {"normal": {"latin": {"url": {"ttf": "https://cdn/inter@5.3.0/latin-900-normal.ttf"}}}}
            }
        }"#;
        let (detail, files) = select_files(json).unwrap();
        assert_eq!(detail.license, "OFL-1.1");
        assert_eq!(
            files,
            vec![
                FontFile { weight: 400, style: "normal".into(), url: "https://cdn/inter@5.3.0/latin-400-normal.ttf".into() },
                FontFile { weight: 400, style: "italic".into(), url: "https://cdn/inter@5.3.0/latin-400-italic.ttf".into() },
            ],
            "700 has no TTF, so only 400 comes through"
        );
    }

    #[test]
    fn the_fonts_dir_sits_beside_the_projects() {
        assert_eq!(fonts_dir(Path::new("/home/x/.opencut/projects")), PathBuf::from("/home/x/.opencut/fonts"));
    }
}
