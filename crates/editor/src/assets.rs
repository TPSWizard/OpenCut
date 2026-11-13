//! The asset manifest: `assets.json` in a project bundle, one entry per
//! thing the project uses that someone else may hold rights to (media files,
//! library sounds, stickers, fonts), each with its origin, licence, source
//! and credit line, plus the credits to give. Both editors build it from
//! the bundle's `project.json` the same way (the web app's
//! `apps/web/src/project/assets.ts`, checked against the same test data);
//! it is written on export and not read back (`project.json` stays the
//! source of truth).
//!
//! A licence of `null` means it is not known: the user's own files, and
//! sources whose licence is not recorded. Nothing is guessed.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const ASSET_MANIFEST_FORMAT: &str = "opencut-asset-manifest";
pub const ASSET_MANIFEST_FORMAT_VERSION: u32 = 1;
pub const ASSET_MANIFEST_ENTRY: &str = "assets.json";

/// Where generated media came from (the web app's `MediaOrigin`): `kind`
/// is "generated" (with the `tool` that made it, the media it was made
/// `from` and the `model` used) or "library" (with its `source` path).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaOrigin {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl MediaOrigin {
    pub fn generated(tool: &str, from: Option<&str>, model: Option<&str>) -> Self {
        Self {
            kind: "generated".to_owned(),
            tool: Some(tool.to_owned()),
            from: from.map(str::to_owned),
            model: model.map(str::to_owned),
            source: None,
        }
    }
}

/// An icon set's licence, as the Iconify API lists it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IconSetInfo {
    pub name: String,
    pub license: Option<String>,
    pub license_url: Option<String>,
    pub author: Option<String>,
}

/// A brand logo's entry in `brands.json`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BrandInfo {
    pub title: String,
    pub license: String,
    pub credit_required: bool,
}

/// What the editor knows about licences it looked up: icon sets by prefix,
/// brand logos by slug and Fontsource families by family name (SPDX id).
#[derive(Clone, Debug, Default)]
pub struct AssetLookups {
    pub icon_sets: BTreeMap<String, IconSetInfo>,
    pub brands: BTreeMap<String, BrandInfo>,
    pub fonts: BTreeMap<String, String>,
}

/// Fonts the editors treat as the system's own (`apps/web/src/fonts/system-fonts.ts`).
pub const SYSTEM_FONTS: &[&str] = &[
    "Arial",
    "Helvetica",
    "Times New Roman",
    "Courier New",
    "Verdana",
    "Georgia",
    "monospace",
    "sans-serif",
    "serif",
];

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetEntry {
    pub kind: String,
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub origin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    pub license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    pub credit_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub uses: u32,
}

/// Licences whose terms ask for a credit (CC BY in either spelling).
pub fn license_requires_credit(license: &str) -> bool {
    let upper = license.to_ascii_uppercase();
    upper.starts_with("CC BY") || upper.starts_with("CC-BY")
}

/// Fontsource's id for a family (lower case, other characters as dashes).
pub fn fontsource_slug(family: &str) -> String {
    let mut id = String::new();
    let mut dash = false;
    for ch in family.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            id.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash {
            id.push('-');
            dash = true;
        }
    }
    id.trim_matches('-').to_owned()
}

/// A Freesound preview URL's sound page (`/previews/<n>/<id>_...`).
fn freesound_page(url: &str) -> Option<String> {
    let rest = &url[url.find("/previews/")? + "/previews/".len()..];
    let (_, file) = rest.split_once('/')?;
    let id: String = file.chars().take_while(char::is_ascii_digit).collect();
    (!id.is_empty() && file[id.len()..].starts_with('_')).then(|| format!("https://freesound.org/s/{id}/"))
}

#[derive(Default)]
struct Uses {
    media: HashMap<String, u32>,
    sounds: BTreeMap<String, (String, u32)>,
    stickers: BTreeMap<String, (String, u32)>,
    fonts: BTreeMap<String, u32>,
}

