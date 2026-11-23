//! V8 integration tests (real isolate + DOM RPC).

use std::sync::Arc;
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
fn xhr_sync_and_async_requests_work() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var log = [];
      var s = new XMLHttpRequest();
      s.open('GET', 'data:,sync-body', false);
      s.setRequestHeader('X-Test', 'echo');
      s.send();
      log.push('sync:' + s.readyState + ':' + s.status + ':' + s.responseText);
      log.push('hdr:' + s.getResponseHeader('x-test'));
      log.push('ctype:' + s.getResponseHeader('content-type'));
      var a = new XMLHttpRequest();
      var states = [];
      a.onreadystatechange = function () {
        if (a.readyState >= 2) states.push(a.readyState);
      };
      a.open('GET', 'data:,async-body', true);
      a.onload = function () {
        log.push('async:' + a.responseText);
        log.push('states:' + states.join(','));
        document.getElementById('t').textContent = log.join('|');
      };
      a.send();
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(400));
    let text = result.final_text;
    assert!(text.contains("sync:4:200:sync-body"), "{text}");
    assert!(text.contains("hdr:null"), "{text}");
    assert!(text.contains("ctype:text/plain;charset=utf-8"), "{text}");
    assert!(text.contains("async:async-body"), "{text}");
    assert!(text.contains("states:2,3,4"), "{text}");
}

#[test]
fn fetch_request_and_response_objects_work() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var h = new Headers([['X-A', 'b'], ['x-a', 'c']]);
      var merged = h.get('X-A');
      var replaced = new Request(new Request('data:,u', { headers: { 'H': 'old' } }), { headers: { 'H': 'new' } });
      var req = new Request('data:,static', { method: 'POST', headers: h, body: 'posted' });
      Promise.all([
        fetch(req).then(function (r) { return r.text().then(function (t) { return 'post:' + t; }); }),
        fetch('data:,{"n":1}', { method: 'GET' }).then(function (r) { return r.json().then(function (j) { return 'json:' + j.n; }); }),
        fetch('data:,buf').then(function (r) { return r.arrayBuffer().then(function (b) {
          var v = new Uint8Array(b);
          return 'buf:' + v[0] + ',' + v[1] + ',' + v[2];
        }); }),
      ]).then(function (parts) {
        document.getElementById('t').textContent =
          [merged, req.method, replaced.headers.get('H'), parts.join(',')].join('|');
      });
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(400));
    let text = result.final_text;
    assert!(text.starts_with("b, c|POST|new|"), "{text}");
    assert!(text.contains("post:static"), "{text}");
    assert!(text.contains("json:1"), "{text}");
    assert!(text.contains("buf:98,117,102"), "{text}");
}

#[test]
fn text_and_blob_helpers_work() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var enc = new TextEncoder();
      var dec = new TextDecoder();
      var bytes = enc.encode('Glide \u263A');
      var round = dec.decode(bytes);
      var loneHi = enc.encode('\ud800');
      var loneLo = enc.encode('\udc00');
      var blob = new Blob(['hello', enc.encode('!')], { type: 'text/plain' });
      var reader = new FileReader();
      var fatal = 'no-throw';
      try {
        new TextDecoder('utf-8', { fatal: true }).decode(new Uint8Array([0xff]));
        fatal = 'unexpected-ok';
      } catch (e) {
        fatal = e.name;
      }
      var bom = new TextDecoder().decode(new Uint8Array([0xef, 0xbb, 0xbf, 0x41]));
      var overlong = new TextDecoder().decode(new Uint8Array([0xc0, 0x80]));
      Promise.all([
        blob.slice(1, 4).text(),
        new Promise(function (resolve) {
          reader.onloadend = function () { resolve(String(reader.result).slice(0, 30)); };
          reader.readAsDataURL(blob);
        }),
      ]).then(function (parts2) {
        var bad = dec.decode(new Uint8Array([0xff, 0xfe, 0xfd]));
        document.getElementById('t').textContent =
          [round, loneHi.length, loneHi[0], loneLo[0], fatal, bom, overlong, parts2[0], parts2[1], bad].join('|');
      });
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(400));
    let text = result.final_text;
    assert!(text.contains("Glide \u{263A}"), "{text}");
    assert!(text.contains("|3|239|239|"), "{text}");
    assert!(text.contains("TypeError"), "{text}");
    assert!(text.contains("|A|"), "{text}");
    assert!(text.contains("\u{FFFD}\u{FFFD}"), "{text}");
    assert!(text.contains("ell"), "{text}");
    assert!(text.contains("data:text/plain;base64,"), "{text}");
}

#[test]
fn fetch_and_xhr_base64_data_urls_work() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var log = [];
      Promise.all([
        fetch('data:text/plain;base64,aGVsbG8=').then(function (r) { return r.text(); }),
        new Promise(function (resolve) {
          var x = new XMLHttpRequest();
          x.open('GET', 'data:text/plain;base64,aGVsbG8=', false);
          x.send();
          resolve(x.responseText);
        }),
      ]).then(function (parts) {
        log.push('fetch:' + parts[0]);
        log.push('xhr:' + parts[1]);
        return fetch('data:text/plain;base64,%%%').then(function () { return 'bad-ok'; }, function () { return 'bad-err'; });
      }).then(function (bad) {
        log.push(bad);
        document.getElementById('t').textContent = log.join('|');
      });
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(400));
    let text = result.final_text;
    assert!(text.contains("fetch:hello"), "{text}");
    assert!(text.contains("xhr:hello"), "{text}");
    assert!(text.contains("bad-err"), "{text}");
}

#[test]
fn xhr_abort_stops_callbacks() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var events = [];
      var x = new XMLHttpRequest();
      x.addEventListener('loadstart', function () { events.push('loadstart'); });
      x.addEventListener('load', function () { events.push('load:' + x.status); });
      x.addEventListener('abort', function () { events.push('abort'); });
      x.addEventListener('loadend', function () { events.push('loadend'); });
      x.open('GET', 'data:,late', true);
      x.send();
      x.abort();
      var afterAbort = events.join(',');
      x.open('GET', 'data:,reuse', false);
      x.send();
      setTimeout(function () {
        document.getElementById('t').textContent =
          afterAbort + '|reuse:' + x.responseText;
      }, 50);
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(400));
    let text = result.final_text;
    assert_eq!(text, "abort,loadend|reuse:reuse", "{text}");
}

#[test]
fn xhr_headers_available_at_headers_received() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var snap = '';
      var states = [];
      var x = new XMLHttpRequest();
      x.onreadystatechange = function () {
        states.push(x.readyState);
        if (x.readyState === 2) {
          snap = x.status + ':' + x.getResponseHeader('content-type');
        }
      };
      x.open('GET', 'data:,h', true);
      x.onload = function () {
        document.getElementById('t').textContent = snap + '|' + states.join(',');
      };
      x.send();
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(400));
    let text = result.final_text;
    assert!(text.starts_with("200:text/plain;charset=utf-8|"), "{text}");
    assert!(text.ends_with("2,3,4"), "{text}");
}

#[test]
fn fetch_binary_body_round_trips() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var binUrl = 'data:application/octet-stream;base64,AP8BAg==';
      Promise.all([
        fetch(binUrl).then(function (r) { return r.arrayBuffer(); }),
        new Promise(function (resolve) {
          var x = new XMLHttpRequest();
          x.open('GET', binUrl, false);
          x.responseType = 'arraybuffer';
          x.send();
          resolve(new Uint8Array(x.response));
        }),
      ]).then(function (parts) {
        var f = new Uint8Array(parts[0]);
        var x = parts[1];
        document.getElementById('t').textContent =
          [f[0], f[1], f[2], f[3], x[0], x[1], x[2], x[3]].join(',');
      });
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(400));
    assert_eq!(result.final_text, "0,255,1,2,0,255,1,2");
}

#[test]
fn fetch_request_init_overrides_input() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var base = new Request('data:,x', { method: 'GET', headers: { 'H': 'old' }, body: 'old-body' });
      var merged = new Request(base, { method: 'POST', headers: { 'H': 'new' }, body: 'new-body' });
      document.getElementById('t').textContent =
        merged.method + '|' + merged.headers.get('H') + '|' + merged.body;
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "POST|new|new-body");
}

#[test]
fn send_beacon_returns_true_without_throwing() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var ok = true;
      try {
        ok = navigator.sendBeacon('data:,b', 'x') === true;
      } catch (e) {
        ok = false;
      }
      document.getElementById('t').textContent = ok ? 'true' : 'false';
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "true");
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
fn query_selector_all_and_live_collections_are_iterable() {
    let html = r#"<!DOCTYPE html><html><body>
      <div class="a"></div><div class="a"></div>
      <form name="f"><input name="q" value="hi"></form>
      <p id="t">x</p>
      <script>
        var n = 0;
        for (const el of document.querySelectorAll('div')) n++;
        var live = 0;
        for (const el of document.getElementsByTagName('div')) live++;
        var forms = document.forms.length;
        var named = document.forms[0].q && document.f.q;
        var img = new Image(8, 4);
        document.getElementById('t').textContent =
          [n, live, forms, named ? named.name : 'missing', img.localName, img.width, img.height].join(',');
      </script>
    </body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "2,2,1,q,img,8,4");
}

#[test]
fn form_owner_resolves_named_query_control() {
    let html = r#"<!DOCTYPE html><html><body>
      <form id="ext"></form>
      <form name="f">
        <input name="q" id="qq" value="hi">
        <input id="box" type="checkbox">
      </form>
      <input id="outside" name="x">
      <input id="linked" form="ext" name="q" value="z">
      <p id="t">x</p>
      <script>
        document.body.onload = function () {
          var input = document.getElementById('qq');
          var box = document.getElementById('box');
          var outside = document.getElementById('outside');
          var linked = document.getElementById('linked');
          var clicked = false;
          try {
            box.onclick = function () { clicked = !!this.form.q; };
            box.click();
          } catch (e) { clicked = false; }
          var ok = input.form && input.form.getAttribute('name') === 'f'
            && input.form.q.value === 'hi'
            && box.form === input.form
            && outside.form === null
            && linked.form && linked.form.id === 'ext'
            && document.forms.namedItem('f').elements.q.value === 'hi'
            && document.f.q.value === 'hi'
            && clicked;
          document.getElementById('t').textContent = ok ? 'ok' : 'bad';
        };
      </script>
    </body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "ok");
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
fn self_and_window_are_global_this() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">x</p><script>
      var ok = (self === globalThis) && (window === globalThis) &&
        (document.defaultView === globalThis) && (self.parent === self) &&
        (self.top === self) && (self.frames === self);
      document.getElementById('t').textContent = ok ? 'ok' : 'bad';
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(80));
    assert_eq!(result.final_text, "ok");
}

