//! In-process tests for the headless control session (F-066).

use std::path::PathBuf;
use std::time::Duration;

use glide::control::ControlBackend;
use glide::control::protocol::{
    ControlRequest, ControlResult, DomParams, NavigateParams, QueryParams, TextParams, WaitParams,
};
use glide::control::session::{SessionOptions, spawn_session};
use glide::tokio_runtime;

fn fixture_url(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    format!("file://{}", path.display())
}

fn missing_file_url() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join("no-such-mcp-page.html");
    format!("file://{}", path.display())
}

fn session_js_on() -> impl ControlBackend {
    let rt = tokio_runtime();
    spawn_session(SessionOptions { js_enabled: true }, rt.handle().clone()).expect("spawn session")
}

fn session_js_off() -> impl ControlBackend {
    let rt = tokio_runtime();
    spawn_session(SessionOptions { js_enabled: false }, rt.handle().clone()).expect("spawn session")
}

fn call<B: ControlBackend>(backend: &B, request: ControlRequest) -> Result<ControlResult, String> {
    backend.call(request, Duration::from_secs(30))
}

fn dom_run_text<B: ControlBackend>(backend: &B) -> String {
    let ControlResult::Html { html, .. } = call(
        backend,
        ControlRequest::Dom(DomParams {
            selector: Some("#run".into()),
            all: false,
            max_chars: None,
        }),
    )
    .expect("dom #run") else {
        panic!("expected html");
    };
    html
}

#[test]
fn navigate_reports_url_title_and_settled() {
    let backend = session_js_on();
    let url = fixture_url("mcp-page-a.html");
    let result = call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: url.clone(),
            timeout_ms: None,
        }),
    )
    .expect("navigate");
    let ControlResult::Page(info) = result else {
        panic!("expected page info, got {result:?}");
    };
    assert!(info.url.contains("mcp-page-a.html"), "{}", info.url);
    assert_eq!(info.title, "Page A");
    assert!(!info.loading);
    assert_eq!(info.status, glide::control::protocol::LoadStatus::Settled);
    assert!(info.js_enabled);
}

#[test]
fn snapshot_text_returns_visible_text_and_respects_max_chars() {
    let backend = session_js_on();
    let url = fixture_url("mcp-page-a.html");
    call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url,
            timeout_ms: None,
        }),
    )
    .expect("navigate");
    let ControlResult::Text { text, truncated } = call(
        &backend,
        ControlRequest::SnapshotText(TextParams { max_chars: None }),
    )
    .expect("snapshot") else {
        panic!("expected text");
    };
    assert!(!truncated);
    assert!(text.contains("MCP Page A"));
    assert!(text.contains("Paragraph on page A"));

    let ControlResult::Text {
        text: short,
        truncated: cut,
    } = call(
        &backend,
        ControlRequest::SnapshotText(TextParams {
            max_chars: Some(12),
        }),
    )
    .expect("short snapshot")
    else {
        panic!("expected text");
    };
    assert!(cut);
    assert!(short.chars().count() <= 12);
}

#[test]
fn back_and_forward_walk_session_history() {
    let backend = session_js_on();
    let a = fixture_url("mcp-page-a.html");
    let b = fixture_url("mcp-page-b.html");
    call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: a,
            timeout_ms: None,
        }),
    )
    .expect("navigate a");
    call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: b,
            timeout_ms: None,
        }),
    )
    .expect("navigate b");

    let ControlResult::Page(back) =
        call(&backend, ControlRequest::Back(WaitParams::default())).expect("back")
    else {
        panic!("expected page");
    };
    assert_eq!(back.title, "Page A");

    let ControlResult::Page(fwd) =
        call(&backend, ControlRequest::Forward(WaitParams::default())).expect("forward")
    else {
        panic!("expected page");
    };
    assert_eq!(fwd.title, "Page B");
}

#[test]
fn back_at_first_page_is_error() {
    let backend = session_js_on();
    call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: fixture_url("mcp-page-a.html"),
            timeout_ms: None,
        }),
    )
    .expect("navigate");
    let err = call(&backend, ControlRequest::Back(WaitParams::default())).expect_err("back");
    assert!(err.contains("no previous page"), "{err}");
}

