//! Headless load and text dump integration tests (no network).

use std::path::PathBuf;

use glide::headless::{dump_text, load};
use glide::tokio_runtime;

fn fixture_url(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    format!("file://{}", path.display())
}

#[test]
fn js_on_runs_inline_script() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let url = fixture_url("js-toggle.html");
    let text = dump_text(&url, true).expect("load");
    assert!(text.contains("scripted"), "expected scripted, got: {text}");
    assert!(!text.contains("static"));
    assert!(!text.contains("no-js-fallback"));
}

#[test]
fn js_off_keeps_static_text() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let url = fixture_url("js-toggle.html");
    let text = dump_text(&url, false).expect("load");
    assert!(text.contains("static"), "expected static, got: {text}");
    assert!(text.contains("no-js-fallback"));
    assert!(!text.contains("scripted"));
}

#[test]
fn counter_fixture_runs_with_js() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let url = fixture_url("counter.html");
    let doc = load(&url, true).expect("load");
    let count_id = doc.0.get_element_by_id("count").expect("count");
    let text = doc
        .0
        .get_node(count_id)
        .map(|n| n.text_content())
        .unwrap_or_default();
    assert_eq!(text.trim(), "ready");
}