#[test]
fn testharness_defines_test_on_global() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/testharness-global.html");
    let url = format!("file://{}", path.display());
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio");
    let _guard = rt.enter();
    let text = crate::headless::dump_text(&url, true).expect("load harness fixture");
    assert!(
        text.contains("ok"),
        "expected testharness on globalThis, dump: {text}"
    );
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
fn instanceof_node_works() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">x</p><script>
      var n = document.createElement('div');
      var ok = n instanceof Node && n instanceof Element && document.body instanceof HTMLElement;
      document.getElementById('t').textContent = ok ? 'yes' : 'no';
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "yes");
}

#[test]
fn identity_is_stable() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">x</p><script>
      var a = document.getElementById('t');
      var b = document.querySelector('#t');
      document.getElementById('t').textContent = (a === b) ? 'same' : 'diff';
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "same");
}

#[test]
fn get_bounding_client_rect_from_blitz() {
    let html = r#"<!DOCTYPE html><html><body>
      <div id="box" style="width:40px;height:10px"></div><p id="t">x</p>
      <script>
        var r = document.getElementById('box').getBoundingClientRect();
        document.getElementById('t').textContent = String(Math.round(r.width));
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(300));
    assert_eq!(result.final_text, "40");
}

#[test]
fn click_event_reaches_domino_listener() {
    let html = r#"<!DOCTYPE html><html><body>
      <button id="b">go</button><p id="t">no</p>
      <script>
        document.getElementById('b').addEventListener('click', function () {
          document.getElementById('t').textContent = 'yes';
        });
        document.getElementById('b').click();
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "yes");
}

#[test]
fn url_global_parses_relative() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">x</p><script>
      var u = new URL('/a?b=1', 'https://example.test/x');
      var ok = u.protocol === 'https:' && u.pathname === '/a' &&
        u.search === '?b=1' && u.searchParams.get('b') === '1' &&
        String(window) === '[object Window]';
      document.getElementById('t').textContent = ok ? 'yes' : 'no';
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "yes");
}

#[test]
fn search_params_round_trip() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">x</p><script>
      var q = new URLSearchParams('a=1&b=two');
      q.append('a', '3');
      document.getElementById('t').textContent = q.get('a') + '|' + q.get('b');
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "1|two");
}

#[test]
fn dom_parser_parses_html() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">x</p><script>
      var d = new DOMParser().parseFromString('<p id="z">hi</p>', 'text/html');
      var node = d.getElementById('z');
      document.getElementById('t').textContent = node ? node.textContent : 'missing';
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "hi");
}

#[test]
fn tree_walker_walks_elements() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">x</p><div id="a"><span id="b"></span></div>
      <script>
        var w = document.createTreeWalker(document.body, 1);
        var names = [];
        for (var n = w.currentNode; n; n = w.nextNode()) {
          if (n.nodeType === 1 && n.id) names.push(n.id);
        }
        document.getElementById('t').textContent = names.join(',');
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert!(
        result.final_text.contains('a') && result.final_text.contains('b'),
        "walker ids: {}",
        result.final_text
    );
}

#[test]
fn window_capture_sees_event_target() {
    let html = r#"<!DOCTYPE html><html><body>
      <p id="el">x</p><p id="t"></p>
      <script>
        var seen = 'missing';
        var reenter = 'none';
        window.addEventListener('click', function (e) {
          seen = e.target && e.target.id;
          try {
            document.getElementById('el').dispatchEvent(e);
            reenter = 'no-throw';
          } catch (err) {
            reenter = (err && (err.code === 11 || err.name === 'INVALID_STATE_ERR')) ? 'state' : 'other';
          }
        }, true);
        document.getElementById('el').click();
        document.getElementById('t').textContent = 'cap=' + seen + ' re=' + reenter;
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "cap=el re=state");
}

#[test]
fn window_bubble_prevent_default_fails_dispatch() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">x</p>
      <script>
        window.addEventListener('x', function (e) { e.preventDefault(); });
        var ev = new Event('x', { bubbles: true, cancelable: true });
        var ret = document.body.dispatchEvent(ev);
        document.getElementById('t').textContent = 'ret=' + ret;
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "ret=false");
}

#[test]
fn once_listeners_remove_match_and_fire_once() {
    let html = r#"<!DOCTYPE html><html><body>
      <button id="b"></button><p id="t">x</p>
      <script>
        var log = [];
        var b = document.getElementById('b');
        function removed() { log.push('removed'); }
        b.addEventListener('click', removed, { once: true });
        b.removeEventListener('click', removed);
        b.click();
        var n = 0;
        var obj = { handleEvent: function () { n++; log.push('obj' + n); } };
        b.addEventListener('click', obj, { once: true });
        b.click();
        b.click();
        var twice = 0;
        function onceFn() { twice++; log.push('fn' + twice); }
        b.addEventListener('click', onceFn, { once: true });
        b.addEventListener('click', onceFn, { once: true });
        b.click();
        b.click();
        function cap() { log.push('cap'); }
        b.addEventListener('click', cap, { capture: true });
        b.removeEventListener('click', cap, { capture: true });
        b.click();
        var w = 0;
        function winOnce() { w++; log.push('win' + w); }
        window.addEventListener('click', winOnce, { once: true });
        b.click();
        b.click();
        document.getElementById('t').textContent = log.join(',');
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(300));
    assert_eq!(result.final_text, "obj1,fn1,win1");
}

#[test]
fn inline_handler_does_not_leak_parameter_names() {
    // `element` is not a parameter. `defaultView` resolves through `with (document)`.
    // `form` is the button's form property, which is null (`typeof null` is "object").
    let html = r#"<!DOCTYPE html><html><body>
      <button id="b" onclick="document.getElementById('t').textContent =
        (typeof element)+':'+(typeof defaultView)+':'+(typeof form)+':'+(this.id)"></button>
      <p id="t">x</p>
      <script>document.getElementById('b').click();</script>
    </body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "undefined:object:object:b");
}

#[test]
fn removed_positioned_child_stays_paintable() {
    let html = r#"<!DOCTYPE html><html><head><style>
      #p { position: relative; width: 120px; height: 120px; }
      #c { position: absolute; z-index: 2; }
    </style></head><body>
      <div id="p"><p id="c">remove me</p></div>
      <p id="t">before</p>
      <script>
        var removed = document.getElementById('c');
        removed.parentNode.removeChild(removed);
        document.getElementById('t').textContent = 'after';
      </script>
    </body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(250));
    assert_eq!(result.final_text, "after");
}

#[test]
fn performance_timing_and_entries_work() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var ok = true;
      var parts = [];
      var to = performance.timeOrigin;
      var ns = performance.timing.navigationStart;
      if (!(to > 1e11)) ok = false;
      if (!ns || ns > to) ok = false;
      if (performance.navigation.type !== 0) ok = false;
      var nav = performance.getEntriesByType('navigation');
      if (nav.length !== 1) ok = false;
      performance.mark('a');
      performance.mark('b');
      performance.measure('m', 'a', 'b');
      var named = performance.getEntriesByName('m', 'measure');
      if (named.length !== 1) ok = false;
      parts.push(ok ? 'perf' : 'perf-fail');
      parts.push(String(nav[0].duration >= 0));
      document.getElementById('t').textContent = parts.join('|');
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "perf|true");
}

#[test]
fn history_push_state_updates_location_and_state() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var log = [];
      window.addEventListener('popstate', function (e) {
        log.push('pop:' + (e.state && e.state.n) + ':' + location.pathname);
      });
      history.pushState({ n: 1 }, '', '/one');
      history.pushState({ n: 2 }, '', '/two');
      var lenAfterPush = history.length;
      var urlOk = location.pathname === '/two'
        && document.URL.indexOf('/two') >= 0
        && history.state && history.state.n === 2;
      history.replaceState({ n: 9 }, '', '/nine');
      var replaceOk = location.pathname === '/nine'
        && history.state.n === 9
        && history.length === lenAfterPush;
      var crossOk = false;
      try {
        history.pushState({}, '', 'https://evil.test/x');
      } catch (e) {
        crossOk = e.name === 'SecurityError' && location.pathname === '/nine';
      }
      history.back();
      var popped = log.join(';');
      document.getElementById('t').textContent =
        [urlOk, replaceOk, crossOk, popped].join('|');
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(300));
    assert_eq!(result.final_text, "true|true|true|pop:1:/one");
}

#[test]
fn crypto_get_random_values_and_uuid_work() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var a = new Uint8Array(32);
      var r1 = crypto.getRandomValues(a);
      var allZero = a.every(function (b) { return b === 0; });
      var sameRef = r1 === a;
      var a2 = new Uint8Array(32);
      crypto.getRandomValues(a2);
      var differ = a.some(function (b, i) { return b !== a2[i]; });
      var quotaOk = false;
      try {
        crypto.getRandomValues(new Uint8Array(65537));
      } catch (e) {
        quotaOk = e.name === 'QuotaExceededError';
      }
      var u1 = crypto.randomUUID();
      var u2 = crypto.randomUUID();
      var re = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
      var uuidOk = re.test(u1) && re.test(u2) && u1 !== u2;
      var ok = !allZero && sameRef && differ && quotaOk && uuidOk && crypto.subtle === undefined;
      document.getElementById('t').textContent = ok ? 'ok' : 'bad';
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "ok");
}

#[test]
fn performance_timing_snapshot_is_stable() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var origUrl = location.href;
      var t0 = performance.timing;
      var same = performance.timing === t0;
      var spread0 = t0.loadEventEnd - t0.navigationStart;
      var navName = performance.getEntriesByType('navigation')[0].name;
      history.pushState({ x: 1 }, '', '/moved');
      var navAfter = performance.getEntriesByType('navigation')[0].name;
      setTimeout(function () {
        var spread1 = performance.timing.loadEventEnd - performance.timing.navigationStart;
        var stillSame = performance.timing === t0;
        document.getElementById('t').textContent = [
          same, stillSame, spread0 === spread1,
          navName === origUrl, navAfter === origUrl
        ].join('|');
      }, 40);
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(350));
    assert_eq!(result.final_text, "true|true|true|true|true");
}