#[test]
fn reload_reruns_inline_script() {
    let backend = session_js_on();
    call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: fixture_url("mcp-page-a.html"),
            timeout_ms: None,
        }),
    )
    .expect("navigate");

    let first = dom_run_text(&backend);
    assert!(first.contains("script ran"), "{first}");
    call(&backend, ControlRequest::Reload(WaitParams::default())).expect("reload");
    let second = dom_run_text(&backend);
    assert!(second.contains("script ran"), "{second}");
    assert_ne!(first, second, "reload should change random suffix");
}

#[test]
fn dom_selector_returns_outer_html() {
    let backend = session_js_on();
    call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: fixture_url("mcp-page-a.html"),
            timeout_ms: None,
        }),
    )
    .expect("navigate");
    let ControlResult::Html {
        html,
        matched,
        truncated,
    } = call(
        &backend,
        ControlRequest::Dom(DomParams {
            selector: Some("#box".into()),
            all: false,
            max_chars: None,
        }),
    )
    .expect("dom")
    else {
        panic!("expected html");
    };
    assert_eq!(matched, 1);
    assert!(!truncated);
    assert!(html.contains("id=\"box\"") || html.contains("id=box"));
    assert!(html.contains("Box"));
}

#[test]
fn dom_missing_selector_is_error() {
    let backend = session_js_on();
    call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: fixture_url("mcp-page-a.html"),
            timeout_ms: None,
        }),
    )
    .expect("navigate");
    let err = call(
        &backend,
        ControlRequest::Dom(DomParams {
            selector: Some("#missing".into()),
            all: false,
            max_chars: None,
        }),
    )
    .expect_err("dom");
    assert!(err.contains("no element matches selector"), "{err}");
}

#[test]
fn query_returns_element_rects_and_visibility() {
    let backend = session_js_on();
    call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: fixture_url("mcp-page-a.html"),
            timeout_ms: None,
        }),
    )
    .expect("navigate");
    let ControlResult::Elements(elements) = call(
        &backend,
        ControlRequest::Query(QueryParams {
            selector: "#box, #hidden".into(),
            limit: None,
        }),
    )
    .expect("query") else {
        panic!("expected elements");
    };
    let box_el = elements
        .iter()
        .find(|e| e.id.as_deref() == Some("box"))
        .expect("box element");
    assert!(box_el.visible);
    assert!(
        (box_el.rect.width - 120.0).abs() < 2.0 && (box_el.rect.height - 40.0).abs() < 2.0,
        "{:?}",
        box_el.rect
    );

    let hidden = elements.iter().find(|e| e.id.as_deref() == Some("hidden"));
    if let Some(h) = hidden {
        assert!(!h.visible, "display:none should not be visible");
    }
}

#[test]
fn screenshot_is_png_with_viewport_size() {
    let backend = session_js_on();
    call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: fixture_url("mcp-page-a.html"),
            timeout_ms: None,
        }),
    )
    .expect("navigate");
    let ControlResult::Png(bytes) = call(&backend, ControlRequest::Screenshot).expect("shot")
    else {
        panic!("expected png");
    };
    assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
    let decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
    let reader = decoder.read_info().expect("png header");
    assert_eq!(reader.info().width, 1100);
    assert_eq!(reader.info().height, 760);
}

#[test]
fn missing_file_navigation_is_error_and_keeps_page() {
    let backend = session_js_on();
    call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: fixture_url("mcp-page-a.html"),
            timeout_ms: None,
        }),
    )
    .expect("navigate");
    let err = call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: missing_file_url(),
            timeout_ms: None,
        }),
    )
    .expect_err("missing file");
    assert!(!err.is_empty());
    let ControlResult::Page(info) = call(&backend, ControlRequest::PageInfo).expect("page info")
    else {
        panic!("expected page");
    };
    assert_eq!(info.title, "Page A");
}

#[test]
fn js_disabled_session_reads_static_text() {
    let backend = session_js_off();
    call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: fixture_url("mcp-page-a.html"),
            timeout_ms: None,
        }),
    )
    .expect("navigate");
    let ControlResult::Text { text, .. } = call(
        &backend,
        ControlRequest::SnapshotText(TextParams { max_chars: None }),
    )
    .expect("text") else {
        panic!("expected text");
    };
    assert!(text.contains("MCP Page A"));
    assert!(!text.contains("script ran"));
}
