//! Element parameters: a flat map of dotted keys to numbers, strings and
//! booleans, exactly as the web Editor API exposes them. A few parameters
//! are structured (a freeform mask's `path` is a list of points); those are
//! kept as JSON so nothing is lost on the way through.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    Bool(bool),
    Number(f64),
    Text(String),
    /// Anything else (arrays, objects, null), kept as written.
    Json(serde_json::Value),
}

impl From<serde_json::Value> for ParamValue {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Bool(value) => Self::Bool(value),
            serde_json::Value::Number(number) => match number.as_f64() {
                Some(value) => Self::Number(value),
                None => Self::Json(serde_json::Value::Number(number)),
            },
            serde_json::Value::String(text) => Self::Text(text),
            other => Self::Json(other),
        }
    }
}

impl From<f64> for ParamValue {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

impl From<bool> for ParamValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<&str> for ParamValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for ParamValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl ParamValue {
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            Self::Text(text) => text.trim().parse().ok(),
            Self::Json(_) => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::Number(value) => Some(*value != 0.0),
            Self::Text(text) => match text.trim() {
                "true" | "yes" | "1" => Some(true),
                "false" | "no" | "0" => Some(false),
                _ => None,
            },
            Self::Json(_) => None,
        }
    }

    pub fn as_text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Number(value) => format_number(*value),
            Self::Bool(value) => value.to_string(),
            Self::Json(value) => value.to_string(),
        }
    }

    /// The value as JSON (text is parsed when it holds JSON, so a path
    /// stored either way reads the same).
    pub fn as_json(&self) -> serde_json::Value {
        match self {
            Self::Bool(value) => serde_json::Value::Bool(*value),
            Self::Number(value) => serde_json::json!(*value),
            Self::Text(text) => serde_json::from_str(text).unwrap_or(serde_json::Value::String(text.clone())),
            Self::Json(value) => value.clone(),
        }
    }
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Params(pub BTreeMap<String, ParamValue>);

impl Params {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> Option<&ParamValue> {
        self.0.get(key)
    }

    pub fn number(&self, key: &str, default: f64) -> f64 {
        self.0
            .get(key)
            .and_then(ParamValue::as_number)
            .unwrap_or(default)
    }

    pub fn bool(&self, key: &str, default: bool) -> bool {
        self.0
            .get(key)
            .and_then(ParamValue::as_bool)
            .unwrap_or(default)
    }

    pub fn text<'a>(&'a self, key: &str, default: &'a str) -> std::borrow::Cow<'a, str> {
        match self.0.get(key) {
            Some(ParamValue::Text(text)) => std::borrow::Cow::Borrowed(text.as_str()),
            Some(other) => std::borrow::Cow::Owned(other.as_text()),
            None => std::borrow::Cow::Borrowed(default),
        }
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<ParamValue>) -> &mut Self {
        self.0.insert(key.into(), value.into());
        self
    }

    pub fn remove(&mut self, key: &str) -> Option<ParamValue> {
        self.0.remove(key)
    }

    /// Overlays `other` on top of these parameters.
    pub fn merge(&mut self, other: &Params) {
        for (key, value) in &other.0 {
            self.0.insert(key.clone(), value.clone());
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &ParamValue)> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Parameters every visual element starts with (same defaults as the web app).
pub fn visual_defaults() -> Params {
    let mut params = Params::new();
    params
        .set("transform.positionX", 0.0)
        .set("transform.positionY", 0.0)
        .set("transform.scaleX", 1.0)
        .set("transform.scaleY", 1.0)
        .set("transform.rotate", 0.0)
        .set("opacity", 1.0)
        .set("blendMode", "normal");
    params
}

/// Parameters for elements that carry audio. `volume` is in decibels.
pub fn audio_defaults() -> Params {
    let mut params = Params::new();
    params.set("volume", 0.0).set("muted", false);
    params
}

/// Parameters for text elements. `fontSize` is in canvas pixels.
pub fn text_defaults() -> Params {
    let mut params = visual_defaults();
    params
        .set("content", "Default text")
        .set("fontFamily", "Arial")
        .set("fontSize", 72.0)
        .set("color", "#ffffff")
        .set("textAlign", "center")
        .set("fontWeight", "normal")
        .set("fontStyle", "normal")
        .set("letterSpacing", 0.0)
        .set("lineHeight", 1.2)
        .set("background.enabled", false)
        .set("background.color", "#000000")
        .set("background.cornerRadius", 0.0)
        .set("background.paddingX", 30.0)
        .set("background.paddingY", 42.0);
    params
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_values_leniently() {
        let mut params = Params::new();
        params.set("a", 2.0).set("b", "3.5").set("c", true).set("d", "no");
        assert_eq!(params.number("a", 0.0), 2.0);
        assert_eq!(params.number("b", 0.0), 3.5);
        assert_eq!(params.number("missing", 7.0), 7.0);
        assert!(params.bool("c", false));
        assert!(!params.bool("d", true));
        assert_eq!(params.text("a", ""), "2");
    }

    #[test]
    fn serialises_as_a_flat_object() {
        let mut params = Params::new();
        params.set("opacity", 0.5).set("content", "Hi").set("muted", true);
        let json = serde_json::to_string(&params).unwrap();
        assert_eq!(json, r#"{"content":"Hi","muted":true,"opacity":0.5}"#);
        let back: Params = serde_json::from_str(&json).unwrap();
        assert_eq!(back, params);
    }
}