#[test]
fn history_back_restores_initial_url() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var rootPath = location.pathname;
      history.pushState({ n: 1 }, '', '/one');
      history.back();
      var ok = location.pathname === rootPath
        && document.URL.indexOf(rootPath) >= 0
        && history.state === null;
      document.getElementById('t').textContent = ok ? 'ok' : 'bad';
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "ok");
}

#[test]
fn history_go_coercion_and_bounds() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      history.pushState({ n: 1 }, '', '/a');
      history.pushState({ n: 2 }, '', '/b');
      var parts = [];
      history.go();
      parts.push('na', history.state && history.state.n);
      history.go('x');
      parts.push('str', history.state && history.state.n);
      history.go(0.5);
      parts.push('frac', history.state && history.state.n);
      history.go(-1);
      parts.push('back', location.pathname, history.state && history.state.n);
      history.go(1);
      parts.push('fwd', location.pathname, history.state && history.state.n);
      history.go(999);
      parts.push('oor', location.pathname);
      history.pushState({ n: 9 }, '', '/x');
      parts.push('still', typeof history.length);
      document.getElementById('t').textContent = parts.join('|');
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(250));
    assert_eq!(
        result.final_text,
        "na|2|str|2|frac|2|back|/a|1|fwd|/b|2|oor|/b|still|number"
    );
}

#[test]
fn history_onpopstate_property_fires() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var hits = [];
      window.onpopstate = function (e) {
        hits.push('on:' + (e.state === null ? 'null' : e.state.v));
      };
      history.pushState({ v: 1 }, '', '/p');
      history.back();
      document.getElementById('t').textContent = hits.join(',');
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "on:null");
}

#[test]
fn history_forward_and_push_truncates_forward_stack() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      history.pushState({ n: 1 }, '', '/a');
      history.pushState({ n: 2 }, '', '/b');
      history.back();
      history.pushState({ n: 3 }, '', '/c');
      var fwd = false;
      try { history.forward(); fwd = location.pathname === '/b'; } catch (_) { fwd = false; }
      var ok = location.pathname === '/c'
        && history.state.n === 3
        && !fwd
        && history.length === 3;
      document.getElementById('t').textContent = ok ? 'ok' : 'bad:' + location.pathname;
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "ok");
}

#[test]
fn history_location_aliases_match_after_push_state() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      history.pushState({ n: 1 }, '', '/alias');
      var ok = location.pathname === '/alias'
        && window.location === location
        && document.location === location
        && document.location.pathname === '/alias';
      document.getElementById('t').textContent = ok ? 'ok' : 'bad';
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "ok");
}

#[test]
fn history_push_state_invalid_url_throws_syntax_error() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var pathBefore = location.pathname;
      var ok = false;
      try {
        history.pushState({}, '', 'http://exam ple.com/x');
      } catch (e) {
        ok = e.name === 'SyntaxError' && location.pathname === pathBefore;
      }
      document.getElementById('t').textContent = ok ? 'ok' : 'bad';
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "ok");
}

#[test]
fn history_and_performance_buffers_are_bounded() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      for (var i = 0; i < 200; i += 1) {
        history.pushState({ i: i }, '', '/p' + i);
      }
      for (var j = 0; j < 300; j += 1) {
        performance.mark('m' + j);
      }
      var ok = history.length <= 100
        && performance.getEntriesByType('mark').length <= 250;
      document.getElementById('t').textContent = ok ? 'ok' : 'bad';
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(400));
    assert_eq!(result.final_text, "ok");
}

#[test]
fn crypto_rejects_non_integer_views() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var bi = new BigUint64Array(2);
      var out = crypto.getRandomValues(bi) === bi && bi[0] !== 0n;
      var dv = false;
      var f32 = false;
      try { crypto.getRandomValues(new DataView(new ArrayBuffer(8))); } catch (e) { dv = e instanceof TypeError; }
      try { crypto.getRandomValues(new Float32Array(4)); } catch (e) { f32 = e instanceof TypeError; }
      document.getElementById('t').textContent = [out, dv, f32].join('|');
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "true|true|true");
}

#[test]
fn navigator_host_hints_work() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var langsOk = navigator.languages[0] === 'en-US'
        && Object.isFrozen(navigator.languages);
      var platOk = typeof navigator.platform === 'string' && navigator.platform.length > 0;
      var coresOk = navigator.hardwareConcurrency >= 1;
      document.getElementById('t').textContent =
        [langsOk, platOk, coresOk].join('|');
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "true|true|true");
}

#[test]
fn mutation_observer_reports_child_and_attribute_changes() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var log = [];
      var mo = new MutationObserver(function (recs) {
        log.push('cb:' + recs.length);
      });
      mo.observe(document.body, { childList: true, attributes: true, subtree: true });
      var el = document.createElement('span');
      el.id = 'added';
      document.body.appendChild(el);
      el.setAttribute('data-a', '1');
      var pending = mo.takeRecords();
      var child = pending.filter(function (r) { return r.type === 'childList'; })[0];
      var attr = pending.filter(function (r) { return r.type === 'attributes'; })[0];
      log.push('take:' + pending.length);
      log.push('child:' + (child && child.addedNodes.length === 1 ? 'ok' : 'bad'));
      log.push('attr:' + (attr && attr.attributeName === 'data-a' ? 'ok' : 'bad'));
      setTimeout(function () {
        document.body.appendChild(document.createElement('i'));
      }, 5);
      setTimeout(function () {
        mo.disconnect();
        document.body.appendChild(document.createElement('b'));
        var hadCb = log.some(function (x) { return x.indexOf('cb:') === 0; });
        log.push('hadCb:' + hadCb);
        document.getElementById('t').textContent = log.join('|');
      }, 80);
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(400));
    let text = result.final_text;
    assert!(text.contains("take:2"), "{text}");
    assert!(text.contains("child:ok"), "{text}");
    assert!(text.contains("attr:ok"), "{text}");
    assert!(text.contains("hadCb:true"), "{text}");
}

#[test]
fn mutation_observer_honors_attribute_filter_and_character_data() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var pending = [];
      var mo = new MutationObserver(function () {});
      mo.observe(document.body, {
        attributes: true,
        attributeFilter: ['data-x'],
        subtree: true,
        characterData: true,
      });
      var el = document.createElement('div');
      document.body.appendChild(el);
      el.setAttribute('data-x', '1');
      el.setAttribute('data-y', '2');
      var text = document.createTextNode('hi');
      el.appendChild(text);
      text.data = 'bye';
      pending = mo.takeRecords();
      var names = pending.filter(function (r) { return r.type === 'attributes'; })
        .map(function (r) { return r.attributeName; });
      var charOk = pending.some(function (r) { return r.type === 'characterData'; });
      var filterOk = names.indexOf('data-x') >= 0 && names.indexOf('data-y') < 0;
      setTimeout(function () {
        document.getElementById('t').textContent =
          [filterOk, charOk, pending.length >= 2].join('|');
      }, 20);
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(300));
    assert_eq!(result.final_text, "true|true|true");
}

#[test]
fn mutation_observer_accepts_text_and_comment_targets() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var parts = [];
      function charDataRecs(mo) {
        return mo.takeRecords().filter(function (r) { return r.type === 'characterData'; });
      }
      function okSingle(mo, target) {
        var recs = charDataRecs(mo);
        return recs.length === 1 && recs[0].target === target;
      }
      var text = document.createTextNode('a');
      var comment = document.createComment('b');
      document.body.appendChild(text);
      document.body.appendChild(comment);
      var textMo = new MutationObserver(function () {});
      var commentMo = new MutationObserver(function () {});
      textMo.observe(text, { characterData: true });
      commentMo.observe(comment, { characterData: true });
      text.data = 'x';
      comment.data = 'y';
      parts.push(okSingle(textMo, text) ? 'text-ok' : 'text-bad');
      parts.push(okSingle(commentMo, comment) ? 'comment-ok' : 'comment-bad');
      var threw = false;
      try {
        textMo.observe({}, { characterData: true });
      } catch (e) {
        threw = e instanceof TypeError;
      }
      parts.push(threw ? 'type-ok' : 'type-bad');
      var detachedText = document.createTextNode('d');
      var detachedComment = document.createComment('c');
      var detachedTextMo = new MutationObserver(function () {});
      var detachedCommentMo = new MutationObserver(function () {});
      detachedTextMo.observe(detachedText, { characterData: true });
      detachedCommentMo.observe(detachedComment, { characterData: true });
      detachedText.data = 'e';
      detachedComment.nodeValue = 'f';
      parts.push(okSingle(detachedTextMo, detachedText) ? 'detached-text-ok' : 'detached-text-bad');
      parts.push(okSingle(detachedCommentMo, detachedComment)
        ? 'detached-comment-ok' : 'detached-comment-bad');
      var bag = document.createElement('div');
      var nestedText = document.createTextNode('nested');
      bag.appendChild(nestedText);
      var cloneHost = document.createElement('div');
      cloneHost.appendChild(nestedText.cloneNode(true));
      var bagMo = new MutationObserver(function () {});
      var cloneMo = new MutationObserver(function () {});
      bagMo.observe(bag, { characterData: true, subtree: true });
      cloneMo.observe(cloneHost, { characterData: true, subtree: true });
      nestedText.data = 'nested2';
      cloneHost.firstChild.data = 'cloned2';
      parts.push(okSingle(bagMo, nestedText) ? 'subtree-ok' : 'subtree-bad');
      parts.push(okSingle(cloneMo, cloneHost.firstChild) ? 'clone-subtree-ok' : 'clone-subtree-bad');
      document.getElementById('t').textContent = parts.join('|');
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(
        result.final_text,
        "text-ok|comment-ok|type-ok|detached-text-ok|detached-comment-ok|subtree-ok|clone-subtree-ok"
    );
}

#[test]
fn intersection_observer_fires_for_visible_and_hidden_elements() {
    let html = r#"<!DOCTYPE html><html><body>
      <div id="vis" style="width:40px;height:10px"></div>
      <div id="hid" style="display:none;width:40px;height:10px"></div>
      <p id="t">pending</p>
      <script>
        var out = { vis: null, hid: null, n: 0 };
        var io = new IntersectionObserver(function (entries) {
          entries.forEach(function (e) {
            out.n += 1;
            if (e.target.id === 'vis') out.vis = e.isIntersecting;
            if (e.target.id === 'hid') out.hid = e.isIntersecting;
          });
        });
        io.observe(document.getElementById('vis'));
        io.observe(document.getElementById('hid'));
        setTimeout(function () {
          document.getElementById('t').textContent =
            [out.n >= 2, out.vis === true, out.hid === false].join('|');
        }, 80);
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(400));
    assert_eq!(result.final_text, "true|true|true");
}