fn walk(value: &Value, uses: &mut Uses) {
    match value {
        Value::Array(items) => items.iter().for_each(|item| walk(item, uses)),
        Value::Object(map) => {
            let text = |key: &str| map.get(key).and_then(Value::as_str);
            if let Some(id) = text("mediaId") {
                *uses.media.entry(id.to_owned()).or_default() += 1;
            }
            match text("type") {
                Some("audio") if text("sourceType") == Some("library") => {
                    if let Some(url) = text("sourceUrl") {
                        let entry = uses.sounds.entry(url.to_owned()).or_insert_with(|| (text("name").unwrap_or(url).to_owned(), 0));
                        entry.1 += 1;
                    }
                }
                Some("sticker") => {
                    if let Some(id) = text("stickerId") {
                        let entry = uses.stickers.entry(id.to_owned()).or_insert_with(|| (text("name").unwrap_or(id).to_owned(), 0));
                        entry.1 += 1;
                    }
                }
                Some("text") => {
                    if let Some(family) = map.get("params").and_then(|params| params.get("fontFamily")).and_then(Value::as_str)
                        && !family.trim().is_empty()
                    {
                        *uses.fonts.entry(family.to_owned()).or_default() += 1;
                    }
                }
                _ => {}
            }
            map.values().for_each(|item| walk(item, uses));
        }
        _ => {}
    }
}

fn some(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_owned())
}

fn media_entry(item: &Value, uses: &Uses) -> AssetEntry {
    let text = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or("");
    let id = text("id").to_owned();
    let origin: Option<MediaOrigin> = item.get("origin").and_then(|value| serde_json::from_value(value.clone()).ok());
    let credit = item.get("credit").filter(|value| value.is_object());
    let mut entry = AssetEntry {
        kind: "media".to_owned(),
        name: text("name").to_owned(),
        file: some(text("file")),
        uses: uses.media.get(&id).copied().unwrap_or(0),
        id,
        ..AssetEntry::default()
    };
    let origin_kind = origin.as_ref().map(|origin| origin.kind.as_str());
    if let Some(credit) = credit {
        let field = |key: &str| credit.get(key).and_then(Value::as_str).unwrap_or("");
        entry.origin = if origin_kind == Some("library") { "library" } else { "stock" }.to_owned();
        entry.provider = some(field("provider"));
        entry.license = some(field("license"));
        entry.license_url = some(field("licenseUrl"));
        entry.creator = some(field("creator"));
        entry.source_url = some(field("sourceUrl"));
        entry.attribution = some(field("attribution"));
        entry.credit_required = license_requires_credit(field("license"));
        if entry.license.is_none() {
            entry.note = Some("The licence was not recorded with this item.".to_owned());
        }
        return entry;
    }
    match origin {
        Some(origin) if origin.kind == "generated" => {
            let tool = origin.tool.clone().unwrap_or_else(|| "a tool".to_owned());
            entry.origin = "generated".to_owned();
            entry.provider = Some(tool.clone());
            entry.derived_from = origin.from.clone();
            entry.note = Some(match (&origin.from, &origin.model) {
                (Some(_), Some(model)) => format!("Made by the {tool} tool from another item (model: {model}); it keeps that item's rights."),
                (Some(_), None) => format!("Made by the {tool} tool from another item; it keeps that item's rights."),
                (None, Some(model)) => format!("Made by the {tool} tool (model: {model})."),
                (None, None) => format!("Made by the {tool} tool."),
            });
        }
        Some(origin) if origin.kind == "library" => {
            entry.origin = "library".to_owned();
            entry.source_url = origin.source.clone();
            entry.note = Some("A library sound whose licence was not recorded.".to_owned());
        }
        _ => {
            entry.origin = "imported".to_owned();
            entry.note = Some("Added from your own files: the rights are yours to check.".to_owned());
        }
    }
    entry
}

fn sound_entry(url: &str, name: &str, count: u32) -> AssetEntry {
    let mut entry = AssetEntry {
        kind: "sound".to_owned(),
        id: url.to_owned(),
        name: name.to_owned(),
        origin: "library".to_owned(),
        uses: count,
        ..AssetEntry::default()
    };
    if url.contains("freesound.org") {
        entry.provider = Some("freesound".to_owned());
        entry.source_url = freesound_page(url).or_else(|| Some(url.to_owned()));
        entry.note = Some("A Freesound preview: check its licence and credit on its Freesound page.".to_owned());
    } else if url.starts_with("/sounds/") {
        entry.provider = Some("bundled-sounds".to_owned());
        entry.note = Some("One of the editor's bundled sounds, not copied into this bundle.".to_owned());
    } else {
        entry.source_url = Some(url.to_owned());
        entry.note = Some("A library sound whose licence was not recorded.".to_owned());
    }
    entry
}

