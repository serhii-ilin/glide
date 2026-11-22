//! Serialized control request and result types shared by session and MCP layers.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const DEFAULT_MAX_CHARS: usize = 20_000;
pub const DEFAULT_QUERY_LIMIT: usize = 50;
pub const DEFAULT_NAV_TIMEOUT_MS: u64 = 12_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum ControlRequest {
    Navigate(NavigateParams),
    Back(WaitParams),
    Forward(WaitParams),
    Reload(WaitParams),
    PageInfo,
    SnapshotText(TextParams),
    Dom(DomParams),
    Query(QueryParams),
    Screenshot,
}

/// Arguments for `glide_navigate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NavigateParams {
    /// Absolute `http(s):`, `file:`, or `data:` URL to load.
    pub url: String,
    /// Optional navigation settle timeout in milliseconds (default [`DEFAULT_NAV_TIMEOUT_MS`]).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Optional wait timeout for history navigation tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
pub struct WaitParams {
    /// Optional settle timeout in milliseconds (default [`DEFAULT_NAV_TIMEOUT_MS`]).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Arguments for `glide_snapshot_text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TextParams {
    /// Maximum Unicode scalar values to return (default [`DEFAULT_MAX_CHARS`], currently 20000).
    #[serde(default)]
    pub max_chars: Option<usize>,
}

/// Arguments for `glide_dom`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DomParams {
    /// CSS selector for the subtree to serialize; omit for the full document.
    #[serde(default)]
    pub selector: Option<String>,
    /// When true and `selector` matches multiple nodes, return all matches.
    #[serde(default)]
    pub all: bool,
    /// Maximum Unicode scalar values of HTML to return (default [`DEFAULT_MAX_CHARS`]).
    #[serde(default)]
    pub max_chars: Option<usize>,
}

/// Arguments for `glide_query`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct QueryParams {
    /// CSS selector to match elements.
    pub selector: String,
    /// Maximum number of elements (default [`DEFAULT_QUERY_LIMIT`], currently 50).
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlResult {
    Page(PageInfo),
    Text {
        text: String,
        truncated: bool,
    },
    Html {
        html: String,
        matched: usize,
        truncated: bool,
    },
    Elements(Vec<ElementInfo>),
    Png(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageInfo {
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub js_enabled: bool,
    pub status: LoadStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadStatus {
    Settled,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementInfo {
    pub tag: String,
    pub id: Option<String>,
    pub class: Option<String>,
    pub text: String,
    pub rect: Rect,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Truncate `s` to at most `max` Unicode scalar values; returns whether anything was cut.
pub fn truncate_chars(s: &str, max: usize) -> (String, bool) {
    if max == 0 {
        return (String::new(), !s.is_empty());
    }
    let count = s.chars().count();
    if count <= max {
        return (s.to_string(), false);
    }
    let end = s
        .char_indices()
        .nth(max)
        .map(|(idx, _)| idx)
        .unwrap_or(s.len());
    (s[..end].to_string(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_json_roundtrip() {
        let req = ControlRequest::Dom(DomParams {
            selector: Some("#x".into()),
            all: true,
            max_chars: Some(100),
        });
        let json = serde_json::to_string(&req).expect("serialize");
        assert!(json.contains("\"method\":\"dom\""));
        let back: ControlRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, req);
    }

    #[test]
    fn truncate_chars_respects_char_boundary() {
        let (out, cut) = truncate_chars("a🙂b", 2);
        assert_eq!(out, "a🙂");
        assert!(cut);
        let (whole, cut) = truncate_chars("hello", 10);
        assert_eq!(whole, "hello");
        assert!(!cut);
    }
}