#[test]
fn resize_observer_reports_initial_size_once() {
    let html = r#"<!DOCTYPE html><html><body>
      <div id="box" style="width:40px;height:10px"></div>
      <p id="t">pending</p>
      <script>
        var sizes = [];
        var ro = new ResizeObserver(function (entries) {
          var cr = entries[0].contentRect;
          sizes.push(Math.round(cr.width) + 'x' + Math.round(cr.height));
        });
        ro.observe(document.getElementById('box'));
        ro.observe(document.getElementById('box'));
        setTimeout(function () {
          ro.observe(document.getElementById('box'));
        }, 20);
        setTimeout(function () {
          document.getElementById('t').textContent = sizes.join(',');
        }, 120);
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(400));
    assert!(
        result.final_text.starts_with("40x10"),
        "{}",
        result.final_text
    );
    assert!(
        result.final_text.matches(',').count() <= 1,
        "{}",
        result.final_text
    );
}

#[test]
fn idle_callback_runs_with_a_deadline() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      requestIdleCallback(function (deadline) {
        var ok = deadline.timeRemaining() > 0 && typeof deadline.didTimeout === 'boolean';
        document.getElementById('t').textContent = ok ? 'ok' : 'bad';
      });
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(300));
    assert_eq!(result.final_text, "ok");
}

#[test]
fn css_supports_escape_and_custom_elements_registry() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var parts = [];
      parts.push(CSS.supports('display', 'flex') ? 'flex' : 'no-flex');
      parts.push(CSS.supports('not-a-real-prop', 'foo') ? 'bad-unknown' : 'unknown-false');
      parts.push(CSS.escape('1a') === '\\31 a' ? 'escape-ok' : 'escape-bad');
      customElements.define('x-ce-test', function () {});
      var dup = false;
      try {
        customElements.define('x-ce-test', function () {});
      } catch (e) {
        dup = e.name === 'NotSupportedError';
      }
      var got = !!customElements.get('x-ce-test');
      var p = customElements.whenDefined('x-ce-when');
      parts.push([got, dup, p instanceof Promise].join(','));
      document.getElementById('t').textContent = parts.join('|');
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(
        result.final_text,
        "flex|unknown-false|escape-ok|true,true,true"
    );
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
        module: false,
        crossorigin: None,
        element: None,
    }];
    let sources =
        crate::js::scripts::resolve_sources(&specs, &net, "https://example.test/", handle);
    assert!(matches!(
        sources[0],
        crate::js::scripts::ScriptSource::Failed
    ));
}

#[test]
fn console_log_reaches_console_feed() {
    let html = r#"<!DOCTYPE html><html><body>
    <script>
      console.log('devtools-console-test');
      console.warn('watch');
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, std::time::Duration::from_millis(120));
    assert_eq!(result.final_text, "");
    let events = result.console_wire;
    let messages: Vec<_> = events.iter().map(|e| e.message.as_str()).collect();
    assert!(
        messages.iter().any(|m| m.contains("devtools-console-test")),
        "{messages:?}"
    );
    assert!(messages.iter().any(|m| m.contains("watch")), "{messages:?}");
    assert!(events.iter().any(|e| e.level == "log"));
    assert!(events.iter().any(|e| e.level == "warn"));
}

#[test]
fn console_log_includes_line_number_from_stack() {
    let html = r#"<!DOCTYPE html><html><body>
    <script>
      console.log('line-numbered');
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, std::time::Duration::from_millis(120));
    let log = result
        .console_wire
        .iter()
        .find(|e| e.level == "log" && e.message.contains("line-numbered"))
        .expect("console log wire");
    assert!(
        log.line.is_some(),
        "expected line number on console log, got {log:?}"
    );
}

#[test]
fn uncaught_exception_reaches_console_feed() {
    let html = r#"<!DOCTYPE html><html><body>
    <script>
      throw new Error('boom-console');
    </script></body></html>"#;
    let result = super::harness::run_inline_page(html, std::time::Duration::from_millis(120));
    let events = result.console_wire;
    assert!(
        events
            .iter()
            .any(|e| e.level == "uncaught" && e.message.contains("boom-console")),
        "{events:?}"
    );
}

#[test]
fn unhandled_rejection_reaches_console_feed() {
    let html = r#"<!DOCTYPE html><html><body>
    <script>
      Promise.reject(new Error('reject-console'));
    </script></body></html>"#;
    let result = super::harness::run_inline_page_with_extra_settle(
        html,
        std::time::Duration::from_millis(200),
        8,
    );
    let events = result.console_wire;
    assert!(
        events
            .iter()
            .any(|e| e.level == "rejection" && e.message.contains("reject-console")),
        "{events:?}"
    );
}

#[test]
fn handled_rejection_revokes_console_row_same_turn() {
    let html = r#"<!DOCTYPE html><html><body>
    <script>
      Promise.reject(new Error('catch-me')).catch(function () {});
    </script></body></html>"#;
    let result = super::harness::run_inline_page_with_extra_settle(
        html,
        std::time::Duration::from_millis(200),
        8,
    );
    let mut tabs = crate::tabs::TabManager::new();
    let id = tabs.add("https://example.test/");
    assert_eq!(tabs.tab_mut(id).map(|t| t.console_epoch), Some(1));
    crate::devtools::console::apply_console_wire(&mut tabs, &result.console_wire);
    let tab = tabs.active().expect("tab");
    assert!(
        tab.console.entries().all(|e| !matches!(
            e.level,
            crate::devtools::console::ConsoleLevel::UnhandledRejection
        )),
        "{:?}",
        tab.console.entries().collect::<Vec<_>>()
    );
}

#[test]
fn handled_rejection_revokes_after_settimeout_catch() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
    <script>
      var p = Promise.reject(new Error('late-catch'));
      setTimeout(function () {
        p.catch(function () {});
        document.getElementById('t').textContent = 'done';
      }, 10);
    </script></body></html>"#;
    let result = super::harness::run_inline_page_with_extra_settle(
        html,
        std::time::Duration::from_millis(300),
        8,
    );
    assert_eq!(result.final_text, "done");
    let mut tabs = crate::tabs::TabManager::new();
    let id = tabs.add("https://example.test/");
    assert_eq!(tabs.tab_mut(id).map(|t| t.console_epoch), Some(1));
    crate::devtools::console::apply_console_wire(&mut tabs, &result.console_wire);
    let tab = tabs.active().expect("tab");
    assert!(
        tab.console.entries().all(|e| !matches!(
            e.level,
            crate::devtools::console::ConsoleLevel::UnhandledRejection
        )),
        "{:?}",
        tab.console.entries().collect::<Vec<_>>()
    );
}

#[test]
fn two_unhandled_rejections_revoking_one_leaves_one_row() {
    let html = r#"<!DOCTYPE html><html><body>
    <script>
      var p1 = Promise.reject(new Error('one'));
      var p2 = Promise.reject(new Error('two'));
      p1.catch(function () {});
    </script></body></html>"#;
    let result = super::harness::run_inline_page_with_extra_settle(
        html,
        std::time::Duration::from_millis(250),
        8,
    );
    let mut tabs = crate::tabs::TabManager::new();
    let id = tabs.add("https://example.test/");
    assert_eq!(tabs.tab_mut(id).map(|t| t.console_epoch), Some(1));
    crate::devtools::console::apply_console_wire(&mut tabs, &result.console_wire);
    let tab = tabs.active().expect("tab");
    let rejections: Vec<_> = tab
        .console
        .entries()
        .filter(|e| {
            matches!(
                e.level,
                crate::devtools::console::ConsoleLevel::UnhandledRejection
            )
        })
        .collect();
    assert_eq!(
        rejections.len(),
        1,
        "{:?}",
        tab.console.entries().collect::<Vec<_>>()
    );
    assert!(
        rejections[0].message.contains("two"),
        "{:?}",
        rejections[0].message
    );
}

#[test]
fn repl_eval_returns_formatted_result() {
    let html = r#"<!DOCTYPE html><html><body><script>var x = 0;</script></body></html>"#;
    let wire = super::harness::run_inline_page_with_repl(
        html,
        "2 + 2",
        std::time::Duration::from_millis(200),
    )
    .expect("repl eval");
    assert!(
        wire.iter()
            .any(|e| e.level == "repl-result" && e.message.contains("< 4")),
        "{wire:?}"
    );
}

#[test]
fn repl_eval_throws_error_row() {
    let html = r#"<!DOCTYPE html><html><body></body></html>"#;
    let wire = super::harness::run_inline_page_with_repl(
        html,
        "throw new Error('repl-fail')",
        std::time::Duration::from_millis(200),
    )
    .expect("repl eval");
    assert!(
        wire.iter()
            .any(|e| e.level == "error" && e.message.contains("repl-fail")),
        "{wire:?}"
    );
}

#[test]
fn repl_eval_respects_watchdog() {
    let mut harness = super::harness::EvalHarness::with_watchdog(
        EMPTY_PAGE,
        true,
        std::time::Duration::from_millis(150),
        300,
    );
    let timeout = harness
        .submit_reply("while (true) {}", std::time::Duration::from_secs(5))
        .expect("reply");
    assert!(
        matches!(timeout, Err(ref message) if message.to_lowercase().contains("timed out")),
        "{timeout:?}"
    );
    let after = harness
        .submit_reply("1 + 1", std::time::Duration::from_secs(5))
        .expect("reply");
    assert_eq!(after, Ok("2".to_string()));
    let wire = harness.js_host.drain_console_wire();
    assert!(
        wire.iter()
            .any(|e| e.level == "error" && e.message.to_lowercase().contains("timed out")),
        "{wire:?}"
    );
    assert!(
        wire.iter()
            .any(|e| e.level == "repl-result" && e.message.contains("< 2")),
        "{wire:?}"
    );
}

fn repl_followup_after_hostile_preview(expr: &str, preview_needle: &str) {
    let html = r#"<!DOCTYPE html><html><body></body></html>"#;
    let wire = super::harness::run_inline_page_with_repl_sequence(
        html,
        &[expr, "1 + 1"],
        std::time::Duration::from_millis(200),
        std::time::Duration::from_millis(200),
    )
    .expect("repl sequence");
    assert!(
        wire.iter()
            .any(|e| e.level == "repl-result" && e.message.contains(preview_needle)),
        "{wire:?}"
    );
    assert!(
        wire.iter()
            .any(|e| e.level == "repl-result" && e.message.contains("< 2")),
        "{wire:?}"
    );
}

