//! Stock media on the desktop: Openverse searched directly (no key) and
//! Pixabay through the shared API worker (which holds the key and keeps
//! Pixabay's 24-hour cache). Files go into `~/.opencut/stock/`; the credit
//! goes on the project (`editor::StockCredit`, carried in bundles).

use std::io::Read;
use std::path::{Path, PathBuf};

use editor::StockItem;

/// Files larger than this are refused.
const MAX_STOCK_BYTES: u64 = 200 * 1024 * 1024;

/// The API worker: `OPENCUT_API_URL`, else the deployed one.
fn api_url() -> String {
    std::env::var("OPENCUT_API_URL").unwrap_or_else(|_| editor::DEFAULT_API_URL.to_owned())
}

fn get_text(url: &str) -> Result<String, ureq::Error> {
    let response = ureq::get(url).header("User-Agent", "ConzaCut desktop").call()?;
    let mut text = String::new();
    response.into_body().into_reader().take(16 * 1024 * 1024).read_to_string(&mut text)?;
    Ok(text)
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Searches a provider; `kind` is "image" or "video".
pub(crate) fn search(provider: &str, kind: &str, query: &str, page: u32) -> Result<Vec<StockItem>, String> {
    let query: String = query.chars().take(100).collect();
    if provider == "openverse" {
        if kind != "image" {
            return Err(crate::i18n::t("Openverse has pictures only; videos come from Pixabay.").to_owned());
        }
        let url = format!(
            "https://api.openverse.org/v1/images/?q={}&page={page}&page_size=20&license={}&mature=false",
            encode(&query),
            editor::OPENVERSE_LICENSES
        );
        let text = get_text(&url).map_err(|error| match error {
            ureq::Error::StatusCode(429) => crate::i18n::t("Openverse's limit for this hour or day is reached; try again later.").to_owned(),
            other => format!("Openverse: {other}"),
        })?;
        let json: serde_json::Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        return Ok(editor::parse_openverse(&json));
    }
    let url = format!("{}/stock/search?q={}&kind={kind}&page={page}", api_url(), encode(&query));
    let text = get_text(&url).map_err(|error| match error {
        ureq::Error::StatusCode(503) => crate::i18n::t("Pixabay is not set up on this server yet (it needs an API key).").to_owned(),
        other => format!("Pixabay: {other}"),
    })?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let results = json.get("results").cloned().unwrap_or(serde_json::Value::Array(Vec::new()));
    serde_json::from_value(results).map_err(|error| error.to_string())
}

fn extension(content_type: &str, kind: &str) -> &'static str {
    match content_type.split(';').next().unwrap_or("").trim() {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/jpeg" | "image/jpg" => "jpg",
        "video/webm" => "webm",
        "video/mp4" => "mp4",
        _ if kind == "video" => "mp4",
        _ => "jpg",
    }
}

fn download_to(url: &str, kind: &str, dir: &Path, stem: &str) -> Result<PathBuf, String> {
    let response = ureq::get(url).header("User-Agent", "ConzaCut desktop").call().map_err(|error| format!("{url}: {error}"))?;
    let content_type = response.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_owned();
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_STOCK_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_STOCK_BYTES {
        return Err("The file is too large".to_owned());
    }
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let path = dir.join(format!("{stem}.{}", extension(&content_type, kind)));
    std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
    Ok(path)
}

/// Downloads a result into `dir`: straight from the source when it gives
/// the file out (Openverse), else (or when that fails) through the worker.
pub(crate) fn download(item: &StockItem, dir: &Path) -> Result<PathBuf, String> {
    let safe: String = item
        .title
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .take(48)
        .collect();
    let stem = format!("{}-{}-{}", item.provider, safe.trim().replace(' ', "-"), &item.id[..item.id.len().min(12)]);
    if let Some(url) = &item.file_url
        && let Ok(path) = download_to(url, &item.kind, dir, &stem)
    {
        return Ok(path);
    }
    let url = format!("{}/stock/file?provider={}&id={}&kind={}", api_url(), encode(&item.provider), encode(&item.id), encode(&item.kind));
    download_to(&url, &item.kind, dir, &stem)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `OPENCUT_STOCK_CHECK=1` searches Openverse and downloads the first
    /// picture (real network calls; one search, one file).
    #[test]
    fn searches_and_downloads_from_openverse() {
        if std::env::var("OPENCUT_STOCK_CHECK").is_err() {
            return;
        }
        let items = search("openverse", "image", "lighthouse", 1).unwrap();
        assert!(!items.is_empty());
        let dir = tempfile::tempdir().unwrap();
        let path = download(&items[0], dir.path()).unwrap();
        let size = std::fs::metadata(&path).unwrap().len();
        eprintln!("STOCK {} by {} ({}) -> {} ({size} bytes)", items[0].title, items[0].creator, items[0].license, path.display());
        assert!(size > 1000);
        let pixabay = search("pixabay", "video", "sea", 1);
        eprintln!("PIXABAY {pixabay:?}");
    }
}
