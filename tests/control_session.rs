//! In-process tests for the headless control session (F-066).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use glide::control::ControlBackend;
use glide::control::protocol::{
    ClickParams, ControlRequest, ControlResult, DomParams, InputOutcome, NavigateParams,
    QueryParams, TextParams, TypeParams, WaitParams,
};
use glide::control::session::{SessionOptions, spawn_session};

static TOKIO_RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn runtime() -> &'static tokio::runtime::Runtime {
    TOKIO_RT.get_or_init(glide::tokio_runtime)
}

fn session_js_on() -> impl ControlBackend {
    spawn_session(
        SessionOptions { js_enabled: true },
        runtime().handle().clone(),
    )
    .expect("spawn session")
}

fn session_js_off() -> impl ControlBackend {
    spawn_session(
        SessionOptions { js_enabled: false },
        runtime().handle().clone(),
    )
    .expect("spawn session")
}

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

fn call<B: ControlBackend>(backend: &B, request: ControlRequest) -> Result<ControlResult, String> {
    backend.call(request, Duration::from_secs(30))
}

fn page_info<B: ControlBackend>(backend: &B) -> glide::control::protocol::PageInfo {
    let ControlResult::Page(info) = call(backend, ControlRequest::PageInfo).expect("page_info")
    else {
        panic!("expected page");
    };
    info
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

fn copy_fixture_tree(dest: &Path) -> std::io::Result<()> {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for name in ["mcp-page-a.html", "mcp-page-b.html"] {
        std::fs::copy(src.join(name), dest.join(name))?;
    }
    Ok(())
}

fn file_url(path: &Path) -> String {
    url::Url::from_file_path(path)
        .expect("file url")
        .to_string()
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
fn session_navigates_data_url_via_tokio_provider() {
    let backend = session_js_on();
    let url = "data:text/html,<!DOCTYPE html><html><head><title>Data</title></head><body><p>DATA-OK</p></body></html>";
    let ControlResult::Page(info) = call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: url.into(),
            timeout_ms: None,
        }),
    )
    .expect("navigate data") else {
        panic!("expected page");
    };
    assert_eq!(info.title, "Data");
    let ControlResult::Text { text, .. } = call(
        &backend,
        ControlRequest::SnapshotText(TextParams { max_chars: None }),
    )
    .expect("text") else {
        panic!("expected text");
    };
    assert!(text.contains("DATA-OK"));
}

#[test]
fn js_navigation_completes_under_four_seconds() {
    let backend = session_js_on();
    let start = Instant::now();
    call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: fixture_url("mcp-page-a.html"),
            timeout_ms: None,
        }),
    )
    .expect("navigate");
    assert!(
        start.elapsed() < Duration::from_secs(4),
        "navigation took {:?}",
        start.elapsed()
    );
}

#[test]
fn short_timeout_nav_reports_settled() {
    let backend = session_js_on();
    let ControlResult::Page(info) = call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: fixture_url("mcp-page-a.html"),
            timeout_ms: Some(800),
        }),
    )
    .expect("navigate") else {
        panic!("expected page");
    };
    assert_eq!(info.status, glide::control::protocol::LoadStatus::Settled);
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
            url: a.clone(),
            timeout_ms: None,
        }),
    )
    .expect("navigate a");
    call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: b.clone(),
            timeout_ms: None,
        }),
    )
    .expect("navigate b");

    let back = page_info(&backend);
    assert!(back.url.contains("mcp-page-b.html"));
    assert!(back.can_go_back);
    assert!(!back.can_go_forward);

    let ControlResult::Page(back) =
        call(&backend, ControlRequest::Back(WaitParams::default())).expect("back")
    else {
        panic!("expected page");
    };
    assert_eq!(back.title, "Page A");
    assert!(back.url.contains("mcp-page-a.html"));
    assert!(!back.can_go_back);
    assert!(back.can_go_forward);

    let ControlResult::Page(fwd) =
        call(&backend, ControlRequest::Forward(WaitParams::default())).expect("forward")
    else {
        panic!("expected page");
    };
    assert_eq!(fwd.title, "Page B");
    assert!(fwd.url.contains("mcp-page-b.html"));
    assert!(fwd.can_go_back);
    assert!(!fwd.can_go_forward);

    let err = call(&backend, ControlRequest::Forward(WaitParams::default())).expect_err("fwd end");
    assert!(err.contains("no next page"), "{err}");

    call(&backend, ControlRequest::Back(WaitParams::default())).expect("back before c");
    call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: b,
            timeout_ms: None,
        }),
    )
    .expect("navigate c truncates forward");
    let after = page_info(&backend);
    assert!(!after.can_go_forward);
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

    let hidden = elements
        .iter()
        .find(|e| e.id.as_deref() == Some("hidden"))
        .expect("hidden element present");
    assert!(!hidden.visible);
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
    let good = fixture_url("mcp-page-a.html");
    call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: good.clone(),
            timeout_ms: None,
        }),
    )
    .expect("navigate");
    let before = page_info(&backend);
    let err = call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: missing_file_url(),
            timeout_ms: None,
        }),
    )
    .expect_err("missing file");
    assert!(!err.is_empty());
    let after = page_info(&backend);
    assert_eq!(after.title, "Page A");
    assert_eq!(after.url, before.url);
    assert_eq!(after.can_go_back, before.can_go_back);
    assert_eq!(after.can_go_forward, before.can_go_forward);
}