#[test]
fn repl_eval_preview_symbol_then_followup_addition() {
    repl_followup_after_hostile_preview("Symbol('s')", "Symbol");
}

#[test]
fn repl_eval_preview_accessor_without_calling_then_followup() {
    repl_followup_after_hostile_preview("({ get x() { throw 1 } })", "get x");
}

#[test]
fn repl_eval_preview_infinite_accessor_without_hang_then_followup() {
    repl_followup_after_hostile_preview("({ get x() { while (true) {} } })", "get x");
}

#[test]
fn repl_eval_preview_proxy_without_traps_then_followup() {
    repl_followup_after_hostile_preview(
        "new Proxy({}, { ownKeys() { while (true) {} } })",
        "Proxy",
    );
}

#[test]
fn repl_eval_global_var_visible_on_next_eval() {
    let html = r#"<!DOCTYPE html><html><body></body></html>"#;
    let wire = super::harness::run_inline_page_with_repl_sequence(
        html,
        &["var x = 5", "x"],
        std::time::Duration::from_millis(200),
        std::time::Duration::from_millis(200),
    )
    .expect("repl sequence");
    assert!(
        wire.iter()
            .any(|e| e.level == "repl-result" && e.message.contains("< 5")),
        "{wire:?}"
    );
}

const EMPTY_PAGE: &str = r#"<!DOCTYPE html><html><body></body></html>"#;

#[test]
fn repl_eval_reply_receives_result() {
    let mut harness =
        super::harness::EvalHarness::new(EMPTY_PAGE, true, std::time::Duration::from_millis(150));
    let outcome = harness
        .submit_reply("2 + 2", std::time::Duration::from_secs(3))
        .expect("reply");
    assert_eq!(outcome, Ok("4".to_string()));
    // The DevTools console keeps receiving the same wire row as a REPL submission.
    let wire = harness.js_host.drain_console_wire();
    assert!(
        wire.iter()
            .any(|e| e.level == "repl-result" && e.message.contains("< 4")),
        "{wire:?}"
    );
}

#[test]
fn repl_eval_reply_receives_error() {
    let mut harness =
        super::harness::EvalHarness::new(EMPTY_PAGE, true, std::time::Duration::from_millis(150));
    let outcome = harness
        .submit_reply(
            "throw new Error('repl-fail')",
            std::time::Duration::from_secs(3),
        )
        .expect("reply");
    assert!(
        matches!(outcome, Err(ref message) if message.contains("repl-fail")),
        "{outcome:?}"
    );
}

#[test]
fn repl_eval_reply_on_watchdog_timeout() {
    let mut harness = super::harness::EvalHarness::with_watchdog(
        EMPTY_PAGE,
        true,
        std::time::Duration::from_millis(150),
        300,
    );
    let outcome = harness
        .submit_reply("while (true) {}", std::time::Duration::from_secs(5))
        .expect("reply");
    assert!(
        matches!(outcome, Err(ref message) if message.to_lowercase().contains("timed out")),
        "{outcome:?}"
    );
    // The watchdog must cancel termination cleanly so the next eval still runs.
    let after = harness
        .submit_reply("1 + 1", std::time::Duration::from_secs(5))
        .expect("reply");
    assert_eq!(after, Ok("2".to_string()));
}

#[test]
fn throwing_eval_does_not_leave_watchdog_armed() {
    let mut harness = super::harness::EvalHarness::with_watchdog(
        EMPTY_PAGE,
        true,
        std::time::Duration::from_millis(150),
        300,
    );
    let thrown = harness
        .submit_reply("throw new Error('x')", std::time::Duration::from_secs(3))
        .expect("reply");
    assert!(
        matches!(thrown, Err(ref message) if message.contains('x')),
        "{thrown:?}"
    );
    // Wait past the watchdog: a leaked watchdog thread would terminate the isolate now.
    std::thread::sleep(std::time::Duration::from_millis(600));
    let after = harness
        .submit_reply("1 + 1", std::time::Duration::from_secs(3))
        .expect("reply");
    assert_eq!(after, Ok("2".to_string()));
}

#[test]
fn compile_error_eval_does_not_leave_watchdog_armed() {
    let mut harness = super::harness::EvalHarness::with_watchdog(
        EMPTY_PAGE,
        true,
        std::time::Duration::from_millis(150),
        300,
    );
    let failed = harness
        .submit_reply("1 +", std::time::Duration::from_secs(3))
        .expect("reply");
    assert!(failed.is_err(), "{failed:?}");
    std::thread::sleep(std::time::Duration::from_millis(600));
    let after = harness
        .submit_reply("1 + 1", std::time::Duration::from_secs(3))
        .expect("reply");
    assert_eq!(after, Ok("2".to_string()));
}

#[test]
fn stale_route_eval_reply_is_error() {
    let mut harness =
        super::harness::EvalHarness::new(EMPTY_PAGE, true, std::time::Duration::from_millis(150));
    let outcome = harness
        .submit_reply_at("1 + 1", 9, 9, std::time::Duration::from_secs(3))
        .expect("reply");
    assert!(
        matches!(outcome, Err(ref message) if message.starts_with("Expression ignored")),
        "{outcome:?}"
    );
}

#[test]
fn throwing_eval_still_mirrors_dom_mutations() {
    let mut harness =
        super::harness::EvalHarness::new(EMPTY_PAGE, true, std::time::Duration::from_millis(150));
    let outcome = harness
        .submit_reply(
            "var p=document.createElement('p'); p.id='t'; p.textContent='kept'; \
             document.body.appendChild(p); throw 1",
            std::time::Duration::from_secs(3),
        )
        .expect("reply");
    assert!(outcome.is_err(), "{outcome:?}");
    assert_eq!(
        harness.element_text("t").as_deref(),
        Some("kept"),
        "the error path must still flush queued DOM mutations"
    );
}

#[test]
fn watchdog_timeout_in_microtasks_still_flushes_dom() {
    let mut harness = super::harness::EvalHarness::with_watchdog(
        EMPTY_PAGE,
        true,
        std::time::Duration::from_millis(150),
        300,
    );
    let outcome = harness
        .submit_reply(
            "document.body.appendChild(Object.assign(document.createElement('p'), \
             {id:'m',textContent:'m'})); \
             Promise.resolve().then(function f(){ return Promise.resolve().then(f); }); 0",
            std::time::Duration::from_secs(5),
        )
        .expect("reply");
    assert!(
        matches!(outcome, Err(ref message) if message.to_lowercase().contains("timed out")),
        "{outcome:?}"
    );
    assert_eq!(
        harness.element_text("m").as_deref(),
        Some("m"),
        "a watchdog timeout in microtasks must not drop the queued mutation batch"
    );
    let result = harness
        .eval_blocking("document.getElementById('m').textContent = 'm2'")
        .expect("eval");
    assert_eq!(result, "\"m2\"");
    assert_eq!(harness.element_text("m").as_deref(), Some("m2"));
}

#[test]
fn repl_eval_microtask_console_log_precedes_result() {
    let mut harness =
        super::harness::EvalHarness::new(EMPTY_PAGE, true, std::time::Duration::from_millis(150));
    let outcome = harness
        .submit_reply(
            "Promise.resolve().then(() => console.log('p')); 1",
            std::time::Duration::from_secs(3),
        )
        .expect("reply");
    assert_eq!(outcome, Ok("1".to_string()));
    let wire = harness.js_host.drain_console_wire();
    let logged = wire
        .iter()
        .position(|e| e.level == "log" && e.message == "p");
    let result = wire
        .iter()
        .position(|e| e.level == "repl-result" && e.message.contains("< 1"));
    assert!(
        matches!((logged, result), (Some(logged), Some(result)) if logged < result),
        "{wire:?}"
    );
}

#[test]
fn eval_blocking_returns_value() {
    let mut harness =
        super::harness::EvalHarness::new(EMPTY_PAGE, true, std::time::Duration::from_millis(150));
    assert_eq!(harness.eval_blocking("1 + 1").expect("eval"), "2");
}

#[test]
fn eval_blocking_dom_mutation_is_mirrored() {
    let mut harness =
        super::harness::EvalHarness::new(EMPTY_PAGE, true, std::time::Duration::from_millis(150));
    let result = harness
        .eval_blocking(
            "(function () {\
               var p = document.createElement('p');\
               p.id = 'added';\
               p.textContent = 'mirrored';\
               document.body.appendChild(p);\
               return 'ok';\
             })()",
        )
        .expect("eval");
    assert_eq!(result, "\"ok\"");
    assert_eq!(
        harness.element_text("added").as_deref(),
        Some("mirrored"),
        "the Blitz document should mirror the eval mutation"
    );
}

#[test]
fn eval_blocking_with_js_disabled_is_error() {
    let mut harness =
        super::harness::EvalHarness::new(EMPTY_PAGE, false, std::time::Duration::from_millis(50));
    let error = harness.eval_blocking("1 + 1").expect_err("disabled");
    assert!(error.contains("JavaScript is disabled"), "{error}");
}

#[test]
fn json_eval_returns_json_text() {
    let mut harness =
        super::harness::EvalHarness::new(EMPTY_PAGE, true, std::time::Duration::from_millis(150));
    let source = super::repl_eval::json_eval_source("({ a: 1 })");
    let raw = harness.eval_blocking(&source).expect("eval");
    // The reply is the DevTools preview of a string, so parse the JS/JSON string
    // literal to recover the JSON text `JSON.stringify` produced.
    let json_text: String = serde_json::from_str(&raw).expect("string preview parses as JSON");
    assert_eq!(json_text, r#"{"a":1}"#);
}

#[test]
fn input_value_round_trips_same_turn_without_stale_readback() {
    let html = r#"<!DOCTYPE html><html><body>
      <input id="i" value="">
      <p id="t">x</p>
      <script>
        var input = document.getElementById('i');
        input.value = 'hello';
        document.getElementById('t').textContent = input.value;
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(80));
    assert_eq!(result.final_text, "hello");
}

