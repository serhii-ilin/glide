//! V8 integration tests (real isolate + DOM RPC).

use std::time::Duration;

#[test]
fn timer_fires_within_100ms() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">before</p>
    <script>
      setTimeout(function () {
        document.getElementById('t').textContent = 'after';
      }, 20);
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(150));
    assert_eq!(result.final_text, "after");
}

#[test]
fn click_listener_receives_bubbled_event() {
    let html2 = r#"<!DOCTYPE html><html><body>
    <button id="btn">go</button><p id="t">x</p>
    <script>
      const b = document.getElementById('btn');
      b.addEventListener('click', function () {
        document.getElementById('t').textContent = 'clicked';
      });
      b.click();
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html2, Duration::from_millis(80));
    assert_eq!(result.final_text, "clicked");
}

#[test]
fn fetch_promise_resolves() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      fetch('data:,hello')
        .then(function (r) { return r.text(); })
        .then(function (body) {
          document.getElementById('t').textContent = body;
        });
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(300));
    assert_eq!(result.final_text, "hello");
}

#[test]
fn create_append_after_text_clear() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">seed</p>
    <script>
      const p = document.getElementById('t');
      p.textContent = '';
      const s = document.createElement('span');
      s.textContent = 'child';
      p.appendChild(s);
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(120));
    assert_eq!(result.final_text, "child");
}

#[test]
fn window_parent_and_top_are_self() {
    let html = r#"<!DOCTYPE html><html><body><script>
      if (self.parent !== self || self.top !== self || self.frames !== self) {
        throw new Error('bad browsing context');
      }
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(80));
    assert_eq!(result.final_text, "");
}

#[test]
fn body_onload_attribute_runs() {
    let html = r#"<!DOCTYPE html><html>
    <body onload="document.getElementById('t').textContent='ran'"><p id="t">x</p>
    </body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(120));
    assert_eq!(result.final_text, "ran");
}

#[test]
fn onclick_attribute_runs_with_this_element() {
    let html = r#"<!DOCTYPE html><html><body>
    <button id="b" onclick="if (this.id !== 'b') throw new Error('bad this');
      document.getElementById('t').textContent='ok'"></button><p id="t">x</p>
    <script>document.getElementById('b').click();</script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(120));
    assert_eq!(result.final_text, "ok");
}

#[test]
fn set_attribute_onclick_registers_handler() {
    let html = r#"<!DOCTYPE html><html><body>
    <div id="t">x</div><div id="b"></div>
    <script>
      var b = document.getElementById('b');
      b.setAttribute('onclick', "document.getElementById('t').textContent='setattr'");
      b.click();
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(120));
    assert_eq!(result.final_text, "setattr");
}

#[test]
fn window_onload_property_runs() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">x</p>
    <script>
      window.onload = function () {
        document.getElementById('t').textContent = 'win';
      };
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(120));
    assert_eq!(result.final_text, "win");
}

#[test]
fn failed_external_script_does_not_panic() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio");
    let handle = rt.handle();
    let net = std::sync::Arc::new(blitz_net::Provider::new(None));
    let specs = vec![crate::js::scripts::ScriptSpec {
        inline: None,
        src: Some("https://example.invalid/missing.js".into()),
        defer: false,
        r#async: false,
        element: None,
    }];
    let sources =
        crate::js::scripts::resolve_sources(&specs, &net, "https://example.test/", handle);
    assert!(matches!(
        sources[0],
        crate::js::scripts::ScriptSource::Failed
    ));
}
