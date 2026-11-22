//! Helpers for V8 integration tests (run on the test thread with a real worker).

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use blitz_dom::DocumentConfig;
use blitz_net::Provider;
use tokio::runtime::Handle;

use crate::html::{GlideHtmlProvider, HtmlPageDocument};
use crate::js::{JsHost, JsTap};
use crate::parse;

pub struct JsRunResult {
    pub final_text: String,
    pub console_wire: Vec<crate::js::console_feed::ConsoleWireEvent>,
}

pub fn run_inline_page(html: &str, pump_for: Duration) -> JsRunResult {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    run_inline_page_with_runtime(html, pump_for, rt.handle(), 0)
}

pub fn run_inline_page_with_extra_settle(
    html: &str,
    pump_for: Duration,
    extra_settle: u32,
) -> JsRunResult {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    run_inline_page_with_runtime(html, pump_for, rt.handle(), extra_settle)
}

fn run_inline_page_with_runtime(
    html: &str,
    pump_for: Duration,
    rt: &Handle,
    extra_settle: u32,
) -> JsRunResult {
    let _console_test_lock = crate::js::console_feed::test_console_serial_lock();
    let _ = crate::js::console_feed::drain_pending();

    let net = Arc::new(Provider::new(None));
    let tap = JsTap::new_enabled();
    let provider = GlideHtmlProvider {
        scripting_enabled: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        js_tap: tap.clone(),
    };
    let mut js_host = JsHost::new(true, tap, Some(net.clone()));
    js_host.set_console_route(0, 1);
    let config = DocumentConfig {
        base_url: Some("https://example.test/".into()),
        net_provider: Some(net.clone()),
        html_parser_provider: Some(Arc::new(provider)),
        ..Default::default()
    };
    let mut document = HtmlPageDocument(parse::document_from_html(html, config, true));
    js_host.maybe_start_page(&document.0, "https://example.test/", rt);
    js_host.finish_script_resolution_sync(rt, &crate::js::scripts::collect_scripts(&document.0));

    let start = Instant::now();
    while start.elapsed() < pump_for {
        document.0.resolve(0.0);
        js_host.pump(&mut document.0, Instant::now());
        thread::sleep(Duration::from_millis(5));
    }
    js_host.flush_dom_rpc(&mut document.0, Duration::from_millis(200));
    document.0.resolve(0.0);
    for _ in 0..extra_settle {
        js_host.pump(&mut document.0, Instant::now());
        thread::sleep(Duration::from_millis(5));
    }

    let final_text = document
        .0
        .get_element_by_id("t")
        .and_then(|id| document.0.get_node(id))
        .map(|n| n.text_content())
        .unwrap_or_default();

    let console_wire = js_host.drain_console_wire();

    JsRunResult {
        final_text,
        console_wire,
    }
}

/// Run inline HTML, then evaluate a Console REPL expression and drain wire events.
pub fn run_inline_page_with_repl(
    html: &str,
    expression: &str,
    pump_for: Duration,
) -> Result<Vec<crate::js::console_feed::ConsoleWireEvent>, String> {
    run_inline_page_with_repl_post(html, expression, pump_for, Duration::from_millis(80))
}

pub fn run_inline_page_with_repl_post(
    html: &str,
    expression: &str,
    pump_for: Duration,
    post_eval: Duration,
) -> Result<Vec<crate::js::console_feed::ConsoleWireEvent>, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let _console_test_lock = crate::js::console_feed::test_console_serial_lock();
    let _ = crate::js::console_feed::drain_pending();
    let net = Arc::new(Provider::new(None));
    let tap = JsTap::new_enabled();
    let provider = GlideHtmlProvider {
        scripting_enabled: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        js_tap: tap.clone(),
    };
    let mut js_host = JsHost::new(true, tap, Some(net.clone()));
    js_host.set_console_route(0, 1);
    let config = DocumentConfig {
        base_url: Some("https://example.test/".into()),
        net_provider: Some(net.clone()),
        html_parser_provider: Some(Arc::new(provider)),
        ..Default::default()
    };
    let mut document = HtmlPageDocument(parse::document_from_html(html, config, true));
    js_host.maybe_start_page(&document.0, "https://example.test/", rt.handle());
    js_host.finish_script_resolution_sync(
        rt.handle(),
        &crate::js::scripts::collect_scripts(&document.0),
    );
    let start = Instant::now();
    while start.elapsed() < pump_for {
        document.0.resolve(0.0);
        js_host.pump(&mut document.0, Instant::now());
        thread::sleep(Duration::from_millis(5));
    }
    js_host.submit_console_eval(expression)?;
    pump_repl_settle(&mut document.0, &mut js_host, post_eval);
    Ok(js_host.drain_console_wire())
}

fn pump_repl_settle(
    document: &mut blitz_dom::BaseDocument,
    js_host: &mut crate::js::JsHost,
    duration: Duration,
) {
    let post_start = Instant::now();
    while post_start.elapsed() < duration {
        document.resolve(0.0);
        js_host.pump(document, Instant::now());
        thread::sleep(Duration::from_millis(25));
    }
}

