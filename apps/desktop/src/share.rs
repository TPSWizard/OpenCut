//! Sharing a project by link from the desktop, the way the web app does
//! (`apps/web/src/project/share.ts`): the bundle is written to a temporary
//! file and uploaded in parts to the API worker (`/share/*`, R2 behind it,
//! kept 7 days), and the link given out is the web app's Open in ConzaCut
//! link to it. The passcode comes from `OPENCUT_SHARE_KEY` or `shareKey`
//! in `~/.opencut/settings.json`.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::time::Duration;

use serde::Deserialize;

/// The web app the link opens in: `OPENCUT_WEB_URL`, else the deployed one.
fn web_origin() -> String {
    std::env::var("OPENCUT_WEB_URL").unwrap_or_else(|_| "https://conzacut.conzahub.com".to_owned())
}

fn api_url() -> String {
    std::env::var("OPENCUT_API_URL").unwrap_or_else(|_| editor::DEFAULT_API_URL.to_owned())
}

/// The sharing passcode, if one is set.
pub(crate) fn passcode() -> Option<String> {
    std::env::var("OPENCUT_SHARE_KEY")
        .ok()
        .filter(|key| !key.is_empty())
        .or_else(|| Some(crate::settings::load().share_key).filter(|key| !key.is_empty()))
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

/// The Open in ConzaCut link for a bundle URL (the web app's `openLinkFor`).
pub(crate) fn open_link_for(bundle_url: &str, origin: &str) -> String {
    format!("{}/open?bundle={}", origin.trim_end_matches('/'), encode(bundle_url))
}

#[derive(Debug)]
pub(crate) struct Shared {
    pub link: String,
    pub expires_at: String,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .http_status_as_error(false)
        .build()
        .into()
}

fn answer<T: for<'de> Deserialize<'de>>(response: ureq::http::Response<ureq::Body>) -> Result<T, String> {
    let status = response.status().as_u16();
    let mut text = String::new();
    response.into_body().into_reader().take(1 << 20).read_to_string(&mut text).map_err(|error| error.to_string())?;
    match status {
        200..=299 => serde_json::from_str(&text).map_err(|error| error.to_string()),
        401 => Err(crate::i18n::t("Wrong sharing passcode").to_owned()),
        503 => Err(crate::i18n::t("Sharing is not set up on the server yet").to_owned()),
        413 => Err(crate::i18n::t("The project is larger than 2 GB, the most a share can hold").to_owned()),
        _ => Err(format!("Sharing failed ({status}): {text}")),
    }
}

/// Uploads `project` (and its media) as a share and returns its link.
pub(crate) fn share(project: &editor::Project, key: &str) -> Result<Shared, String> {
    let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
    let path = dir.path().join("share.opencut.zip");
    editor::export_bundle(project, &path).map_err(|error| error.to_string())?;
    let size = std::fs::metadata(&path).map_err(|error| error.to_string())?.len();
    let api = api_url();
    let agent = agent();

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Start {
        id: String,
        upload_id: String,
        part_size: u64,
        parts: u64,
    }
    let start: Start = answer(
        agent
            .post(&format!("{api}/share/start"))
            .header("X-Share-Key", key)
            .header("Content-Type", "application/json")
            .send(serde_json::json!({ "size": size, "name": project.name }).to_string())
            .map_err(|error| error.to_string())?,
    )?;
    let abort = || {
        let _ = agent
            .post(&format!("{api}/share/abort"))
            .header("X-Share-Key", key)
            .header("Content-Type", "application/json")
            .send(serde_json::json!({ "id": start.id, "uploadId": start.upload_id }).to_string());
    };
    let mut file = File::open(&path).map_err(|error| error.to_string())?;
    let mut parts = Vec::new();
    for index in 0..start.parts {
        let offset = index * start.part_size;
        let length = start.part_size.min(size - offset) as usize;
        let mut chunk = vec![0u8; length];
        if let Err(error) = file.seek(SeekFrom::Start(offset)).and_then(|_| file.read_exact(&mut chunk)) {
            abort();
            return Err(error.to_string());
        }
        let url = format!("{api}/share/part?id={}&uploadId={}&part={}", start.id, encode(&start.upload_id), index + 1);
        let mut result = Err(String::new());
        for _ in 0..3 {
            result = agent
                .put(&url)
                .header("X-Share-Key", key)
                .send(&chunk[..])
                .map_err(|error| error.to_string())
                .and_then(answer::<serde_json::Value>);
            if result.is_ok() {
                break;
            }
        }
        match result {
            Ok(part) => parts.push(part),
            Err(error) => {
                abort();
                return Err(error);
            }
        }
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Done {
        url: String,
        expires_at: String,
    }
    let done: Done = answer(
        agent
            .post(&format!("{api}/share/complete"))
            .header("X-Share-Key", key)
            .header("Content-Type", "application/json")
            .send(serde_json::json!({ "id": start.id, "uploadId": start.upload_id, "parts": parts }).to_string())
            .map_err(|error| error.to_string())?,
    )?;
    Ok(Shared { link: open_link_for(&done.url, &web_origin()), expires_at: done.expires_at })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_open_in_the_web_app_and_read_back() {
        let bundle = "https://opencut-api.example/share/AAAAAAAAAAAAAAAAAAAAAA";
        let link = open_link_for(bundle, "https://conzacut.conzahub.com/");
        assert_eq!(link, "https://conzacut.conzahub.com/open?bundle=https%3A%2F%2Fopencut-api.example%2Fshare%2FAAAAAAAAAAAAAAAAAAAAAA");
        assert_eq!(editor::bundle_url_from_link(&link).as_deref(), Some(bundle));
    }

    /// `OPENCUT_SHARE_CHECK=1` with `OPENCUT_API_URL` and `OPENCUT_SHARE_KEY`
    /// shares a small synthetic project and opens the link again.
    #[test]
    fn shares_and_opens_a_project() {
        if std::env::var("OPENCUT_SHARE_CHECK").is_err() {
            return;
        }
        let mut project = editor::Project::new("Share check");
        editor::apply(
            &mut project,
            &editor::Command::InsertElement {
                element: editor::Element::text("Shared", time::MediaTime::ZERO),
                placement: editor::TrackPlacement::Auto,
            },
        )
        .unwrap();
        let shared = share(&project, &passcode().expect("OPENCUT_SHARE_KEY")).unwrap();
        eprintln!("SHARE {} until {}", shared.link, shared.expires_at);
        let dir = tempfile::tempdir().unwrap();
        let store = editor::ProjectStore::new(dir.path().join("projects"));
        let target = crate::open_link::target_from(&shared.link).unwrap();
        let report = crate::open_link::open(&target, &store).unwrap();
        assert_eq!(report.project.name, "Share check");
    }
}
