//! Serialized control request and result types shared by session and MCP layers.

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
    Click(ClickParams),
    Type(TypeParams),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavigateParams {
    pub url: String,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WaitParams {
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextParams {
    #[serde(default)]
    pub max_chars: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomParams {
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub all: bool,
    #[serde(default)]
    pub max_chars: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryParams {
    pub selector: String,
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
    Input(InputOutcome),
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClickParams {
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub x: Option<f32>,
    #[serde(default)]
    pub y: Option<f32>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

// `ControlRequest` derives `Eq`; a click point is a float pair, so provide the marker
// implementation that the derive cannot produce for `f32`.
impl Eq for ClickParams {}

impl ClickParams {
    /// Exactly one of a selector or both x and y must be supplied.
    pub fn validate(&self) -> Result<(), String> {
        let has_selector = self
            .selector
            .as_deref()
            .is_some_and(|selector| !selector.trim().is_empty());
        let has_point = self.x.is_some() && self.y.is_some();
        if has_selector != has_point {
            Ok(())
        } else {
            Err("give a selector or both x and y".to_string())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeParams {
    pub text: String,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub submit: bool,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputOutcome {
    pub x: f32,
    pub y: f32,
    pub target: Option<ElementInfo>,
    pub navigated: bool,
    pub page: PageInfo,
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

    #[test]
    fn click_params_require_selector_or_point() {
        let err = ClickParams {
            selector: None,
            x: None,
            y: None,
            timeout_ms: None,
        }
        .validate()
        .expect_err("neither");
        assert_eq!(err, "give a selector or both x and y");

        let err = ClickParams {
            selector: None,
            x: Some(10.0),
            y: None,
            timeout_ms: None,
        }
        .validate()
        .expect_err("half a point");
        assert_eq!(err, "give a selector or both x and y");

        let err = ClickParams {
            selector: Some("#x".into()),
            x: Some(10.0),
            y: Some(20.0),
            timeout_ms: None,
        }
        .validate()
        .expect_err("selector and point");
        assert_eq!(err, "give a selector or both x and y");

        ClickParams {
            selector: Some("#x".into()),
            x: None,
            y: None,
            timeout_ms: None,
        }
        .validate()
        .expect("selector");

        ClickParams {
            selector: None,
            x: Some(10.0),
            y: Some(20.0),
            timeout_ms: None,
        }
        .validate()
        .expect("point");
    }
}