/// Owns one page document and JS host so tests can eval and then inspect the DOM.
pub struct EvalHarness {
    pub document: HtmlPageDocument,
    pub js_host: JsHost,
    _rt: tokio::runtime::Runtime,
    _console_test_lock: std::sync::MutexGuard<'static, ()>,
}

impl EvalHarness {
    pub fn new(html: &str, js_enabled: bool, pump_for: Duration) -> Self {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        let console_test_lock = crate::js::console_feed::test_console_serial_lock();
        let _ = crate::js::console_feed::drain_pending();
        let net = Arc::new(Provider::new(None));
        let tap = if js_enabled {
            JsTap::new_enabled()
        } else {
            JsTap::new_disabled()
        };
        let provider = GlideHtmlProvider {
            scripting_enabled: Arc::new(std::sync::atomic::AtomicBool::new(js_enabled)),
            js_tap: tap.clone(),
        };
        let mut js_host = JsHost::new(js_enabled, tap, Some(net.clone()));
        js_host.set_console_route(0, 1);
        let config = DocumentConfig {
            base_url: Some("https://example.test/".into()),
            net_provider: Some(net.clone()),
            html_parser_provider: Some(Arc::new(provider)),
            ..Default::default()
        };
        let mut document = HtmlPageDocument(parse::document_from_html(html, config, js_enabled));
        if js_enabled {
            js_host.maybe_start_page(&document.0, "https://example.test/", rt.handle());
            js_host.finish_script_resolution_sync(
                rt.handle(),
                &crate::js::scripts::collect_scripts(&document.0),
            );
        }
        let start = Instant::now();
        while start.elapsed() < pump_for {
            document.0.resolve(0.0);
            js_host.pump(&mut document.0, Instant::now());
            thread::sleep(Duration::from_millis(5));
        }
        js_host.flush_dom_rpc(&mut document.0, Duration::from_millis(200));
        Self {
            document,
            js_host,
            _rt: rt,
            _console_test_lock: console_test_lock,
        }
    }

    pub fn eval_blocking(&mut self, expression: &str) -> Result<String, String> {
        self.js_host.eval_blocking(
            &mut self.document.0,
            expression,
            JsHost::default_eval_timeout(),
        )
    }

    /// Submit an eval with its own reply channel and pump until the worker answers.
    pub fn submit_reply(
        &mut self,
        expression: &str,
        wait: Duration,
    ) -> Result<Result<String, String>, String> {
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.js_host.submit_eval(expression, Some(tx))?;
        let deadline = Instant::now() + wait;
        loop {
            match rx.try_recv() {
                Ok(outcome) => return Ok(outcome),
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    return Err("eval reply channel closed".to_string());
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
            }
            if Instant::now() >= deadline {
                return Err("eval reply timed out".to_string());
            }
            self.document.0.resolve(0.0);
            self.js_host.pump(&mut self.document.0, Instant::now());
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn element_text(&self, id: &str) -> Option<String> {
        self.document
            .0
            .get_element_by_id(id)
            .and_then(|node| self.document.0.get_node(node))
            .map(|node| node.text_content())
    }
}

/// Load inline HTML, evaluate a sequence of REPL expressions, then drain wire events.
pub fn run_inline_page_with_repl_sequence(
    html: &str,
    expressions: &[&str],
    pump_for: Duration,
    post_eval_each: Duration,
) -> Result<Vec<crate::js::console_feed::ConsoleWireEvent>, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let _console_test_lock = crate::js::console_feed::test_console_serial_lock();
    let _ = crate::js::console_feed::drain_pending();
    let net = Arc::new(Provider::new(None));
    let tap = JsTap::new_enabled();
    let provider = GlideHtmlProvider {
        scripting_enabled: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        js_tap: tap.clone(),
    };
    let mut js_host = JsHost::new(true, tap, Some(net.clone()));
    js_host.set_console_route(0, 1);
    let config = DocumentConfig {
        base_url: Some("https://example.test/".into()),
        net_provider: Some(net.clone()),
        html_parser_provider: Some(Arc::new(provider)),
        ..Default::default()
    };
    let mut document = HtmlPageDocument(parse::document_from_html(html, config, true));
    js_host.maybe_start_page(&document.0, "https://example.test/", rt.handle());
    js_host.finish_script_resolution_sync(
        rt.handle(),
        &crate::js::scripts::collect_scripts(&document.0),
    );
    let start = Instant::now();
    while start.elapsed() < pump_for {
        document.0.resolve(0.0);
        js_host.pump(&mut document.0, Instant::now());
        thread::sleep(Duration::from_millis(5));
    }
    for expression in expressions {
        js_host.submit_console_eval(expression)?;
        pump_repl_settle(&mut document.0, &mut js_host, post_eval_each);
    }
    Ok(js_host.drain_console_wire())
}