#[test]
fn request_submit_rejects_foreign_submitter() {
    let html = r#"<!DOCTYPE html><html><body>
      <form id="f"></form>
      <button id="b" type="submit"></button>
      <p id="t">pending</p>
      <script>
        var err = '';
        try {
          document.getElementById('f').requestSubmit(document.getElementById('b'));
        } catch (e) {
          err = (e.name || 'Error') + ':' + (e.code != null ? e.code : '');
        }
        document.getElementById('t').textContent = err;
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(80));
    assert!(
        result.final_text.contains("NotFoundError") || result.final_text.contains("not owned"),
        "{}",
        result.final_text
    );
}

#[test]
fn hidden_gbv_submit_handler_updates_entry_list() {
    let html = r#"<!DOCTYPE html><html><body>
      <form action="/search" method="get">
        <input type="hidden" name="gbv" id="gbv" value="1">
      </form>
      <script>
        var f = document.forms[0];
        f.onsubmit = function () {
          if (typeof XMLHttpRequest !== 'undefined') {
            var el = document.getElementById('gbv');
            try { el.value = '2'; } catch (_) {}
            el.setAttribute('value', '2');
          }
        };
        f.requestSubmit();
      </script></body></html>"#;
    let (_, urls) =
        super::harness::run_inline_page_with_navigation(html, Duration::from_millis(80));
    assert_eq!(urls.len(), 1, "{urls:?}");
    assert!(
        urls[0].contains("gbv=2"),
        "expected upgraded gbv in navigation URL, got {}",
        urls[0]
    );
}

#[test]
fn request_submit_fires_submit_event_and_navigates() {
    let html = r#"<!DOCTYPE html><html><body>
      <form action="/search" method="get">
        <input name="q" value="glide">
        <input type="hidden" name="token">
      </form>
      <p id="t">pending</p>
      <script>
        var f = document.forms[0];
        var fired = false;
        f.onsubmit = function (e) {
          fired = true;
          e.target.elements.namedItem('token').value = 't1';
        };
        f.requestSubmit();
        document.getElementById('t').textContent = fired ? 'fired' : 'missed';
      </script></body></html>"#;
    let (result, urls) =
        super::harness::run_inline_page_with_navigation(html, Duration::from_millis(80));
    assert_eq!(result.final_text, "fired");
    assert_eq!(urls, vec!["https://example.test/search?q=glide&token=t1"]);
}

#[test]
fn form_submit_does_not_fire_submit_event() {
    let html = r#"<!DOCTYPE html><html><body>
      <form action="/search" method="get"><input name="q" value="glide"></form>
      <p id="t">pending</p>
      <script>
        var f = document.forms[0];
        var fired = false;
        f.onsubmit = function () { fired = true; };
        f.submit();
        document.getElementById('t').textContent = fired ? 'fired' : 'quiet';
      </script></body></html>"#;
    let (result, urls) =
        super::harness::run_inline_page_with_navigation(html, Duration::from_millis(80));
    assert_eq!(result.final_text, "quiet");
    assert_eq!(urls, vec!["https://example.test/search?q=glide"]);
}

#[test]
fn canceled_submit_event_prevents_navigation() {
    let html = r#"<!DOCTYPE html><html><body>
      <form action="/search" method="get"><input name="q" value="glide"></form>
      <script>
        var f = document.forms[0];
        f.onsubmit = function (e) { e.preventDefault(); };
        f.requestSubmit();
      </script></body></html>"#;
    let (_, urls) =
        super::harness::run_inline_page_with_navigation(html, Duration::from_millis(80));
    assert!(urls.is_empty(), "{urls:?}");
}

#[test]
fn location_assign_requests_navigation() {
    let html = r#"<!DOCTYPE html><html><body>
      <p id="t">x</p>
      <script>location.assign('/next?x=1');</script></body></html>"#;
    let (_, urls) =
        super::harness::run_inline_page_with_navigation(html, Duration::from_millis(80));
    assert_eq!(urls, vec!["https://example.test/next?x=1"]);
}

#[test]
fn timers_fixture_reports_wake_and_advances_with_pump() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/timers.html");
    let html = std::fs::read_to_string(path).expect("timers fixture");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let net = Arc::new(blitz_net::Provider::new(None));
    let tap = crate::js::JsTap::new_enabled();
    let mut host = crate::js::JsHost::new(true, tap, Some(net));
    let provider = crate::html::GlideHtmlProvider {
        scripting_enabled: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        js_tap: crate::js::JsTap::new_enabled(),
    };
    let config = blitz_dom::DocumentConfig {
        base_url: Some("https://example.test/".into()),
        html_parser_provider: Some(Arc::new(provider)),
        ..Default::default()
    };
    let mut doc = crate::parse::document_from_html(&html, config, true);
    host.maybe_start_page(&doc, "https://example.test/timers", rt.handle());
    host.finish_script_resolution_sync(rt.handle(), &crate::js::scripts::collect_scripts(&doc));
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_millis(600) {
        doc.resolve(0.0);
        host.pump(&mut doc, std::time::Instant::now());
        if host
            .next_wake_in()
            .is_some_and(|d| d <= Duration::from_millis(5))
        {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    host.pump(&mut doc, std::time::Instant::now());
    assert!(host.next_wake_in().is_some());
    let timeout_text = doc
        .get_element_by_id("timeout")
        .and_then(|id| doc.get_node(id))
        .map(|n| n.text_content())
        .unwrap_or_default();
    assert_eq!(timeout_text.trim(), "fired");
}

#[test]
fn url_search_params_iterates_and_mutates() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
      <script>
        var p = new URLSearchParams('a=1&b=two');
        var pairs = [];
        p.forEach(function (value, name) { pairs.push(name + '=' + value); });
        var entries = Array.from(p.entries()).map(function (e) { return e[0] + ':' + e[1]; });
        p.set('c', '3');
        p['delete']('a');
        document.getElementById('t').textContent =
          pairs.join(',') + '|' + entries.join(',') + '|' + p.size + '|' + p.has('a') +
          '|' + Array.from(p).map(function (e) { return e[0]; }).join(',');
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(60));
    assert_eq!(result.final_text, "a=1,b=two|a:1,b:two|2|false|b,c");
}

#[test]
fn document_cookie_get_set_visible_to_scripts() {
    use std::sync::Arc;
    let _jar_guard = crate::cookies::CookieJar::lock_global_for_test();
    crate::cookies::CookieJar::replace_global(Arc::new(crate::cookies::CookieJar::empty()));
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
      <script>
        document.cookie = 'a=1; path=/';
        document.getElementById('t').textContent = document.cookie;
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(80));
    assert_eq!(result.final_text, "a=1");
}

#[test]
fn document_cookie_expired_assignment_does_not_throw() {
    use std::sync::Arc;
    let _jar_guard = crate::cookies::CookieJar::lock_global_for_test();
    crate::cookies::CookieJar::replace_global(Arc::new(crate::cookies::CookieJar::empty()));
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
      <script>
        var name = 'exp_' + Math.random().toString(36).slice(2);
        try {
          document.cookie = name + '=; expires=Thu, 01 Jan 1970 00:00:00 GMT; Path=/';
          document.getElementById('t').textContent = 'ok';
        } catch (e) {
          document.getElementById('t').textContent = 'threw: ' + e.message;
        }
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(80));
    assert_eq!(result.final_text, "ok");
}

#[test]
fn custom_element_upgrade_runs_lifecycle_callbacks() {
    let html = r#"<!DOCTYPE html><html><head><style>
      x-pick:not(:defined){visibility:hidden}</style></head><body>
      <x-pick id="ce">before</x-pick>
      <p id="t">pending</p>
      <script>
        function XPick() {}
        XPick.prototype = Object.create(HTMLElement.prototype);
        XPick.prototype.constructor = XPick;
        XPick.prototype.connectedCallback = function () {
          this.textContent = 'upgraded';
        };
        XPick.observedAttributes = ['data-v'];
        XPick.prototype.attributeChangedCallback = function (n, o, v) {
          this.setAttribute('data-seen', v);
        };
        customElements.define('x-pick', XPick);
        document.getElementById('ce').setAttribute('data-v', '42');
        document.getElementById('t').textContent = document.getElementById('ce').textContent;
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(250));
    assert_eq!(result.final_text, "upgraded");
}

#[test]
fn custom_element_is_customized_builtin_is_rejected() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
      <script>
        var ok = false;
        try {
          customElements.define('x-iso', function(){}, { extends: 'button' });
        } catch (e) {
          ok = e.name === 'NotSupportedError';
        }
        document.getElementById('t').textContent = ok ? 'rejected' : 'bad';
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(120));
    assert_eq!(result.final_text, "rejected");
}

#[test]
fn custom_element_module_define_upgrades_widget() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/custom-element-module.html");
    let url = format!("file://{}", path.display());
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    let _guard = rt.enter();
    let text = crate::headless::dump_text(&url, true).expect("load");
    assert!(text.contains("module-widget"), "{text}");
}

#[test]
fn custom_element_mutation_observer_attribute_ordering() {
    let html = r#"<!DOCTYPE html><html><body>
      <x-ob></x-ob><p id="t"></p>
      <script>
        var order = [];
        function XO() {}
        XO.prototype = Object.create(HTMLElement.prototype);
        XO.observedAttributes = ['msg'];
        XO.prototype.attributeChangedCallback = function(n, o, v) {
          order.push('ce:' + o + '>' + v);
        };
        customElements.define('x-ob', XO);
        var ob = new MutationObserver(function(recs) {
          for (var i = 0; i < recs.length; i++) {
            var r = recs[i];
            if (r.type === 'attributes') order.push('mo:' + r.attributeName);
          }
        });
        ob.observe(document.querySelector('x-ob'), { attributes: true, attributeOldValue: true });
        document.querySelector('x-ob').setAttribute('msg', '2');
        document.getElementById('t').textContent = order.join(',');
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(250));
    assert!(
        result.final_text.contains("ce:null>2"),
        "attributeChanged old value: {}",
        result.final_text
    );
}

#[test]
fn custom_element_es6_class_connected_callback() {
    let html = r#"<!DOCTYPE html><html><body>
      <x-el id="a">start</x-el><p id="t"></p>
      <script>
        class XEl extends HTMLElement {
          connectedCallback() { this.textContent = 'ok'; }
        }
        customElements.define('x-el', XEl);
        document.getElementById('t').textContent = document.getElementById('a').textContent;
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "ok");
}