#[test]
fn failed_back_leaves_history_and_page_untouched() {
    let dir = std::env::temp_dir().join(format!("glide-control-back-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    copy_fixture_tree(&dir).expect("copy fixtures");

    let url_a = file_url(&dir.join("mcp-page-a.html"));
    let url_b = file_url(&dir.join("mcp-page-b.html"));
    let url_c =
        file_url(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mcp-page-a.html"));

    let backend = session_js_on();
    call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: url_a,
            timeout_ms: None,
        }),
    )
    .expect("navigate a");
    call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: url_b.clone(),
            timeout_ms: None,
        }),
    )
    .expect("navigate b");

    std::fs::remove_file(dir.join("mcp-page-a.html")).expect("delete a");

    let err = call(&backend, ControlRequest::Back(WaitParams::default())).expect_err("back");
    assert!(!err.is_empty(), "{err}");

    let info = page_info(&backend);
    assert!(info.url.contains("mcp-page-b.html"), "{}", info.url);
    assert_eq!(info.title, "Page B");
    assert!(info.can_go_back);
    assert!(!info.can_go_forward);

    call(
        &backend,
        ControlRequest::Navigate(NavigateParams {
            url: url_c,
            timeout_ms: None,
        }),
    )
    .expect("navigate c");

    let ControlResult::Page(back_to_b) =
        call(&backend, ControlRequest::Back(WaitParams::default())).expect("back to b")
    else {
        panic!("expected page");
    };
    assert_eq!(back_to_b.title, "Page B");

    let _ = std::fs::remove_dir_all(&dir);
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

fn load_form<B: ControlBackend>(backend: &B) {
    call(
        backend,
        ControlRequest::Navigate(NavigateParams {
            url: fixture_url("mcp-form.html"),
            timeout_ms: None,
        }),
    )
    .expect("navigate form");
}

fn input_outcome<B: ControlBackend>(
    backend: &B,
    request: ControlRequest,
) -> Result<InputOutcome, String> {
    match call(backend, request)? {
        ControlResult::Input(outcome) => Ok(outcome),
        other => panic!("expected input outcome, got {other:?}"),
    }
}

fn element_html<B: ControlBackend>(backend: &B, selector: &str) -> String {
    let ControlResult::Html { html, .. } = call(
        backend,
        ControlRequest::Dom(DomParams {
            selector: Some(selector.into()),
            all: false,
            max_chars: None,
        }),
    )
    .expect("dom") else {
        panic!("expected html");
    };
    html
}

fn button_click_params<B: ControlBackend>(backend: &B) -> ClickParams {
    let ControlResult::Elements(elements) = call(
        backend,
        ControlRequest::Query(QueryParams {
            selector: "#btn".into(),
            limit: None,
        }),
    )
    .expect("query button") else {
        panic!("expected elements");
    };
    let button = elements.into_iter().next().expect("button");
    assert!(button.visible, "{:?}", button.rect);
    ClickParams {
        selector: None,
        x: Some(button.rect.x + button.rect.width / 2.0),
        y: Some(button.rect.y + button.rect.height / 2.0),
        timeout_ms: None,
    }
}

