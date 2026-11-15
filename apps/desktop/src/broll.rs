//! Script to B-roll on the desktop, as the web app does it
//! (`apps/web/src/broll`): the shot list comes from `editor::plan_shots`
//! (shared with the web app), Claude's search words optionally from the API
//! worker (`POST /broll/plan`, which needs the server passcode, `shareKey`),
//! and the pictures from the stock search (`crate::stock`).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::settings::api_url;

/// Claude's query for each shot's line, from the API worker.
pub(crate) fn claude_queries(shots: &[editor::Shot], passcode: &str) -> Result<Vec<String>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(120)))
        .http_status_as_error(false)
        .build()
        .into();
    let lines: Vec<&str> = shots.iter().map(|shot| shot.text.as_str()).collect();
    let response = agent
        .post(&format!("{}/broll/plan", api_url()))
        .header("X-Share-Key", passcode)
        .header("Content-Type", "application/json")
        .send(serde_json::json!({ "lines": lines }).to_string())
        .map_err(|error| error.to_string())?;
    let status = response.status().as_u16();
    let mut text = String::new();
    response.into_body().into_reader().take(1 << 20).read_to_string(&mut text).map_err(|error| error.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    if !(200..300).contains(&status) {
        let message = json.get("error").and_then(serde_json::Value::as_str).unwrap_or(&text);
        return Err(format!("Claude search words ({status}): {message}"));
    }
    let queries: Vec<String> = json
        .get("queries")
        .and_then(serde_json::Value::as_array)
        .map(|items| items.iter().filter_map(|item| item.as_str().map(|query| query.trim().to_owned())).collect())
        .unwrap_or_default();
    if queries.len() != shots.len() || queries.iter().any(String::is_empty) {
        return Err("The server's answer did not have one query per line".to_owned());
    }
    Ok(queries)
}

/// What became of one shot: its picture or video on disk with the result
/// it came from, or why there is none.
pub(crate) enum Found {
    File(editor::StockItem, PathBuf),
    Problem(String),
}

/// The first result for each shot's query, downloaded into `dir`.
pub(crate) fn fetch(shots: &[editor::Shot], provider: &str, kind: &str, dir: &Path) -> Vec<Found> {
    shots
        .iter()
        .map(|shot| {
            let items = match crate::stock::search(provider, kind, &shot.query, 1) {
                Ok(items) => items,
                Err(error) => return Found::Problem(error),
            };
            let Some(item) = items.into_iter().next() else {
                return Found::Problem(crate::i18n::t("No results").to_owned());
            };
            match crate::stock::download(&item, dir) {
                Ok(path) => Found::File(item, path),
                Err(error) => Found::Problem(error),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `OPENCUT_BROLL_CHECK=1` searches Openverse for two planned shots and
    /// downloads the first picture of each (network; no key needed).
    #[test]
    fn finds_pictures_for_a_script() {
        if std::env::var("OPENCUT_BROLL_CHECK").is_err() {
            return;
        }
        let shots = editor::plan_shots("Morning light falls over the mountains.\n\nFresh bread cools on a wooden table.", 2.5);
        assert_eq!(shots.len(), 2);
        let dir = tempfile::tempdir().unwrap();
        let found = fetch(&shots, "openverse", "image", dir.path());
        for (shot, found) in shots.iter().zip(&found) {
            match found {
                Found::File(item, path) => {
                    eprintln!("{} -> {} ({} bytes)", shot.query, item.title, std::fs::metadata(path).unwrap().len());
                    assert!(std::fs::metadata(path).unwrap().len() > 1000);
                }
                Found::Problem(problem) => panic!("{}: {problem}", shot.query),
            }
        }
    }
}