#[test]
fn document_cookie_rejected_assignment_does_not_throw() {
    use std::sync::Arc;
    let _jar_guard = crate::cookies::CookieJar::lock_global_for_test();
    crate::cookies::CookieJar::replace_global(Arc::new(crate::cookies::CookieJar::empty()));
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
      <script>
        try {
          document.cookie = 'a=1; Domain=example.com; Path=/';
          document.getElementById('t').textContent = 'ok';
        } catch (e) {
          document.getElementById('t').textContent = 'threw: ' + e.message;
        }
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(80));
    assert_eq!(result.final_text, "ok");
}

#[test]
fn custom_element_class_explicit_super_upgrades() {
    let html = r#"<!DOCTYPE html><html><body>
      <x-el id="a"></x-el>
      <p id="t"></p>
      <script>
        class XEl extends HTMLElement {
          constructor() { super(); }
          connectedCallback() { this.textContent = 'ok'; }
        }
        customElements.define('x-el', XEl);
        document.getElementById('t').textContent =
          document.getElementById('a').textContent;
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "ok");
}

#[test]
fn custom_element_class_super_sets_dataset_on_upgrade() {
    let html = r#"<!DOCTYPE html><html><body>
      <x-el id="a"></x-el>
      <p id="t"></p>
      <script>
        class XEl extends HTMLElement {
          constructor() {
            super();
            this.dataset.ok = '1';
          }
          connectedCallback() {}
        }
        customElements.define('x-el', XEl);
        document.getElementById('t').textContent =
          document.getElementById('a').dataset.ok || '';
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "1");
}

#[test]
fn custom_element_constructor_returning_other_node_fails_upgrade() {
    let html = r#"<!DOCTYPE html><html><body>
      <x-bad id="a"></x-bad><x-good id="b"></x-good><p id="t"></p>
      <script>
        function Bad(){ return document.createElement('div'); }
        function Good(){ this.textContent = 'ok'; }
        Good.prototype = Object.create(HTMLElement.prototype);
        Good.prototype.constructor = Good;
        customElements.define('x-bad', Bad);
        customElements.define('x-good', Good);
        document.getElementById('t').textContent =
          document.getElementById('a').localName + '|' + document.getElementById('b').textContent;
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "x-bad|ok");
}

#[test]
fn custom_element_create_element_runs_ctor_before_connect() {
    let html = r#"<!DOCTYPE html><html><body><p id="t"></p>
      <script>
        var log = [];
        function XA() { log.push('ctor'); this.textContent = 'ctor-text'; }
        XA.prototype = Object.create(HTMLElement.prototype);
        XA.prototype.constructor = XA;
        XA.prototype.connectedCallback = function () { log.push('conn'); };
        customElements.define('x-a', XA);
        var el = document.createElement('x-a');
        document.getElementById('t').textContent = log.join(',');
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(150));
    assert_eq!(result.final_text, "ctor");
}

#[test]
fn custom_element_connection_callbacks_follow_insert_remove_insert() {
    let html = r#"<!DOCTYPE html><html><body><div id="host"></div><p id="t"></p>
      <script>
        var log = [];
        function XW() {}
        XW.prototype = Object.create(HTMLElement.prototype);
        XW.prototype.connectedCallback = function(){ log.push('c'); };
        XW.prototype.disconnectedCallback = function(){ log.push('d'); };
        customElements.define('x-w', XW);
        var el = document.createElement('x-w');
        document.getElementById('host').appendChild(el);
        document.getElementById('host').removeChild(el);
        document.body.appendChild(el);
        document.getElementById('t').textContent = log.join('');
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "cdc");
}

#[test]
fn custom_element_remove_parent_fires_disconnected_on_child() {
    let html = r#"<!DOCTYPE html><html><body><div id="box"><x-c></x-c></div><p id="t"></p>
      <script>
        var n = 0;
        function XC() {}
        XC.prototype = Object.create(HTMLElement.prototype);
        XC.prototype.disconnectedCallback = function(){ n += 1; };
        customElements.define('x-c', XC);
        document.body.removeChild(document.getElementById('box'));
        document.getElementById('t').textContent = String(n);
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(150));
    assert_eq!(result.final_text, "1");
}

#[test]
fn custom_element_move_between_parents_fires_disconnect_then_connect() {
    let html = r#"<!DOCTYPE html><html><body>
      <x-m id="m"></x-m><div id="b"></div><p id="t"></p>
      <script>
        var log = [];
        function XM() {}
        XM.prototype = Object.create(HTMLElement.prototype);
        XM.prototype.disconnectedCallback = function(){ log.push('d'); };
        XM.prototype.connectedCallback = function(){ log.push('c'); };
        customElements.define('x-m', XM);
        log.length = 0;
        document.getElementById('b').appendChild(document.getElementById('m'));
        document.getElementById('t').textContent = log.join('');
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "dc");
}

#[test]
fn custom_element_define_upgrade_reaction_order() {
    let html = r#"<!DOCTYPE html><html><body>
      <x-a msg="1"></x-a><x-b msg="2"></x-b><p id="t"></p>
      <script>
        var log = [];
        function mk(tag) {
          function T() { log.push(tag + '-ctor'); }
          T.prototype = Object.create(HTMLElement.prototype);
          T.observedAttributes = ['msg'];
          T.prototype.attributeChangedCallback = function(n,o,v) {
            log.push(tag + '-attr:' + n + ':' + o + ':' + v);
          };
          T.prototype.connectedCallback = function(){ log.push(tag + '-conn'); };
          return T;
        }
        customElements.define('x-a', mk('a'));
        customElements.define('x-b', mk('b'));
        document.getElementById('t').textContent = log.join(', ');
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(250));
    assert_eq!(
        result.final_text,
        "a-ctor, a-attr:msg:null:1, a-conn, b-ctor, b-attr:msg:null:2, b-conn"
    );
}

#[test]
fn custom_element_attribute_changed_uses_old_value() {
    let html = r#"<!DOCTYPE html><html><body>
      <x-a msg="1"></x-a><p id="t"></p>
      <script>
        var log = [];
        function XA() {}
        XA.prototype = Object.create(HTMLElement.prototype);
        XA.observedAttributes = ['msg'];
        XA.prototype.attributeChangedCallback = function(n,o,v) {
          log.push('a-attr:' + n + ':' + o + ':' + v);
        };
        customElements.define('x-a', XA);
        document.querySelector('x-a').setAttribute('msg', '9');
        document.getElementById('t').textContent = log.join(', ');
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "a-attr:msg:null:1, a-attr:msg:1:9");
}

#[test]
fn custom_element_unobserved_attribute_skips_callback() {
    let html = r#"<!DOCTYPE html><html><body>
      <x-a></x-a><p id="t"></p>
      <script>
        var n = 0;
        function XA() {}
        XA.prototype = Object.create(HTMLElement.prototype);
        XA.observedAttributes = ['msg'];
        XA.prototype.attributeChangedCallback = function(){ n++; };
        customElements.define('x-a', XA);
        document.querySelector('x-a').setAttribute('other', '1');
        document.getElementById('t').textContent = String(n);
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(150));
    assert_eq!(result.final_text, "0");
}

#[test]
fn custom_element_reused_constructor_throws_not_supported() {
    let html = r#"<!DOCTYPE html><html><body><p id="t"></p>
      <script>
        function C() {}
        C.prototype = Object.create(HTMLElement.prototype);
        customElements.define('x-a', C);
        var err = '';
        try {
          customElements.define('x-b', C);
        } catch (e) {
          err = e.name;
        }
        document.getElementById('t').textContent =
          err + '|' + (customElements.get('x-a') === C ? 'yes' : 'no');
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(120));
    assert_eq!(result.final_text, "NotSupportedError|yes");
}

#[test]
fn custom_element_when_defined_invalid_name_syntax_error() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
      <script>
        var done = false;
        Promise.all([
          customElements.whenDefined('div'),
          customElements.whenDefined('X-A'),
        ]).then(function(){ document.getElementById('t').textContent = 'resolved'; })
          .catch(function(){ document.getElementById('t').textContent = 'rejected'; });
        queueMicrotask(function(){ done = true; });
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(250));
    assert_eq!(result.final_text, "rejected");
}

#[test]
fn custom_element_throwing_connected_still_upgrades_sibling() {
    let html = r#"<!DOCTYPE html><html><body>
      <x-bad></x-bad><x-ok></x-ok><p id="t"></p>
      <script>
        function Bad() {}
        Bad.prototype = Object.create(HTMLElement.prototype);
        Bad.prototype.connectedCallback = function(){ throw new Error('boom'); };
        function Ok() { this.textContent = 'fine'; }
        Ok.prototype = Object.create(HTMLElement.prototype);
        customElements.define('x-bad', Bad);
        customElements.define('x-ok', Ok);
        document.getElementById('t').textContent = document.querySelector('x-ok').textContent;
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "fine");
}

#[test]
fn custom_element_unicode_and_invalid_names() {
    let html = r#"<!DOCTYPE html><html><body><p id="t"></p>
      <script>
        var reserved = false, unicode = false;
        try { customElements.define('annotation-xml', function(){}); } catch (e) { reserved = true; }
        try {
          customElements.define('x-é', function(){});
          unicode = true;
        } catch (e) {}
        try { customElements.define('a-', function(){}); } catch (e) {}
        document.getElementById('t').textContent = (reserved ? 'r' : '') + '|' + (unicode ? 'u' : '');
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(120));
    assert_eq!(result.final_text, "r|u");
}

#[test]
fn websocket_loopback_echo_and_close() {
    let server_rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("server runtime");
    let (ws_url, _server) = server_rt.block_on(super::websocket::test_support::test_server(None));
    let html = format!(
        r#"<!DOCTYPE html><html><body><p id="t">pending</p>
      <script>
        var ws = new WebSocket({url:?});
        var log = [];
        ws.onopen = function() {{ ws.send('hello'); }};
        ws.onmessage = function(ev) {{ log.push('msg:' + ev.data); ws.close(1000, 'done'); }};
        ws.onclose = function(ev) {{
          log.push('close:' + ev.code + ':' + ev.reason + ':' + ev.wasClean);
          document.getElementById('t').textContent = log.join('|');
        }};
      </script></body></html>"#,
        url = ws_url
    );
    let page_rt = super::harness::harness_tokio_runtime();
    let result = super::harness::run_inline_page_with_runtime_on_base(
        &html,
        Duration::from_millis(1200),
        page_rt.handle(),
        80,
        "http://example.test/",
    );
    let text = result.final_text;
    assert!(text.contains("msg:hello"), "{text}");
    assert!(text.contains("close:1000:done:true"), "{text}");
}

