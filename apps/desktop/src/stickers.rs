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
        "logos" => {
            // Brand logos from Simple Icons, in the colour the id names.
            let (slug, hex) = value.split_once(':')?;
            let valid_slug = !slug.is_empty() && slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
            let valid_hex = hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit());
            (valid_slug && valid_hex).then(|| format!("{ICONIFY_API}/simple-icons/{slug}.svg?color=%23{}", hex.to_ascii_uppercase()))
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

// ---- packs: emoji and brands -----------------------------------------

/// The brand list the web app's Brands pack uses (built from Simple Icons
/// by `scripts/build-brand-stickers.py`).
const BRAND_DATA: &str = include_str!("../../web/src/stickers/providers/brands.json");

/// Shown with the Brands pack, as the web app does.
pub(crate) const TRADEMARK_NOTICE: &str = "Brand logos are trademarks of their owners. Use them only to refer to the brand, follow the brand's guidelines and do not suggest endorsement. Icons from Simple Icons (CC0).";

/// The web app's popular picks, shown before searching.
pub(crate) const POPULAR_BRANDS: &[&str] = &[
    "youtube", "instagram", "tiktok", "x", "facebook", "threads", "snapchat", "bluesky", "reddit", "discord", "twitch", "kick",
    "spotify", "applemusic", "soundcloud", "netflix", "apple", "google", "android", "paypal", "stripe", "github", "gitlab",
    "figma", "notion", "whatsapp", "telegram", "signal", "zoom", "gmail", "wordpress", "shopify", "airbnb", "uber", "adidas",
    "nike", "playstation", "steam", "epicgames", "anthropic", "claude", "mastodon", "pinterest", "tumblr", "vimeo", "patreon",
    "kofi", "substack", "medium", "dribbble", "behance", "davinciresolve", "obsstudio", "unity", "unrealengine",
];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Brand {
    pub slug: String,
    pub title: String,
    pub hex: String,
    /// The icon's licence and whether it asks for a credit (`brands.json`).
    pub license: String,
    pub credit: bool,
}