fn sticker_entry(sticker_id: &str, name: &str, count: u32, lookups: &AssetLookups) -> AssetEntry {
    let mut entry = AssetEntry {
        kind: "sticker".to_owned(),
        id: sticker_id.to_owned(),
        name: name.to_owned(),
        origin: "fetched".to_owned(),
        uses: count,
        ..AssetEntry::default()
    };
    let (provider, value) = sticker_id.split_once(':').unwrap_or((sticker_id, ""));
    match provider {
        "icons" => {
            let (prefix, icon) = value.split_once(':').unwrap_or((value, ""));
            entry.provider = Some("iconify".to_owned());
            entry.source_url = Some(format!("https://icon-sets.iconify.design/{prefix}/{icon}/"));
            match prefix {
                "twemoji" => {
                    entry.license = Some("CC-BY-4.0".to_owned());
                    entry.license_url = Some("https://creativecommons.org/licenses/by/4.0/".to_owned());
                    entry.creator = Some("Twitter".to_owned());
                    entry.attribution = Some("Twemoji by Twitter (jdecked/twemoji), CC BY 4.0".to_owned());
                }
                "noto" => {
                    entry.license = Some("Apache-2.0".to_owned());
                    entry.license_url = Some("https://www.apache.org/licenses/LICENSE-2.0".to_owned());
                    entry.creator = Some("Google".to_owned());
                    entry.attribution = Some("Noto Emoji by Google, Apache 2.0".to_owned());
                }
                _ => match lookups.icon_sets.get(prefix) {
                    Some(set) if set.license.is_some() => {
                        let license = set.license.clone().unwrap_or_default();
                        entry.license = Some(license.clone());
                        entry.license_url = set.license_url.clone();
                        entry.creator = set.author.clone();
                        entry.attribution = Some(match &set.author {
                            Some(author) => format!("{} by {author} ({license})", set.name),
                            None => format!("{} ({license})", set.name),
                        });
                    }
                    _ => entry.note = Some(format!("Icon set \"{prefix}\": its licence was not looked up.")),
                },
            }
        }
        "logos" => {
            let slug = value.split(':').next().unwrap_or("");
            entry.provider = Some("simple-icons".to_owned());
            entry.source_url = Some(format!("https://simpleicons.org/?q={slug}"));
            match lookups.brands.get(slug) {
                Some(brand) => {
                    entry.license = some(&brand.license);
                    entry.attribution = brand.credit_required.then(|| format!("{} icon from Simple Icons ({})", brand.title, brand.license));
                    entry.note = Some(format!("{} is a trademark of its owner.", brand.title));
                }
                None => entry.note = Some("A brand logo (a trademark of its owner); its licence was not looked up.".to_owned()),
            }
        }
        "flags" => {
            entry.origin = "built-in".to_owned();
            entry.provider = Some("conzacut".to_owned());
            entry.note = Some("A flag picture shipped with the editor; its source licence is not recorded.".to_owned());
        }
        "shapes" => {
            entry.origin = "built-in".to_owned();
            entry.provider = Some("conzacut".to_owned());
            entry.note = Some("A shape drawn by the editor.".to_owned());
        }
        _ => entry.note = Some("A sticker from an unknown source.".to_owned()),
    }
    entry.credit_required = entry.license.as_deref().is_some_and(license_requires_credit)
        || (provider == "logos" && lookups.brands.get(value.split(':').next().unwrap_or("")).is_some_and(|brand| brand.credit_required));
    entry
}

fn font_entry(family: &str, count: u32, lookups: &AssetLookups) -> AssetEntry {
    let mut entry = AssetEntry {
        kind: "font".to_owned(),
        id: family.to_owned(),
        name: family.to_owned(),
        uses: count,
        ..AssetEntry::default()
    };
    if SYSTEM_FONTS.contains(&family) {
        entry.origin = "system".to_owned();
        entry.note = Some("A system font: not included; each computer's own copy is used.".to_owned());
        return entry;
    }
    entry.origin = "fetched".to_owned();
    match lookups.fonts.get(family) {
        Some(license) => {
            entry.provider = Some("fontsource".to_owned());
            entry.license = some(license);
            entry.source_url = Some(format!("https://fontsource.org/fonts/{}", fontsource_slug(family)));
        }
        None => entry.note = Some("A web font whose licence was not looked up.".to_owned()),
    }
    entry.credit_required = entry.license.as_deref().is_some_and(license_requires_credit);
    entry
}

