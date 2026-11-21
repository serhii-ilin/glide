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
