//! Desktop preferences kept in `~/.opencut/settings.json`: how big the
//! preview renders and whether it decodes from proxy media.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// How many pixels the preview renders.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PreviewQuality {
    /// The size the frame is shown at (never more than the canvas).
    #[default]
    Auto,
    /// The canvas size, whatever the panel's size.
    Full,
    /// Half of Auto, each side.
    Half,
    /// A quarter of Auto, each side.
    Quarter,
}

impl PreviewQuality {
    pub(crate) const ALL: [PreviewQuality; 4] = [Self::Auto, Self::Full, Self::Half, Self::Quarter];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Full => "Full",
            Self::Half => "1/2",
            Self::Quarter => "1/4",
        }
    }

    /// The preview's pixel size for a frame shown at `fitted` (already fitted
    /// to the canvas aspect) on a canvas of `canvas`.
    pub(crate) fn target(self, fitted: (u32, u32), canvas: (u32, u32)) -> (u32, u32) {
        let scale = |(width, height): (u32, u32), factor: f32| {
            (
                (((width as f32 * factor).round() as u32).max(2)) & !1,
                (((height as f32 * factor).round() as u32).max(2)) & !1,
            )
        };
        match self {
            Self::Auto => fitted,
            Self::Full => scale(canvas, 1.0),
            Self::Half => scale(fitted, 0.5),
            Self::Quarter => scale(fitted, 0.25),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Settings {
    pub preview_quality: PreviewQuality,
    /// Decode the preview from proxies when they exist.
    pub use_proxies: bool,
    /// Make proxies on import for videos larger than 1080p.
    pub auto_proxies: bool,
    /// Interface language: `auto`, `en` or `zh-CN`.
    pub locale: String,
    /// Passcode for sharing projects by link (the API worker's
    /// `SHARE_UPLOAD_KEY`); `OPENCUT_SHARE_KEY` overrides it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub share_key: String,
    /// The API worker to use instead of the hosted one, such as
    /// `https://cut.example.com/api` for a self-hosted server
    /// (`infra/self-host`); `OPENCUT_API_URL` overrides it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub api_url: String,
    /// The web app that share links open in, such as
    /// `https://cut.example.com`; `OPENCUT_WEB_URL` overrides it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub web_url: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            preview_quality: PreviewQuality::Auto,
            use_proxies: true,
            auto_proxies: true,
            locale: "auto".to_owned(),
            share_key: String::new(),
            api_url: String::new(),
            web_url: String::new(),
        }
    }
}

/// The hosted web app, used when no other is set.
pub(crate) const DEFAULT_WEB_URL: &str = "https://conzacut.conzahub.com";

/// The first of the environment variable and the saved setting that is an
/// `http://` or `https://` address (trailing slashes removed), else `default`.
fn server_url(from_env: Option<String>, saved: &str, default: &str) -> String {
    [from_env.as_deref(), Some(saved)]
        .into_iter()
        .flatten()
        .map(|value| value.trim().trim_end_matches('/'))
        .find(|value| {
            let rest = value.strip_prefix("https://").or_else(|| value.strip_prefix("http://"));
            rest.is_some_and(|rest| !rest.is_empty() && !rest.chars().any(char::is_whitespace))
        })
        .unwrap_or(default)
        .to_owned()
}

/// The API worker: `OPENCUT_API_URL`, else `apiUrl` in settings, else the hosted one.
pub(crate) fn api_url() -> String {
    server_url(std::env::var("OPENCUT_API_URL").ok(), &load().api_url, editor::DEFAULT_API_URL)
}

/// The web app links open in: `OPENCUT_WEB_URL`, else `webUrl` in settings, else the hosted one.
pub(crate) fn web_url() -> String {
    server_url(std::env::var("OPENCUT_WEB_URL").ok(), &load().web_url, DEFAULT_WEB_URL)
}

fn path() -> Option<PathBuf> {
    Some(editor::ProjectStore::default_root().parent()?.join("settings.json"))
}

/// The saved settings, or the defaults.
pub(crate) fn load() -> Settings {
    path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub(crate) fn save(settings: &Settings) -> Result<(), String> {
    let path = path().ok_or_else(|| "No home folder for settings".to_owned())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    std::fs::write(path, text).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_urls_come_from_env_then_settings() {
        let default = "https://hosted.example";
        assert_eq!(server_url(None, "", default), default);
        assert_eq!(server_url(None, " https://cut.example/api/ ", default), "https://cut.example/api");
        assert_eq!(server_url(Some("http://localhost:8080/api".into()), "https://cut.example/api", default), "http://localhost:8080/api");
        assert_eq!(server_url(Some(String::new()), "https://cut.example/api", default), "https://cut.example/api");
        for bad in ["cut.example", "ftp://cut.example", "https://", "https://a b"] {
            assert_eq!(server_url(None, bad, default), default, "{bad}");
        }
        let saved: Settings = serde_json::from_str(r#"{"apiUrl":"https://cut.example/api","webUrl":"https://cut.example"}"#).unwrap();
        assert_eq!((saved.api_url.as_str(), saved.web_url.as_str()), ("https://cut.example/api", "https://cut.example"));
        assert!(!serde_json::to_string(&Settings::default()).unwrap().contains("apiUrl"));
    }

    #[test]
    fn targets_scale_the_fitted_size() {
        let fitted = (640, 360);
        let canvas = (1920, 1080);
        assert_eq!(PreviewQuality::Auto.target(fitted, canvas), (640, 360));
        assert_eq!(PreviewQuality::Full.target(fitted, canvas), (1920, 1080));
        assert_eq!(PreviewQuality::Half.target(fitted, canvas), (320, 180));
        assert_eq!(PreviewQuality::Quarter.target(fitted, canvas), (160, 90));
        assert_eq!(PreviewQuality::Quarter.target((6, 4), canvas), (2, 2));
    }

    #[test]
    fn settings_read_partial_files() {
        let parsed: Settings = serde_json::from_str(r#"{"previewQuality":"half"}"#).unwrap();
        assert_eq!(parsed.preview_quality, PreviewQuality::Half);
        assert!(parsed.use_proxies && parsed.auto_proxies);
        let written = serde_json::to_string(&Settings::default()).unwrap();
        assert!(written.contains("\"previewQuality\":\"auto\""), "{written}");
    }
}
