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
    pub console: Vec<String>,
}

pub fn run_inline_page(html: &str, pump_for: Duration) -> JsRunResult {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    run_inline_page_with_runtime(html, pump_for, &rt.handle())
}

fn run_inline_page_with_runtime(html: &str, pump_for: Duration, rt: &Handle) -> JsRunResult {
    let net = Arc::new(Provider::new(None));
    let tap = JsTap::new_enabled();
    let provider = GlideHtmlProvider {
        scripting_enabled: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        js_tap: tap.clone(),
    };
    let mut js_host = JsHost::new(true, tap, Some(net.clone()));
    let config = DocumentConfig {
        base_url: Some("https://example.test/".into()),
        net_provider: Some(net.clone()),
        html_parser_provider: Some(Arc::new(provider)),
        ..Default::default()
    };
    let mut document = HtmlPageDocument(parse::document_from_html(html, config, true));
    js_host.maybe_start_page(&document.0, "https://example.test/", &rt);
    js_host.finish_script_resolution_sync(&rt, &crate::js::scripts::collect_scripts(&document.0));

    let start = Instant::now();
    while start.elapsed() < pump_for {
        document.0.resolve(0.0);
        js_host.pump(&mut document.0, Instant::now());
        thread::sleep(Duration::from_millis(5));
    }
    js_host.flush_dom_rpc(&mut document.0, Duration::from_millis(200));
    document.0.resolve(0.0);

    let final_text = document
        .0
        .get_element_by_id("t")
        .and_then(|id| document.0.get_node(id))
        .map(|n| n.text_content())
        .unwrap_or_default();

    JsRunResult {
        final_text,
        console: Vec::new(),
    }
}
