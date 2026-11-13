//! What the desktop knows about the licences of a project's assets, for the
//! bundle's asset manifest (`editor::asset_manifest`): Fontsource families
//! from the `provenance.json` written beside each fetched font, brand logos
//! from `brands.json`, and icon sets from the Iconify API (one request for
//! the sets the project uses; without a connection they stay unknown).

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use editor::{AssetLookups, BrandInfo, IconSetInfo, Project};

const ICONIFY_API: &str = "https://api.iconify.design";

/// Icon set prefixes of the project's icon stickers (emoji sets the
/// manifest already knows are left out).
fn icon_prefixes(project: &Project) -> BTreeSet<String> {
    let mut prefixes = BTreeSet::new();
    for track in &project.tracks {
        for element in &track.elements {
            if let editor::ElementKind::Sticker { sticker_id, .. } = &element.kind
                && let Some(rest) = sticker_id.strip_prefix("icons:")
                && let Some((prefix, _)) = rest.split_once(':')
                && !matches!(prefix, "twemoji" | "noto")
                && prefix.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                prefixes.insert(prefix.to_owned());
            }
        }
    }
    prefixes
}

/// Parses the Iconify `/collections` answer into icon set licences.
pub(crate) fn icon_sets_from_json(text: &str) -> Vec<(String, IconSetInfo)> {
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    map.into_iter()
        .map(|(prefix, entry)| {
            let text = |value: Option<&serde_json::Value>| value.and_then(serde_json::Value::as_str).map(str::to_owned);
            let info = IconSetInfo {
                name: text(entry.get("name")).unwrap_or_else(|| prefix.clone()),
                license: text(entry.pointer("/license/spdx")),
                license_url: text(entry.pointer("/license/url")),
                author: text(entry.pointer("/author/name")),
            };
            (prefix, info)
        })
        .collect()
}

fn fetch_icon_sets(prefixes: &BTreeSet<String>) -> Vec<(String, IconSetInfo)> {
    if prefixes.is_empty() {
        return Vec::new();
    }
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(8))).build().into();
    let url = format!("{ICONIFY_API}/collections?prefixes={}", prefixes.iter().cloned().collect::<Vec<_>>().join(","));
    let Ok(response) = agent.get(&url).header("User-Agent", "ConzaCut desktop").call() else {
        return Vec::new();
    };
    let mut text = String::new();
    if response.into_body().into_reader().take(4 * 1024 * 1024).read_to_string(&mut text).is_err() {
        return Vec::new();
    }
    icon_sets_from_json(&text)
}

/// Fontsource families fetched into `fonts_dir`, by family name.
fn fetched_fonts(fonts_dir: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(fonts_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let text = std::fs::read_to_string(entry.path().join("provenance.json")).ok()?;
            let value: serde_json::Value = serde_json::from_str(&text).ok()?;
            Some((value.get("family")?.as_str()?.to_owned(), value.get("license")?.as_str()?.to_owned()))
        })
        .collect()
}

/// Everything the desktop can find out; `online` allows the Iconify request.
pub(crate) fn lookups(project: &Project, projects_root: &Path, online: bool) -> AssetLookups {
    let mut lookups = AssetLookups::default();
    for brand in crate::stickers::brands() {
        lookups.brands.insert(
            brand.slug.clone(),
            BrandInfo { title: brand.title.clone(), license: brand.license.clone(), credit_required: brand.credit },
        );
    }
    lookups.fonts.extend(fetched_fonts(&crate::fonts::fonts_dir(projects_root)));
    if online {
        lookups.icon_sets.extend(fetch_icon_sets(&icon_prefixes(project)));
    }
    lookups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_iconify_collections() {
        let sets = icon_sets_from_json(
            r#"{"mdi":{"name":"Material Design Icons","author":{"name":"Pictogrammers"},"license":{"title":"Apache 2.0","spdx":"Apache-2.0","url":"https://x"}}}"#,
        );
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].0, "mdi");
        assert_eq!(sets[0].1.license.as_deref(), Some("Apache-2.0"));
        assert_eq!(sets[0].1.author.as_deref(), Some("Pictogrammers"));
    }

    #[test]
    fn brands_carry_their_licence() {
        let youtube = crate::stickers::brands().iter().find(|brand| brand.slug == "youtube").unwrap();
        assert!(!youtube.license.is_empty());
    }
}
