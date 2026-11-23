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
fn late_settimeout_mutation_visible_in_dump() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("dom-repro/late-timer-mutation.html"), true).expect("load");
    assert!(text.contains("LATE-300"), "{text}");
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
fn custom_element_upgrade_fixture() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("custom-element-upgrade.html"), true).expect("load");
    assert!(text.contains("connected"), "{text}");
    assert!(text.contains("attr:msg:1"), "{text}");
    assert!(text.contains(";visible"), "{text}");
    assert!(text.contains("disconnected"), "{text}");
    assert!(text.contains("hello"), "{text}");
}

#[test]
fn observers_and_idle_work() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("observers.html"), true).expect("load");
    assert!(text.contains("OBSERVERS-OK"), "{text}");
    assert!(!text.contains("OBSERVERS-FAIL"), "{text}");
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

#[test]
fn headless_post_form_navigation_submits_urlencoded_body() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    let posted = Arc::new(Mutex::new(String::new()));
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let posted2 = posted.clone();
    std::thread::spawn(move || {
        for _ in 0..4 {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).expect("read");
            let req = String::from_utf8_lossy(&buf[..n]);
            let (status, body, extra) = if req.starts_with("GET /form") {
                (
                    "200 OK",
                    "<!DOCTYPE html><html><body>\
<form id=f action=\"/done\" method=\"post\"><input name=q value=\"hi\"></form>\
<script>document.getElementById('f').requestSubmit()</script>\
</body></html>",
                    "Content-Type: text/html; charset=utf-8\r\n",
                )
            } else if req.starts_with("POST /done") {
                *posted2.lock().expect("lock") =
                    req.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
                (
                    "200 OK",
                    "<!DOCTYPE html><html><body><p>posted ok</p></body></html>",
                    "Content-Type: text/html; charset=utf-8\r\n",
                )
            } else {
                ("404 Not Found", "", "")
            };
            let resp = format!(
                "HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let url = format!("http://127.0.0.1:{port}/form");
    let text = dump_text(&url, true).expect("load");
    assert!(text.contains("posted ok"), "{text}");
    assert_eq!(posted.lock().expect("lock").as_str(), "q=hi");
}

#[test]
fn headless_post_checkbox_checked_sends_on() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    let posted = Arc::new(Mutex::new(String::new()));
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let posted2 = posted.clone();
    std::thread::spawn(move || {
        for _ in 0..4 {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).expect("read");
            let req = String::from_utf8_lossy(&buf[..n]);
            let (status, body, extra) = if req.starts_with("GET /form") {
                (
                    "200 OK",
                    "<!DOCTYPE html><html><body>\
<form id=f action=\"/done\" method=\"post\">\
<input type=\"checkbox\" name=\"agree\" value=\"on\" checked=\"\">\
<input type=\"submit\"></form>\
<script>document.getElementById('f').requestSubmit();</script>\
</body></html>",
                    "Content-Type: text/html; charset=utf-8\r\n",
                )
            } else if req.starts_with("POST /done") {
                *posted2.lock().expect("lock") =
                    req.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
                (
                    "200 OK",
                    "<!DOCTYPE html><html><body><p>posted ok</p></body></html>",
                    "Content-Type: text/html; charset=utf-8\r\n",
                )
            } else {
                ("404 Not Found", "", "")
            };
            let resp = format!(
                "HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let url = format!("http://127.0.0.1:{port}/form");
    let text = dump_text(&url, true).expect("load");
    assert!(text.contains("posted ok"), "{text}");
    assert_eq!(posted.lock().expect("lock").as_str(), "agree=on");
}

#[test]
fn page_script_navigation_is_followed() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("page-navigate.html"), true).expect("load");
    assert!(text.contains("NAVIGATED ?from=source"), "{text}");
    assert!(!text.contains("start"), "{text}");
}

#[test]
fn inline_module_script_runs() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("module-script.html"), true).expect("load");
    assert!(text.contains("module ran"), "{text}");
}

#[test]
fn external_module_script_runs() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("module-external.html"), true).expect("load");
    assert!(text.contains("external module ran"), "{text}");
}

#[test]
fn module_import_resolves_relative_dependency() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("module-import-ok.html"), true).expect("load");
    assert!(text.contains("imported 7"), "{text}");
}

#[test]
fn module_import_missing_dependency_leaves_page_intact() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("module-import-missing.html"), true).expect("load");
    assert!(text.contains("before"), "{text}");
    assert!(!text.contains("imported"), "{text}");
}

#[test]
fn module_graph_handles_import_cycle() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("module-cycle.html"), true).expect("load");
    assert!(text.contains("cycle ok"), "{text}");
}

#[test]
fn dynamic_import_resolves_module() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("module-dynamic.html"), true).expect("load");
    assert!(text.contains("dynamic ok"), "{text}");
}

#[test]
fn import_meta_url_matches_module_resource() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let dep = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/module-meta-dep.js");
    let expected = format!("file://{}", dep.display());
    let text = dump_text(&fixture_url("module-meta.html"), true).expect("load");
    assert!(text.contains(&expected), "{text}");
}

#[test]
fn bare_module_specifier_is_rejected() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("module-bare.html"), true).expect("load");
    assert!(text.contains("before"), "{text}");
    assert!(!text.contains("bare ran"), "{text}");
}

