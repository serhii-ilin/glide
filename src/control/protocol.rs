//! Serialized control request and result types shared by session and MCP layers.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const DEFAULT_MAX_CHARS: usize = 20_000;
pub const DEFAULT_QUERY_LIMIT: usize = 50;
pub const DEFAULT_NAV_TIMEOUT_MS: u64 = 12_000;
/// Upper bound for client-supplied navigation/wait timeouts (MCP and control callers).
pub const MAX_CLIENT_TIMEOUT_MS: u64 = 120_000;
/// Default time `glide_scroll` keeps pumping page scripts after a scroll (scroll listeners,
/// IntersectionObserver callbacks and the `fetch` they start) before it answers.
pub const DEFAULT_SCROLL_SETTLE_MS: u64 = 3_000;

/// Resolve an optional client timeout to milliseconds, applying [`DEFAULT_NAV_TIMEOUT_MS`]
/// when absent and capping at [`MAX_CLIENT_TIMEOUT_MS`].
pub fn clamp_client_timeout_ms(timeout_ms: Option<u64>) -> u64 {
    let ms = timeout_ms.unwrap_or(DEFAULT_NAV_TIMEOUT_MS);
    ms.min(MAX_CLIENT_TIMEOUT_MS)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    Screenshot(ScreenshotParams),
    ComputedStyle(ComputedStyleParams),
    Scroll(ScrollParams),
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

/// Arguments for `glide_screenshot`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default, JsonSchema)]
pub struct ScreenshotParams {
    /// When true, capture the whole document height (capped at 16384 px) instead of the
    /// viewport at the current scroll offset. Default false.
    #[serde(default)]
    pub full_page: bool,
}

/// `Screenshot` used to be a unit variant, so a request with no `params` at all must still
/// parse: an absent adjacently-tagged content only deserializes as an `Option`.
impl<'de> Deserialize<'de> for ScreenshotParams {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(default)]
            full_page: bool,
        }
        let fields = Option::<Fields>::deserialize(deserializer)?;
        Ok(Self {
            full_page: fields.is_some_and(|f| f.full_page),
        })
    }
}

/// Arguments for `glide_scroll`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, JsonSchema)]
pub struct ScrollParams {
    /// Horizontal scroll delta in CSS pixels (positive scrolls right).
    #[serde(default)]
    pub dx: Option<f64>,
    /// Vertical scroll delta in CSS pixels (positive scrolls down).
    #[serde(default)]
    pub dy: Option<f64>,
    /// CSS selector of an element to scroll into view first (top-aligned); a delta, when
    /// given, is applied after it.
    #[serde(default)]
    pub to_selector: Option<String>,
    /// How long to let page scripts react after the scroll, in milliseconds (default 3000;
    /// clamped to 120000).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Viewport position and sizes after a scroll, in CSS pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScrollInfo {
    pub x: f64,
    pub y: f64,
    pub scroll_width: f64,
    pub scroll_height: f64,
    pub viewport_width: u32,
    pub viewport_height: u32,
    /// False when the page was still busy (settle timed out or the flush gave up) when the
    /// result was taken, so scroll-triggered content may still be arriving.
    pub settled: bool,
}

/// Arguments for `glide_computed_style`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ComputedStyleParams {
    /// CSS selector for the target element (first match).
    pub selector: String,
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
    ComputedStyle(std::collections::BTreeMap<String, String>),
    Scroll(ScrollInfo),
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

    // `Screenshot` used to be a unit variant: a bare request without params must still parse.
    #[test]
    fn bare_screenshot_request_without_params_still_deserializes() {
        let req: ControlRequest =
            serde_json::from_str(r#"{"method":"screenshot"}"#).expect("bare screenshot");
        assert_eq!(req, ControlRequest::Screenshot(ScreenshotParams::default()));
        let full: ControlRequest =
            serde_json::from_str(r#"{"method":"screenshot","params":{"full_page":true}}"#)
                .expect("full-page screenshot");
        assert_eq!(
            full,
            ControlRequest::Screenshot(ScreenshotParams { full_page: true })
        );
    }

    #[test]
    fn scroll_request_json_roundtrip_with_optional_fields() {
        let req = ControlRequest::Scroll(ScrollParams {
            dx: None,
            dy: Some(120.5),
            to_selector: Some("#end".into()),
            timeout_ms: None,
        });
        let json = serde_json::to_string(&req).expect("serialize");
        assert!(json.contains("\"method\":\"scroll\""), "{json}");
        let back: ControlRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, req);
        let sparse: ScrollParams = serde_json::from_str("{\"dy\": 5}").expect("sparse");
        assert_eq!(sparse.dy, Some(5.0));
        assert_eq!(sparse.to_selector, None);
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
