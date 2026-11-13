//! Stock media: Openverse search results (CC-licensed images; read
//! directly, no key) and Pixabay results (through the API worker, which
//! returns this same shape), and the credits text a project's stock media
//! needs. The web app's `src/stock/` is the same, with the same test data.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Project, StockCredit};

/// The API worker the editors share (Pixabay search and file downloads).
pub const DEFAULT_API_URL: &str = "https://opencut-api.justinconza.workers.dev";

/// Openverse licences the editors search by default: free to use and
/// change commercially (CC0, Public Domain Mark, CC BY). Share-alike and
/// non-commercial licences are left out.
pub const OPENVERSE_LICENSES: &str = "cc0,pdm,by";

/// One search result (the worker's and the web app's `StockItem`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockItem {
    pub provider: String,
    pub id: String,
    pub kind: String,
    pub title: String,
    pub thumbnail: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    pub license: String,
    pub license_url: String,
    pub creator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_url: Option<String>,
    pub source_url: String,
    pub attribution: String,
    /// The file itself, when the provider gives it out (Openverse).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_url: Option<String>,
}

impl StockItem {
    pub fn credit(&self) -> StockCredit {
        StockCredit {
            provider: self.provider.clone(),
            id: self.id.clone(),
            title: self.title.clone(),
            creator: self.creator.clone(),
            creator_url: self.creator_url.clone(),
            license: self.license.clone(),
            license_url: self.license_url.clone(),
            source_url: self.source_url.clone(),
            attribution: self.attribution.clone(),
        }
    }
}

/// "CC BY 2.0", "CC0 1.0", "Public Domain Mark 1.0".
pub fn openverse_license_label(license: &str, version: &str) -> String {
    let name = match license.to_ascii_lowercase().as_str() {
        "cc0" => "CC0".to_owned(),
        "pdm" => "Public Domain Mark".to_owned(),
        other => format!("CC {}", other.to_ascii_uppercase()),
    };
    if version.is_empty() { name } else { format!("{name} {version}") }
}

/// The results of an Openverse `/v1/images/` search.
pub fn parse_openverse(json: &Value) -> Vec<StockItem> {
    let text = |item: &Value, key: &str| item.get(key).and_then(Value::as_str).unwrap_or("").to_owned();
    json.get("results")
        .and_then(Value::as_array)
        .map(|results| {
            results
                .iter()
                .filter_map(|item| {
                    let id = text(item, "id");
                    let url = text(item, "url");
                    if id.is_empty() || !url.starts_with("http") {
                        return None;
                    }
                    let creator = item.get("creator").and_then(Value::as_str).unwrap_or("Unknown").to_owned();
                    let title = match text(item, "title") {
                        t if t.is_empty() => "Untitled".to_owned(),
                        t => t,
                    };
                    Some(StockItem {
                        provider: "openverse".to_owned(),
                        kind: "image".to_owned(),
                        thumbnail: text(item, "thumbnail"),
                        width: item.get("width").and_then(Value::as_u64).unwrap_or(0) as u32,
                        height: item.get("height").and_then(Value::as_u64).unwrap_or(0) as u32,
                        duration: None,
                        license: openverse_license_label(&text(item, "license"), &text(item, "license_version")),
                        license_url: text(item, "license_url"),
                        creator_url: item.get("creator_url").and_then(Value::as_str).map(str::to_owned),
                        source_url: text(item, "foreign_landing_url"),
                        attribution: match text(item, "attribution") {
                            a if a.is_empty() => format!("\"{title}\" by {creator}"),
                            a => a,
                        },
                        file_url: Some(url),
                        id,
                        title,
                        creator,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The credits for the project's stock media, one line each in media order
/// (only media still in the project).
pub fn credits_text(project: &Project) -> String {
    let mut lines: Vec<String> = Vec::new();
    for media in &project.media {
        if let Some(credit) = project.credits.get(&media.id)
            && !lines.contains(&credit.attribution)
        {
            lines.push(credit.attribution.clone());
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shaped like a real Openverse response (fields as returned 2026-11-12);
    /// the web app's `src/stock/__tests__/stock.test.ts` uses the same data.
    pub(crate) fn openverse_fixture() -> Value {
        serde_json::json!({
            "result_count": 2,
            "results": [
                {
                    "id": "49dec960-1cc7-4738-afc6-793315cd51e9",
                    "title": "gloomy forest",
                    "url": "https://live.staticflickr.com/8043/8394379683_a49795d0e3_b.jpg",
                    "thumbnail": "https://api.openverse.org/v1/images/49dec960-1cc7-4738-afc6-793315cd51e9/thumb/",
                    "creator": "gorchakov.artem",
                    "creator_url": "https://www.flickr.com/photos/56003265@N05",
                    "license": "by",
                    "license_version": "2.0",
                    "license_url": "https://creativecommons.org/licenses/by/2.0/",
                    "foreign_landing_url": "https://www.flickr.com/photos/56003265@N05/8394379683",
                    "width": 1024,
                    "height": 640,
                    "attribution": "\"gloomy forest\" by gorchakov.artem is licensed under CC BY 2.0. To view a copy of this license, visit https://creativecommons.org/licenses/by/2.0/."
                },
                { "id": "", "url": "https://example.org/broken.jpg" },
                {
                    "id": "0f6d0a54-6b0a-4a41-9d6f-3a1fb2a5d1c0",
                    "title": "",
                    "url": "https://upload.wikimedia.org/x.jpg",
                    "thumbnail": "",
                    "creator": null,
                    "license": "cc0",
                    "license_version": "1.0",
                    "license_url": "https://creativecommons.org/publicdomain/zero/1.0/",
                    "foreign_landing_url": "https://commons.wikimedia.org/wiki/File:X.jpg",
                    "width": 800,
                    "height": 600
                }
            ]
        })
    }

    #[test]
    fn openverse_results_become_stock_items() {
        let items = parse_openverse(&openverse_fixture());
        assert_eq!(items.len(), 2, "the entry without an id is skipped");
        assert_eq!(items[0].license, "CC BY 2.0");
        assert_eq!(items[0].file_url.as_deref(), Some("https://live.staticflickr.com/8043/8394379683_a49795d0e3_b.jpg"));
        assert!(items[0].attribution.starts_with("\"gloomy forest\" by gorchakov.artem"));
        assert_eq!(items[1].title, "Untitled");
        assert_eq!(items[1].creator, "Unknown");
        assert_eq!(items[1].license, "CC0 1.0");
        assert_eq!(items[1].attribution, "\"Untitled\" by Unknown");
        assert_eq!(openverse_license_label("pdm", "1.0"), "Public Domain Mark 1.0");
    }

    #[test]
    fn credits_list_the_stock_media_in_the_project() {
        let items = parse_openverse(&openverse_fixture());
        let mut project = Project::new("Credits");
        for (index, item) in items.iter().enumerate() {
            project.media.push(crate::MediaItem {
                id: format!("m{index}"),
                name: item.title.clone(),
                kind: crate::MediaKind::Image,
                path: format!("{index}.jpg").into(),
                duration: None,
                width: None,
                height: None,
                fps: None,
                has_audio: false,
            });
            project.credits.insert(format!("m{index}"), item.credit());
        }
        project.credits.insert("gone".into(), items[0].credit());
        let text = credits_text(&project);
        assert_eq!(text.lines().count(), 2);
        assert!(text.lines().next().unwrap().contains("gorchakov.artem"));
    }
}