/// The manifest of a bundle's `project.json` (`{"project", "media"}`).
pub fn asset_manifest(bundle: &Value, lookups: &AssetLookups) -> Value {
    let mut uses = Uses::default();
    if let Some(scenes) = bundle.get("project").and_then(|project| project.get("scenes")) {
        walk(scenes, &mut uses);
    }
    let media: Vec<AssetEntry> = bundle
        .get("media")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(|item| media_entry(item, &uses)).collect())
        .unwrap_or_default();
    // Made-from items keep the rights of what they were made from.
    let by_id: HashMap<String, AssetEntry> = media.iter().map(|entry| (entry.id.clone(), entry.clone())).collect();
    let mut assets: Vec<AssetEntry> = media
        .into_iter()
        .map(|mut entry| {
            if let Some(source) = entry.derived_from.as_ref().and_then(|from| by_id.get(from))
                && entry.origin == "generated"
            {
                entry.license = source.license.clone();
                entry.license_url = source.license_url.clone();
                entry.creator = source.creator.clone();
                entry.source_url = source.source_url.clone();
                entry.attribution = source.attribution.clone();
                entry.credit_required = source.credit_required;
            }
            entry
        })
        .collect();
    assets.extend(uses.sounds.iter().map(|(url, (name, count))| sound_entry(url, name, *count)));
    assets.extend(uses.stickers.iter().map(|(id, (name, count))| sticker_entry(id, name, *count, lookups)));
    assets.extend(uses.fonts.iter().map(|(family, count)| font_entry(family, *count, lookups)));

    let mut credits: Vec<String> = Vec::new();
    for entry in &assets {
        if let Some(line) = &entry.attribution
            && !credits.contains(line)
        {
            credits.push(line.clone());
        }
    }
    let project = bundle.get("project").and_then(|project| project.get("metadata"));
    let field = |key: &str| project.and_then(|metadata| metadata.get(key)).cloned().unwrap_or(Value::Null);
    json!({
        "format": ASSET_MANIFEST_FORMAT,
        "formatVersion": ASSET_MANIFEST_FORMAT_VERSION,
        "project": {"id": field("id"), "name": field("name")},
        "summary": {
            "assets": assets.len(),
            "creditRequired": assets.iter().filter(|entry| entry.credit_required).count(),
            "licenseUnknown": assets.iter().filter(|entry| entry.license.is_none() && entry.origin != "system").count(),
        },
        "credits": credits,
        "assets": assets,
    })
}

/// The credit lines of a manifest, one per line.
pub fn manifest_credits_text(manifest: &Value) -> String {
    manifest
        .get("credits")
        .and_then(Value::as_array)
        .map(|lines| lines.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same bundle and lookups as `apps/web/src/project/__tests__/assets.test.ts`.
    pub(crate) fn fixture() -> (Value, AssetLookups) {
        let bundle: Value = serde_json::from_str(include_str!("../../../fixtures/asset-manifest/bundle.json")).unwrap();
        let mut lookups = AssetLookups::default();
        lookups.icon_sets.insert(
            "mdi".into(),
            IconSetInfo {
                name: "Material Design Icons".into(),
                license: Some("Apache-2.0".into()),
                license_url: Some("https://github.com/Templarian/MaterialDesign/blob/master/LICENSE".into()),
                author: Some("Pictogrammers".into()),
            },
        );
        lookups.icon_sets.insert("cc".into(), IconSetInfo { name: "Credit Set".into(), license: Some("CC-BY-4.0".into()), license_url: None, author: Some("Someone".into()) });
        lookups.brands.insert("youtube".into(), BrandInfo { title: "YouTube".into(), license: "CC0-1.0".into(), credit_required: false });
        lookups.fonts.insert("Inter".into(), "OFL-1.1".into());
        (bundle, lookups)
    }

    #[test]
    fn matches_the_shared_expected_manifest() {
        let (bundle, lookups) = fixture();
        let manifest = asset_manifest(&bundle, &lookups);
        let expected: Value = serde_json::from_str(include_str!("../../../fixtures/asset-manifest/expected.json")).unwrap();
        assert_eq!(manifest, expected, "{}", serde_json::to_string_pretty(&manifest).unwrap());
    }

    #[test]
    fn freesound_pages_and_slugs() {
        assert_eq!(freesound_page("https://cdn.freesound.org/previews/123/123456_789-hq.mp3").as_deref(), Some("https://freesound.org/s/123456/"));
        assert_eq!(freesound_page("https://freesound.org/x.mp3"), None);
        assert_eq!(fontsource_slug("  Open Sans "), "open-sans");
        assert_eq!(fontsource_slug("M PLUS 1p"), "m-plus-1p");
        assert!(license_requires_credit("CC BY 2.0"));
        assert!(license_requires_credit("CC-BY-4.0"));
        assert!(!license_requires_credit("CC0 1.0"));
    }
}