#[test]
fn http_module_script_fetch_cookie_same_origin_and_not_cross_port() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    const MODULE: &str = "export const msg = 'cookie-module';";
    let listener_a = TcpListener::bind("127.0.0.1:0").expect("bind a");
    let port_a = listener_a.local_addr().expect("a").port();
    let listener_b = TcpListener::bind("127.0.0.1:0").expect("bind b");
    let port_b = listener_b.local_addr().expect("b").port();
    let seen = Arc::new(Mutex::new(Vec::<(u16, String)>::new()));

    let serve = |listener: TcpListener, port: u16, seen: Arc<Mutex<Vec<(u16, String)>>>| {
        thread::spawn(move || {
            for _ in 0..6 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut buf = [0u8; 4096];
                let Ok(n) = stream.read(&mut buf) else {
                    continue;
                };
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req.split_whitespace().nth(1).unwrap_or("");
                let cookie = req
                    .lines()
                    .find(|l| l.to_ascii_lowercase().starts_with("cookie:"))
                    .map(str::trim)
                    .unwrap_or("")
                    .to_string();
                if path.ends_with(".js") {
                    seen.lock().expect("lock").push((port, cookie));
                }
                let (ctype, body, extra) = if path.ends_with("index.html") {
                    (
                        "text/html",
                        format!(
                            r#"<!DOCTYPE html><html><body><p id="t">x</p>
<script type="module" src="http://127.0.0.1:{port_b}/cross.js"></script>
<script type="module">
import {{ msg }} from "http://127.0.0.1:{port_a}/same.js";
document.getElementById('t').textContent = msg;
</script></body></html>"#
                        ),
                        "Set-Cookie: jar=1; Path=/; SameSite=Lax\r\n",
                    )
                } else if path.ends_with("same.js") {
                    ("application/javascript", MODULE.to_string(), "")
                } else {
                    (
                        "application/javascript",
                        "export const msg='cross';".into(),
                        "",
                    )
                };
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
            }
        });
    };
    serve(listener_a, port_a, Arc::clone(&seen));
    serve(listener_b, port_b, Arc::clone(&seen));
    thread::sleep(Duration::from_millis(20));

    let rt = tokio_runtime();
    let _guard = rt.enter();
    let url = format!("http://127.0.0.1:{port_a}/index.html");
    let text = dump_text(&url, true).expect("load");
    assert!(text.contains("cookie-module"), "{text}");
    let hits = seen.lock().expect("lock").clone();
    let cross = hits
        .iter()
        .find(|(p, _)| *p == port_b)
        .map(|(_, c)| c.as_str())
        .unwrap_or("");
    let same = hits
        .iter()
        .find(|(p, _)| *p == port_a)
        .map(|(_, c)| c.as_str())
        .unwrap_or("");
    assert!(
        !cross.to_ascii_lowercase().contains("jar="),
        "Lax cookie must not cross port: {cross}"
    );
    assert!(
        same.to_ascii_lowercase().contains("jar=1"),
        "same-origin module fetch should send cookie: {same}"
    );
}

#[test]
fn http_module_import_resolves() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    const MODULE: &str = "export const msg = 'http module';";
    const PAGE: &str = r#"<!DOCTYPE html><html><body><p id="t">x</p>
<script type="module">
import { msg } from "./entry.js";
document.getElementById("t").textContent = msg;
</script></body></html>"#;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    thread::spawn(move || {
        for _ in 0..8 {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = [0u8; 4096];
            let Ok(n) = stream.read(&mut buf) else {
                continue;
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            let path = req.split_whitespace().nth(1).unwrap_or("");
            let (ctype, body) = if path.ends_with("entry.js") {
                ("application/javascript", MODULE)
            } else {
                ("text/html", PAGE)
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });

    let rt = tokio_runtime();
    let _guard = rt.enter();
    let url = format!("http://127.0.0.1:{port}/index.html");
    let text = dump_text(&url, true).expect("load");
    assert!(text.contains("http module"), "{text}");
}

#[test]
fn dynamically_inserted_script_runs() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("dynamic-script.html"), true).expect("load");
    assert!(text.contains("dynamic"), "{text}");
    assert!(!text.contains("after"), "{text}");
}

#[test]
fn dynamically_inserted_external_script_runs_and_fires_load() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("dynamic-script-external.html"), true).expect("load");
    assert!(text.contains("external dynamic loaded"), "{text}");
}

#[test]
fn inner_html_scripts_do_not_run() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("innerhtml-script.html"), true).expect("load");
    assert!(text.contains("before"), "{text}");
    assert!(!text.contains("inner ran"), "{text}");
}

#[test]
fn module_runs_between_parser_scripts_and_dom_content_loaded() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("module-order.html"), true).expect("load");
    assert!(text.contains("classic,module,dom"), "{text}");
}

#[test]
fn classic_script_dynamic_import_resolves_relative_module() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("classic-dynamic-import.html"), true).expect("load");
    assert!(text.contains("imported 7"), "{text}");
}

#[test]
fn classic_script_bare_dynamic_import_rejects_without_aborting() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("classic-import-bare.html"), true).expect("load");
    assert!(text.contains("after"), "{text}");
}

#[test]
fn nomodule_classic_script_is_skipped() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("nomodule-vs-module.html"), true).expect("load");
    assert!(text.contains("module"), "{text}");
    assert!(!text.contains("nomodule"), "{text}");
}

#[test]
fn module_cache_evaluates_same_url_once() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("module-cache-twice.html"), true).expect("load");
    assert!(text.contains('1'), "expected counter 1, got: {text}");
}

#[test]
fn dynamic_import_runs_after_timers_on_pump() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("import-async-order.html"), true).expect("load");
    assert!(
        text.contains("a,b,t0,imp"),
        "expected pump ordering, got: {text}"
    );
}

#[test]
fn module_imports_data_url() {
    let rt = tokio_runtime();
    let _guard = rt.enter();
    let text = dump_text(&fixture_url("module-data-import.html"), true).expect("load");
    assert!(text.contains("data-42"), "{text}");
}
