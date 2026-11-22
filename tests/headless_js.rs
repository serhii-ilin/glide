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
fn text_content_on_attached_element_mirrors() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let url = fixture_url("mirror.html");
    let text = dump_text(&url, true).expect("load");
    assert!(
        text.contains("ATTACHED-TEXT"),
        "expected mirrored text in blitz dump, got: {text}"
    );
}

fn width_after(label: &str, text: &str) -> f64 {
    let idx = text
        .find(label)
        .unwrap_or_else(|| panic!("missing {label} in {text}"));
    let rest = &text[idx + label.len()..];
    let (neg, rest) = rest
        .strip_prefix('-')
        .map(|rest| (true, rest))
        .unwrap_or((false, rest));
    let num: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let parsed: f64 = num.parse().unwrap_or_else(|_| panic!("width in {text}"));
    if neg { -parsed } else { parsed }
}

#[test]
fn document_write_inserts_after_current_script() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("dom-repro/document-write.html"), true).expect("load");
    let before = text.find("BEFORE").expect(&text);
    let written = text.find("WRITTEN-TEXT").expect(&text);
    let after = text.find("AFTER").expect(&text);
    assert!(before < written && written < after, "{text}");
    assert!(text.contains("found=true"), "{text}");
}

#[test]
fn nested_insert_matches_static_width() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let ctrl = dump_text(&fixture_url("dom-repro/ctrl.html"), true).expect("ctrl");
    let nest = dump_text(&fixture_url("dom-repro/nest.html"), true).expect("nest");
    let ctrl_w = width_after("W=", &ctrl);
    let nest_w = width_after("W=", &nest);
    assert!(
        (nest_w - ctrl_w).abs() < 2.0 && nest_w < 80.0,
        "ctrl={ctrl_w} nest={nest_w}\n{ctrl}\n{nest}"
    );
    assert!(
        nest.contains("A1")
            && nest.find("A1") < nest.find("B2")
            && nest.find("B2") < nest.find("TAIL"),
        "fragment order: {nest}"
    );
}

#[test]
fn removed_subtree_child_can_be_reinserted() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("dom-repro/move.html"), true).expect("load");
    assert!(text.contains("KID"), "{text}");
    assert!(text.contains("REINS"), "{text}");
    assert!(text.contains("CLICKED"), "{text}");
}

#[test]
fn perf100_attached_appends_stay_under_budget() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("dom-repro/perf100.html"), true).expect("load");
    let attached = width_after("attached=", &text) as u64;
    let detached = width_after("detached=", &text) as u64;
    let layout = width_after("layout200=", &text) as u64;
    assert!(
        attached < 1500 && attached + detached + layout < 1500,
        "timings attached={attached} detached={detached} layout={layout} dump={text}"
    );
}

#[test]
fn window_load_and_ready_state() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let rs = dump_text(&fixture_url("dom-repro/rs.html"), true).expect("rs");
    assert!(rs.contains("rs0=loading"), "{rs}");
    assert!(rs.contains("rsL=complete"), "{rs}");
    assert!(rs.contains("dt=html"), "{rs}");
    assert!(rs.contains("ns=http://www.w3.org/2000/svg"), "{rs}");
    let w = dump_text(&fixture_url("dom-repro/w.html"), true).expect("w");
    assert!(w.contains("wDCL") && w.contains("dDCL"), "{w}");
    assert!(w.contains("wload:true:true"), "{w}");
    assert!(w.contains("wclick"), "{w}");
    assert!(w.contains("[object Window]"), "{w}");
    assert!(w.contains("loc=file:"), "{w}");
}

#[test]
fn repeated_text_content_does_not_grow_node_count() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("dom-repro/leak.html"), true).expect("load");
    let delta = width_after("delta=", &text);
    let n0 = width_after("n0=", &text);
    assert!(n0 > 3.0, "node scan failed: {text}");
    assert!(delta <= 30.0, "node count grew: {text}");
    assert!(text.contains("last=v199"), "{text}");
}

#[test]
fn document_parent_is_null_inside_listener() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(
        &fixture_url("dom-repro/document-parent-in-listener.html"),
        true,
    )
    .expect("load");
    assert!(text.contains("pn=null"), "{text}");
    assert!(text.contains("cdp=ok"), "{text}");
    assert!(!text.contains("cdpE"), "{text}");
}

#[test]
fn async_document_write_does_not_wipe_the_page() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("dom-repro/async-write.html"), true).expect("load");
    assert!(text.contains("KEEP-ME"), "{text}");
    assert!(!text.contains("ASYNCWRITE"), "{text}");
    let defer = dump_text(&fixture_url("dom-repro/defer-write.html"), true).expect("defer");
    assert!(defer.contains("KEEP-DEFER"), "{defer}");
    assert!(!defer.contains("DEFERWRITE"), "{defer}");
}

#[test]
fn layout_cache_refreshes_on_the_next_task() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("dom-repro/layout-cache.html"), true).expect("load");
    let w1 = width_after("w1=", &text);
    let w2 = width_after("w2=", &text);
    assert!(w1 > 400.0, "first width {text}");
    assert!(w2 < w1 && w2 < 300.0, "stale layout cache: {text}");
}

#[test]
fn promise_continuation_mutates_before_settle() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("dom-repro/microtask-text.html"), true).expect("load");
    assert!(text.contains("AFTER-MICRO"), "{text}");
    assert!(!text.contains("BEFORE"), "{text}");
}

#[test]
fn onclick_attribute_calls_get_element_by_id() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("onclick-get.html"), true).expect("load");
    assert!(text.contains("got"), "{text}");
    assert!(!text.contains("old"), "{text}");
}

#[test]
fn skipped_insert_is_not_mirrored_twice() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("dom-repro/skip-insert.html"), true).expect("load");
    assert_eq!(text.matches("YONCE").count(), 1, "{text}");
}

#[test]
fn timer_tasks_reset_document_write_state() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("dom-repro/timer-write.html"), true).expect("load");
    assert!(text.contains("replaced"), "{text}");
    assert!(!text.contains("FIRST"), "{text}");
}

#[test]
fn body_onload_fixture_runs() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("body-onload.html"), true).expect("load");
    assert!(text.contains("BODY-ONLOAD-RAN"), "{text}");
}

#[test]
fn onclick_attr_fixture_runs() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("onclick-attr.html"), true).expect("load");
    assert!(text.contains("CLICKED"), "{text}");
    assert!(!text.contains("pending"), "{text}");
}

#[test]
fn network_apis_work() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("network-apis.html"), true).expect("load");
    assert!(text.contains("NETWORK-OK"), "{text}");
    assert!(!text.contains("NETWORK-FAIL"), "{text}");
}

#[test]
fn timing_history_crypto_work() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("timing-history.html"), true).expect("load");
    assert!(text.contains("TIMING-OK"), "{text}");
    assert!(!text.contains("TIMING-FAIL"), "{text}");
}

#[test]
fn navigator_languages_work() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("navigator-languages.html"), true).expect("load");
    assert!(text.contains("NAV-OK"), "{text}");
    assert!(text.contains("en-US|en-US,en"), "{text}");
}

#[test]
fn common_web_api_shims_work() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("web-api-shims.html"), true).expect("load");
    assert!(text.contains("SHIMS-OK"), "{text}");
    assert!(!text.contains("SHIMS-FAIL"), "{text}");
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