#[test]
fn click_selector_runs_onclick() {
    let backend = session_js_on();
    load_form(&backend);
    let outcome = input_outcome(
        &backend,
        ControlRequest::Click(ClickParams {
            selector: Some("#btn".into()),
            x: None,
            y: None,
            timeout_ms: None,
        }),
    )
    .expect("click");
    let target = outcome.target.expect("target");
    assert_eq!(target.id.as_deref(), Some("btn"));
    assert!(target.visible);
    assert!(!outcome.navigated);
    assert_eq!(outcome.page.title, "Form");
    assert!(element_html(&backend, "#out").contains("clicked"));
}

#[test]
fn click_coordinates_hit_the_button() {
    let backend = session_js_on();
    load_form(&backend);
    let params = button_click_params(&backend);
    let outcome =
        input_outcome(&backend, ControlRequest::Click(params)).expect("click coordinates");
    assert!(!outcome.navigated);
    assert_eq!(
        outcome.target.expect("target").id.as_deref(),
        Some("btn"),
        "coordinate clicks should resolve the hit element"
    );
    assert!(element_html(&backend, "#out").contains("clicked"));
}

#[test]
fn click_coordinates_ignore_blank_selector() {
    let backend = session_js_on();
    load_form(&backend);
    let mut params = button_click_params(&backend);
    params.selector = Some("   ".into());
    let outcome = input_outcome(&backend, ControlRequest::Click(params)).expect("click");
    assert!(!outcome.navigated);
    assert!(element_html(&backend, "#out").contains("clicked"));
}

#[test]
fn click_link_navigates() {
    let backend = session_js_on();
    load_form(&backend);
    let outcome = input_outcome(
        &backend,
        ControlRequest::Click(ClickParams {
            selector: Some("#link".into()),
            x: None,
            y: None,
            timeout_ms: None,
        }),
    )
    .expect("click link");
    assert!(outcome.navigated, "link click should navigate");
    assert_eq!(outcome.page.title, "Page B");
    assert!(outcome.page.url.contains("mcp-page-b.html"));
    assert!(outcome.page.can_go_back);
}

#[test]
fn click_plain_inline_link_navigates() {
    let backend = session_js_on();
    load_form(&backend);
    let outcome = input_outcome(
        &backend,
        ControlRequest::Click(ClickParams {
            selector: Some("#inline".into()),
            x: None,
            y: None,
            timeout_ms: None,
        }),
    )
    .expect("click inline link");
    assert!(outcome.navigated, "inline link click should navigate");
    assert_eq!(outcome.page.title, "Page B");
}

#[test]
fn click_wrapped_link_navigates() {
    let backend = session_js_on();
    load_form(&backend);
    let outcome = input_outcome(
        &backend,
        ControlRequest::Click(ClickParams {
            selector: Some("#wrapped".into()),
            x: None,
            y: None,
            timeout_ms: None,
        }),
    )
    .expect("click wrapped link");
    assert!(outcome.navigated, "wrapped link click should navigate");
    assert_eq!(outcome.page.title, "Page B");
}

#[test]
fn click_javascript_link_runs_onclick_without_error() {
    let backend = session_js_on();
    load_form(&backend);
    let outcome = input_outcome(
        &backend,
        ControlRequest::Click(ClickParams {
            selector: Some("#js-link".into()),
            x: None,
            y: None,
            timeout_ms: None,
        }),
    )
    .expect("javascript link click");
    assert!(!outcome.navigated);
    assert!(element_html(&backend, "#out").contains("js clicked"));
}

#[test]
fn click_mailto_link_is_ignored() {
    let backend = session_js_on();
    load_form(&backend);
    let outcome = input_outcome(
        &backend,
        ControlRequest::Click(ClickParams {
            selector: Some("#mailto-link".into()),
            x: None,
            y: None,
            timeout_ms: None,
        }),
    )
    .expect("mailto link click");
    assert!(!outcome.navigated);
    assert_eq!(outcome.page.title, "Form");
}

#[test]
fn click_link_to_missing_file_reports_clicked() {
    let backend = session_js_on();
    load_form(&backend);
    let err = input_outcome(
        &backend,
        ControlRequest::Click(ClickParams {
            selector: Some("#missing-link".into()),
            x: None,
            y: None,
            timeout_ms: None,
        }),
    )
    .expect_err("missing link");
    assert!(err.starts_with("clicked; navigation to "), "{err}");
    assert!(err.contains("failed"), "{err}");
}