pub(crate) fn brands() -> &'static [Brand] {
    static BRANDS: std::sync::OnceLock<Vec<Brand>> = std::sync::OnceLock::new();
    BRANDS.get_or_init(|| {
        #[derive(serde::Deserialize)]
        struct File {
            #[serde(default)]
            license: String,
            brands: Vec<(String, String, String, String, String, bool)>,
        }
        serde_json::from_str::<File>(BRAND_DATA)
            .map(|file| {
                // An empty per-icon licence means the collection's own (as the web app reads it).
                let default = file.license;
                file.brands
                    .into_iter()
                    .map(|(slug, title, hex, _, license, credit)| Brand {
                        slug,
                        title,
                        hex,
                        license: if license.is_empty() { default.clone() } else { license },
                        credit,
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// Brands whose title or slug matches, exact first (the web app's order).
pub(crate) fn search_brands(query: &str, limit: usize) -> Vec<&'static Brand> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        let popular: Vec<&Brand> = POPULAR_BRANDS.iter().filter_map(|slug| brands().iter().find(|brand| brand.slug == *slug)).collect();
        return popular.into_iter().take(limit).collect();
    }
    let compact: String = needle.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let mut scored: Vec<(u8, &Brand)> = brands()
        .iter()
        .filter_map(|brand| {
            let title = brand.title.to_lowercase();
            let score = if title == needle || brand.slug == compact {
                0
            } else if title.starts_with(&needle) || (!compact.is_empty() && brand.slug.starts_with(&compact)) {
                1
            } else if title.contains(&needle) || (!compact.is_empty() && brand.slug.contains(&compact)) {
                2
            } else {
                return None;
            };
            Some((score, brand))
        })
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.title.len().cmp(&b.1.title.len())).then(a.1.title.cmp(&b.1.title)));
    scored.into_iter().take(limit).map(|(_, brand)| brand).collect()
}

/// The sticker id of a brand logo in `hex` (RRGGBB).
pub(crate) fn brand_sticker_id(slug: &str, hex: &str) -> String {
    format!("logos:{slug}:{}", hex.to_ascii_uppercase())
}

/// The emoji sets the web app offers: id, name and the credit to give.
pub(crate) const EMOJI_SETS: &[(&str, &str, &str)] = &[
    ("twemoji", "Twemoji", "Twemoji by Twitter, CC BY 4.0: credit it where the video is published."),
    ("noto", "Noto Emoji", "Noto Emoji by Google, Apache 2.0."),
];

/// Skin tones: label and the name suffix the sets use.
pub(crate) const SKIN_TONES: &[(&str, &str)] = &[
    ("Default", ""),
    ("Light", "-light-skin-tone"),
    ("Medium light", "-medium-light-skin-tone"),
    ("Medium", "-medium-skin-tone"),
    ("Medium dark", "-medium-dark-skin-tone"),
    ("Dark", "-dark-skin-tone"),
];

const EMOJI_CATEGORY_ORDER: &[&str] = &[
    "Smileys & Emotion",
    "People & Body",
    "Animals & Nature",
    "Food & Drink",
    "Travel & Places",
    "Activities",
    "Objects",
    "Symbols",
    "Flags",
];

/// An emoji set's categories (names without a skin tone) and every name.
#[derive(Clone, Debug, Default)]
pub(crate) struct EmojiCollection {
    pub categories: Vec<(String, Vec<String>)>,
    pub names: HashSet<String>,
}

pub(crate) fn emoji_collection_from_json(text: &str) -> Result<EmojiCollection, String> {
    #[derive(serde::Deserialize)]
    struct Raw {
        #[serde(default)]
        categories: std::collections::BTreeMap<String, Vec<String>>,
    }
    let raw: Raw = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let mut collection = EmojiCollection::default();
    let mut categories: Vec<(String, Vec<String>)> = raw
        .categories
        .into_iter()
        .map(|(category, names)| {
            collection.names.extend(names.iter().cloned());
            (category, names.into_iter().filter(|name| !name.contains("skin-tone")).collect())
        })
        .collect();
    categories.sort_by_key(|(category, _)| EMOJI_CATEGORY_ORDER.iter().position(|known| known == category).unwrap_or(usize::MAX));
    collection.categories = categories;
    Ok(collection)
}

/// The set's category lists, from `dir/emoji-<set>.json` or the Iconify API
/// (cached there for next time).
pub(crate) fn load_emoji_collection(dir: &Path, set: &str) -> Result<EmojiCollection, String> {
    if !EMOJI_SETS.iter().any(|(id, ..)| *id == set) {
        return Err(format!("{set} is not an emoji set"));
    }
    let cache = dir.join(format!("emoji-{set}.json"));
    if let Ok(text) = fs::read_to_string(&cache)
        && let Ok(collection) = emoji_collection_from_json(&text)
    {
        return Ok(collection);
    }
    let url = format!("{ICONIFY_API}/collection?prefix={set}");
    let response = ureq::get(&url).call().map_err(|error| format!("{url}: {error}"))?;
    let mut text = String::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_FILE_BYTES)
        .read_to_string(&mut text)
        .map_err(|error| format!("{url}: {error}"))?;
    let collection = emoji_collection_from_json(&text)?;
    fs::create_dir_all(dir).ok();
    fs::write(&cache, &text).ok();
    Ok(collection)
}

/// The name in the chosen skin tone when the set has that variant.
pub(crate) fn with_skin_tone(name: &str, suffix: &str, names: &HashSet<String>) -> String {
    if suffix.is_empty() {
        return name.to_owned();
    }
    let toned = format!("{name}{suffix}");
    if names.contains(&toned) { toned } else { name.to_owned() }
}

/// Names containing every word of the query, shortest first.
pub(crate) fn search_emoji(collection: &EmojiCollection, query: &str, limit: usize) -> Vec<String> {
    let words: Vec<String> = query.to_lowercase().split([' ', '-']).filter(|word| !word.is_empty()).map(str::to_owned).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut matches: Vec<&String> = collection
        .categories
        .iter()
        .flat_map(|(_, names)| names.iter())
        .filter(|name| words.iter().all(|word| name.contains(word.as_str())))
        .collect();
    matches.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
    matches.dedup();
    matches.into_iter().take(limit).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brands_and_emoji_follow_the_web_app() {
        assert!(brands().len() > 3000);
        assert_eq!(search_brands("YouTube", 3)[0].slug, "youtube");
        assert!(search_brands("", 60).len() > 40, "popular picks before searching");
        let spotify = search_brands("spotify", 1)[0];
        assert_eq!(brand_sticker_id(&spotify.slug, &spotify.hex.to_lowercase()), format!("logos:spotify:{}", spotify.hex));
        assert_eq!(
            sticker_url("logos:spotify:1db954").as_deref(),
            Some("https://api.iconify.design/simple-icons/spotify.svg?color=%231DB954")
        );
        assert_eq!(sticker_url("logos:spotify"), None);
        assert_eq!(sticker_url("logos:../x:FFFFFF"), None);

        let collection = emoji_collection_from_json(
            r#"{"categories":{"Smileys & Emotion":["grinning-face","grinning-face-with-big-eyes"],"People & Body":["waving-hand","waving-hand-dark-skin-tone","person"]}}"#,
        )
        .unwrap();
        assert_eq!(collection.categories[0].0, "Smileys & Emotion");
        assert_eq!(collection.categories[1].1, vec!["waving-hand".to_owned(), "person".to_owned()]);
        assert_eq!(with_skin_tone("waving-hand", "-dark-skin-tone", &collection.names), "waving-hand-dark-skin-tone");
        assert_eq!(with_skin_tone("person", "-dark-skin-tone", &collection.names), "person");
        assert_eq!(search_emoji(&collection, "big face", 5), vec!["grinning-face-with-big-eyes".to_owned()]);
    }

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