#[test]
fn websocket_close_validation_leaves_socket_open() {
    let server_rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("server runtime");
    let (ws_url, _server) = server_rt.block_on(super::websocket::test_support::test_server(None));
    let html = format!(
        r#"<!DOCTYPE html><html><body><p id="t">pending</p>
      <script>
        var ws = new WebSocket({url:?});
        var out = [];
        ws.onopen = function() {{
          try {{ ws.close(1005); }} catch (e) {{ out.push('e1005:' + e.name); }}
          out.push('c1005:' + ws.readyState);
          try {{ ws.close(1000, 'x'.repeat(200)); }} catch (e) {{ out.push('elong:' + e.name); }}
          out.push('long:' + ws.readyState);
        }};
        ws.onclose = function() {{ out.push('closed'); }};
        setTimeout(function() {{
          document.getElementById('t').textContent = out.join('|');
        }}, 800);
      </script></body></html>"#,
        url = ws_url
    );
    let page_rt = super::harness::harness_tokio_runtime();
    let result = super::harness::run_inline_page_with_runtime_on_base(
        &html,
        Duration::from_millis(2500),
        page_rt.handle(),
        100,
        "http://example.test/",
    );
    let text = result.final_text;
    assert!(text.contains("e1005:InvalidAccessError"), "{text}");
    assert!(text.contains("elong:SyntaxError"), "{text}");
    assert!(text.contains("c1005:1"), "{text}");
}

#[test]
fn websocket_binary_blob_echo() {
    let server_rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("server runtime");
    let (ws_url, _server) = server_rt.block_on(super::websocket::test_support::test_server(None));
    let html = format!(
        r#"<!DOCTYPE html><html><body><p id="t">pending</p>
      <script>
        var ws = new WebSocket({url:?});
        ws.binaryType = 'blob';
        ws.onopen = function() {{
          ws.send(new Blob([new Uint8Array([1, 2, 3])]));
        }};
        ws.onmessage = function(ev) {{
          var kind = ev.data instanceof Blob ? 'blob' : 'other';
          ev.data.arrayBuffer().then(function(buf) {{
            var b = new Uint8Array(buf);
            document.getElementById('t').textContent = kind + ':' + b[0] + ',' + b[1] + ',' + b[2];
          }});
        }};
      </script></body></html>"#,
        url = ws_url
    );
    let page_rt = super::harness::harness_tokio_runtime();
    let result = super::harness::run_inline_page_with_runtime_on_base(
        &html,
        Duration::from_millis(1500),
        page_rt.handle(),
        80,
        "http://example.test/",
    );
    assert_eq!(result.final_text, "blob:1,2,3");
}

#[test]
fn websocket_url_fragment_and_default_port() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
      <script>
        var out = [];
        try { new WebSocket('ws://example.test/#frag'); } catch (e) { out.push('frag:' + e.name); }
        try {
          var w = new WebSocket('ws://example.test:80/path');
          out.push('url:' + w.url);
        } catch (e) { out.push('urlerr:' + e.name); }
        document.getElementById('t').textContent = out.join('|');
      </script></body></html>"#;
    let result = super::harness::run_inline_page_on_base(
        html,
        "http://example.test/",
        Duration::from_millis(300),
    );
    let text = result.final_text;
    assert!(text.contains("frag:SyntaxError"), "{text}");
    assert!(text.contains("url:ws://example.test/path"), "{text}");
}

#[test]
fn websocket_binary_arraybuffer_roundtrip_200kb() {
    let server_rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("server runtime");
    let (ws_url, _server) = server_rt.block_on(super::websocket::test_support::test_server(None));
    let html = format!(
        r#"<!DOCTYPE html><html><body><p id="t">pending</p>
      <script>
        var ws = new WebSocket({url:?});
        ws.binaryType = 'arraybuffer';
        ws.onopen = function() {{
          var buf = new Uint8Array(200 * 1024);
          for (var i = 0; i < buf.length; i++) buf[i] = i & 255;
          ws.send(buf);
        }};
        ws.onmessage = function(ev) {{
          var b = new Uint8Array(ev.data);
          var ok = b.length === 200 * 1024 && b[0] === 0 && b[b.length - 1] === ((b.length - 1) & 255);
          document.getElementById('t').textContent = ok ? 'ok:' + b.length : 'bad:' + b.length;
        }};
      </script></body></html>"#,
        url = ws_url
    );
    let page_rt = super::harness::harness_tokio_runtime();
    let result = super::harness::run_inline_page_with_runtime_on_base(
        &html,
        Duration::from_millis(4000),
        page_rt.handle(),
        120,
        "http://example.test/",
    );
    assert!(
        result.final_text.starts_with("ok:204800"),
        "{}",
        result.final_text
    );
}

#[test]
fn websocket_protocol_negotiation() {
    let server_rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("server runtime");
    let (ws_url, _server) =
        server_rt.block_on(super::websocket::test_support::test_server(Some("chat")));
    let html = format!(
        r#"<!DOCTYPE html><html><body><p id="t">pending</p>
      <script>
        var out = [];
        try {{ new WebSocket({url:?}, ['a','a']); }} catch (e) {{ out.push('dup:' + e.name); }}
        try {{ new WebSocket({url:?}, ''); }} catch (e) {{ out.push('empty:' + e.name); }}
        var ws = new WebSocket({url:?}, ['chat', 'other']);
        ws.onopen = function() {{ document.getElementById('t').textContent = out.join('|') + '|proto:' + ws.protocol; }};
      </script></body></html>"#,
        url = ws_url
    );
    let page_rt = super::harness::harness_tokio_runtime();
    let result = super::harness::run_inline_page_with_runtime_on_base(
        &html,
        Duration::from_millis(1500),
        page_rt.handle(),
        80,
        "http://example.test/",
    );
    let text = result.final_text;
    assert!(text.contains("dup:SyntaxError"), "{text}");
    assert!(text.contains("empty:SyntaxError"), "{text}");
    assert!(text.contains("proto:chat"), "{text}");
}

#[test]
fn websocket_rejects_mixed_content_and_bad_url() {
    let html = r#"<!DOCTYPE html><html><body><p id="t">pending</p>
      <script>
        var out = [];
        try { new WebSocket('ws://insecure.test/'); } catch (e) { out.push('mixed:' + e.name); }
        try { new WebSocket('https://bad.example/ws'); } catch (e) { out.push('bad:' + e.name); }
        document.getElementById('t').textContent = out.join('|');
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    let text = result.final_text;
    assert!(text.contains("mixed:SecurityError"), "{text}");
    assert!(text.contains("bad:SyntaxError"), "{text}");
}

#[test]
fn intersection_observer_fires_after_dom_moves_target_into_view() {
    let html = r#"<!DOCTYPE html><html><body>
      <div id="target" style="width:40px;height:40px;margin-top:1200px"></div>
      <p id="t">pending</p>
      <script>
        var sawAfterMove = false;
        var io = new IntersectionObserver(function (entries) {
          entries.forEach(function (e) {
            if (e.target.id === 'target' && e.isIntersecting) sawAfterMove = true;
          });
        });
        io.observe(document.getElementById('target'));
        setTimeout(function () {
          document.getElementById('target').style.marginTop = '0px';
        }, 120);
        setTimeout(function () {
          document.getElementById('t').textContent = sawAfterMove ? 'intersected' : 'pending';
        }, 700);
      </script></body></html>"#;
    let result =
        super::harness::run_inline_page_in_viewport(html, 400, 300, Duration::from_millis(1000));
    assert_eq!(result.final_text, "intersected", "{}", result.final_text);
}

#[test]
fn intersection_observer_threshold_crossing_fires() {
    let html = r#"<!DOCTYPE html><html><body>
      <div id="half" style="width:100px;height:100px;margin-top:200px"></div>
      <p id="t">pending</p>
      <script>
        var ratios = [];
        var io = new IntersectionObserver(function (entries) {
          entries.forEach(function (e) { ratios.push(e.intersectionRatio.toFixed(2)); });
        }, { threshold: [0, 0.5, 1] });
        io.observe(document.getElementById('half'));
        setTimeout(function () {
          document.getElementById('t').textContent = ratios.join(',');
        }, 80);
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(300));
    assert!(
        !result.final_text.is_empty() && result.final_text != "pending",
        "expected threshold callback, got {}",
        result.final_text
    );
}

#[test]
fn intersection_observer_take_records_buffers_entries() {
    let html = r#"<!DOCTYPE html><html><body>
      <div id="box" style="width:10px;height:10px"></div>
      <p id="t">pending</p>
      <script>
        var delivered = 0;
        var io = new IntersectionObserver(function () { delivered += 1; });
        io.observe(document.getElementById('box'));
        var rec = io.takeRecords();
        document.getElementById('t').textContent =
          rec.length + '|' + (rec[0] && rec[0].target.id) + '|' + delivered;
      </script></body></html>"#;
    let result = super::harness::run_inline_page(html, Duration::from_millis(200));
    assert_eq!(result.final_text, "1|box|0", "{}", result.final_text);
}

#[test]
fn lazy_img_fetches_only_after_near_viewport_layout_change() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    let requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let requests2 = requests.clone();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for _ in 0..16 {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut buf = [0u8; 4096];
            let Ok(n) = stream.read(&mut buf) else {
                continue;
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            let path = req.split_whitespace().nth(1).unwrap_or("").to_string();
            requests2.lock().expect("lock").push(path.clone());
            let (status, body, extra) = if path.ends_with(".png") {
                (
                    "200 OK",
                    vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
                    "Content-Type: image/png\r\n",
                )
            } else {
                ("404 Not Found", Vec::new(), "")
            };
            let resp = format!(
                "HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.write_all(&body);
        }
    });

    let html_move = format!(
        r#"<!DOCTYPE html><html><body>
<img id="lz" width="32" height="32" loading="lazy" style="margin-top:1200px" src="http://127.0.0.1:{port}/pixel.png" alt="">
<p id="t">pending</p>
<script>
setTimeout(function() {{ document.getElementById('lz').style.marginTop = '0px'; }}, 120);
</script>
</body></html>"#
    );
    super::harness::run_inline_page_in_viewport(&html_move, 400, 300, Duration::from_millis(1600));
    let paths = requests.lock().expect("lock").clone();
    assert!(
        paths.iter().any(|p| p.contains("pixel.png")),
        "lazy image should load after moving near viewport: {paths:?}"
    );
}