#[test]
fn click_covered_element_is_error() {
    let backend = session_js_on();
    load_form(&backend);
    let err = input_outcome(
        &backend,
        ControlRequest::Click(ClickParams {
            selector: Some("#covered".into()),
            x: None,
            y: None,
            timeout_ms: None,
        }),
    )
    .expect_err("covered element");
    assert!(err.contains("is covered by"), "{err}");
    assert!(err.contains("overlay"), "{err}");
}

#[test]
fn type_text_updates_input_listener() {
    let backend = session_js_on();
    load_form(&backend);
    let outcome = input_outcome(
        &backend,
        ControlRequest::Type(TypeParams {
            text: "Alice".into(),
            selector: Some("#name".into()),
            submit: false,
            timeout_ms: None,
        }),
    )
    .expect("type");
    assert!(!outcome.navigated);
    assert_eq!(outcome.page.title, "Form");
    assert_eq!(outcome.target.expect("target").id.as_deref(), Some("name"));
    // The host dispatches the event type only; page JS does not see the typed value
    // until F-039 mirrors form control values from Blitz into domino.
    let echo = element_html(&backend, "#echo");
    assert!(
        echo.contains(">input</p>"),
        "input listener did not run: {echo}"
    );
}

#[test]
fn type_submit_presses_enter() {
    let backend = session_js_on();
    load_form(&backend);
    let outcome = input_outcome(
        &backend,
        ControlRequest::Type(TypeParams {
            text: "query".into(),
            selector: Some("#q".into()),
            submit: true,
            timeout_ms: None,
        }),
    )
    .expect("type submit");
    assert!(outcome.navigated, "data: form submission should navigate");
    assert_eq!(outcome.page.title, "Submitted");
    assert!(
        outcome.page.url.contains("q=query"),
        "form submission should use the typed value: {}",
        outcome.page.url
    );
}

#[test]
fn type_submit_file_form_does_not_navigate() {
    let backend = session_js_on();
    load_form(&backend);
    let outcome = input_outcome(
        &backend,
        ControlRequest::Type(TypeParams {
            text: "query".into(),
            selector: Some("#fileq".into()),
            submit: true,
            timeout_ms: None,
        }),
    )
    .expect("type submit file form");
    assert!(!outcome.navigated);
    assert_eq!(outcome.page.title, "Form");
    let key = element_html(&backend, "#filekey");
    assert!(
        key.contains(">keydown</p>"),
        "keydown did not reach the page: {key}"
    );
}

#[test]
fn type_into_button_is_error() {
    let backend = session_js_on();
    load_form(&backend);
    let err = input_outcome(
        &backend,
        ControlRequest::Type(TypeParams {
            text: "nope".into(),
            selector: Some("#btn".into()),
            submit: false,
            timeout_ms: None,
        }),
    )
    .expect_err("type into button");
    assert_eq!(err, "element `#btn` is not a text field");
}

#[test]
fn type_without_focus_is_error() {
    let backend = session_js_on();
    load_form(&backend);
    let err = input_outcome(
        &backend,
        ControlRequest::Type(TypeParams {
            text: "nope".into(),
            selector: None,
            submit: false,
            timeout_ms: None,
        }),
    )
    .expect_err("type without focus");
    assert_eq!(err, "no focused text field; pass a selector");
}

#[test]
fn click_unknown_selector_is_error() {
    let backend = session_js_on();
    load_form(&backend);
    let err = input_outcome(
        &backend,
        ControlRequest::Click(ClickParams {
            selector: Some("#nope".into()),
            x: None,
            y: None,
            timeout_ms: None,
        }),
    )
    .expect_err("unknown selector");
    assert!(err.contains("no element matches selector"), "{err}");
}

#[test]
fn click_hidden_element_is_error() {
    let backend = session_js_on();
    load_form(&backend);
    let err = input_outcome(
        &backend,
        ControlRequest::Click(ClickParams {
            selector: Some("#hidden".into()),
            x: None,
            y: None,
            timeout_ms: None,
        }),
    )
    .expect_err("hidden element");
    assert!(err.contains("is not visible"), "{err}");
}

#[test]
fn click_without_target_is_error() {
    let backend = session_js_on();
    load_form(&backend);
    let err = input_outcome(
        &backend,
        ControlRequest::Click(ClickParams {
            selector: None,
            x: None,
            y: None,
            timeout_ms: None,
        }),
    )
    .expect_err("no target");
    assert_eq!(err, "give a selector or both x and y");
}
