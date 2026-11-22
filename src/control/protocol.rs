//! Serialized control request and result types shared by session and MCP layers.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const DEFAULT_MAX_CHARS: usize = 20_000;
pub const DEFAULT_QUERY_LIMIT: usize = 50;
pub const DEFAULT_CONSOLE_LIMIT: usize = 100;
pub const MAX_CONSOLE_LIMIT: usize = 500;
pub const CONSOLE_MESSAGE_MAX_CHARS: usize = 2_000;
pub const DEFAULT_NAV_TIMEOUT_MS: u64 = 12_000;
/// Upper bound for client-supplied navigation/wait timeouts (MCP and control callers).
pub const MAX_CLIENT_TIMEOUT_MS: u64 = 120_000;

/// Resolve an optional client timeout to milliseconds, applying [`DEFAULT_NAV_TIMEOUT_MS`]
/// when absent and capping at [`MAX_CLIENT_TIMEOUT_MS`].
pub fn clamp_client_timeout_ms(timeout_ms: Option<u64>) -> u64 {
    let ms = timeout_ms.unwrap_or(DEFAULT_NAV_TIMEOUT_MS);
    ms.min(MAX_CLIENT_TIMEOUT_MS)
}

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
    ConsoleMessages(ConsoleParams),
    Eval(EvalParams),
}

/// Arguments for `glide_navigate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NavigateParams {
    /// Absolute `http(s):`, `file:`, or `data:` URL to load.
    pub url: String,
    /// Optional navigation settle timeout in milliseconds (default 12000 ms; clamped to 120000 ms max).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Optional wait timeout for history navigation tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
pub struct WaitParams {
    /// Optional settle timeout in milliseconds (default 12000 ms; clamped to 120000 ms max).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Arguments for `glide_snapshot_text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TextParams {
    /// Maximum Unicode scalar values to return (default 20000 characters).
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
    /// Maximum Unicode scalar values of HTML to return (default 20000 characters).
    #[serde(default)]
    pub max_chars: Option<usize>,
}

/// Arguments for `glide_query`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct QueryParams {
    /// CSS selector to match elements.
    pub selector: String,
    /// Maximum number of elements (default 50).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Arguments for `glide_console_messages`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConsoleParams {
    /// Sequence cursor; omit or 0 to read from the oldest retained entry (default 0).
    #[serde(default)]
    pub since: Option<u64>,
    /// When set, only rows at these levels (e.g. log, warn, error, uncaught, rejection,
    /// repl-input, repl-result; case-insensitive).
    #[serde(default)]
    pub levels: Option<Vec<String>>,
    /// Maximum rows to return (default 100, maximum 500).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Arguments for `glide_eval`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EvalParams {
    /// JavaScript expression to evaluate in the page global scope (indirect eval).
    pub expression: String,
    /// When true, return `JSON.stringify` output instead of a DevTools-style preview.
    #[serde(default)]
    pub json: bool,
    /// Maximum Unicode scalar values of the value text (default 20000 characters).
    #[serde(default)]
    pub max_chars: Option<usize>,
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
    Console(ConsoleSlice),
    Eval {
        value: String,
        truncated: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsoleSlice {
    pub entries: Vec<ConsoleMessage>,
    pub next_cursor: u64,
    pub dropped: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsoleMessage {
    pub seq: u64,
    pub level: String,
    pub message: String,
    pub source: Option<String>,
    pub line: Option<usize>,
    pub timestamp_ms: u64,
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
    fn clamp_client_timeout_ms_uses_default_and_caps() {
        assert_eq!(clamp_client_timeout_ms(None), DEFAULT_NAV_TIMEOUT_MS);
        assert_eq!(clamp_client_timeout_ms(Some(5_000)), 5_000);
        assert_eq!(
            clamp_client_timeout_ms(Some(999_000)),
            MAX_CLIENT_TIMEOUT_MS
        );
    }

    #[test]
    fn tool_input_schemas_use_plain_descriptions() {
        for schema in [
            schemars::schema_for!(NavigateParams),
            schemars::schema_for!(DomParams),
            schemars::schema_for!(QueryParams),
        ] {
            let json = serde_json::to_string(&schema).expect("schema json");
            assert!(
                !json.contains("[`"),
                "schema must not expose Rust doc links: {json}"
            );
        }
        let nav = serde_json::to_string(&schemars::schema_for!(NavigateParams)).expect("nav");
        let dom = serde_json::to_string(&schemars::schema_for!(DomParams)).expect("dom");
        let query = serde_json::to_string(&schemars::schema_for!(QueryParams)).expect("query");
        assert!(
            nav.contains("12000"),
            "navigate timeout default in schema: {nav}"
        );
        assert!(
            dom.contains("20000"),
            "dom max_chars default in schema: {dom}"
        );
        assert!(
            query.contains("50"),
            "query limit default in schema: {query}"
        );
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
