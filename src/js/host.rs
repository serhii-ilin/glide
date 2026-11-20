//! JS host on the UI thread: DOM RPC drain, script scheduling, frame pump.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use blitz_dom::BaseDocument;
use blitz_net::Provider;
use blitz_traits::events::{BlitzImeEvent, UiEvent};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, unbounded};

use super::apply::apply_request;
use super::bridge::{DomRequest, DomResponse, bubble_handles};
use super::scripts::{
    ScriptSource, ScriptSpec, collect_scripts, execution_plan, source_body, spawn_resolve_sources,
};
use super::snapshot;
use super::tap::JsTap;
use super::worker::JsWorker;

pub struct DomRpc {
    pub request: DomRequest,
    pub reply: Sender<DomResponse>,
}

pub struct JsHost {
    enabled: Arc<AtomicBool>,
    worker: Option<JsWorker>,
    dom_rx: Receiver<DomRpc>,
    tap: JsTap,
    base_url: String,
    net: Option<Arc<Provider>>,
    started_for: Option<(usize, String)>,
    stale_page_doc_id: Option<usize>,
    clock: Instant,
    pending_sources: Option<Receiver<Vec<ScriptSource>>>,
    pending_indices: Option<(Vec<usize>, Vec<usize>, Vec<usize>)>,
    script_specs: Vec<ScriptSpec>,
}

impl JsHost {
    pub fn new(enabled: bool, tap: JsTap, net: Option<Arc<Provider>>) -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(enabled)),
            worker: None,
            dom_rx: unbounded().1,
            tap,
            base_url: String::new(),
            net,
            started_for: None,
            stale_page_doc_id: None,
            clock: Instant::now(),
            pending_sources: None,
            pending_indices: None,
            script_specs: Vec::new(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&mut self, on: bool, chrome: &BaseDocument) {
        self.enabled.store(on, Ordering::Relaxed);
        self.mark_stale_page(chrome);
        self.reset_page_js_state();
    }

    fn reset_page_js_state(&mut self) {
        self.worker = None;
        self.started_for = None;
        self.pending_sources = None;
        self.pending_indices = None;
        self.script_specs.clear();
    }

    fn mark_stale_page(&mut self, chrome: &BaseDocument) {
        self.stale_page_doc_id = Some(page_document_id(chrome));
    }

    pub fn now_ms(&self) -> u64 {
        self.clock.elapsed().as_millis() as u64
    }

    pub fn maybe_start_page(
        &mut self,
        chrome: &BaseDocument,
        page_url: &str,
        rt: &tokio::runtime::Handle,
    ) {
        if !self.is_enabled() {
            return;
        }
        let current_doc_id = page_document_id(chrome);
        if !should_start_page(
            self.stale_page_doc_id,
            self.started_for.as_ref(),
            current_doc_id,
            page_url,
        ) {
            return;
        }
        let specs =
            with_page_subdoc(chrome, collect_scripts).unwrap_or_else(|| collect_scripts(chrome));
        self.script_specs = specs.clone();
        self.base_url = page_url.to_string();
        self.started_for = Some((current_doc_id, page_url.to_string()));
        self.stale_page_doc_id = None;
        self.clock = Instant::now();

        let (dom_tx, dom_rx) = unbounded();
        self.dom_rx = dom_rx;
        let worker = JsWorker::spawn(dom_tx);
        let snapshot_json = serde_json::to_string(
            &with_page_subdoc(chrome, snapshot::snapshot_tree)
                .unwrap_or_else(|| snapshot::snapshot_tree(chrome)),
        )
        .unwrap_or_else(|_| "[]".into());
        worker.init_page(snapshot_json, page_url.to_string());
        self.worker = Some(worker);

        let plan = execution_plan(&specs);
        self.pending_indices = Some(plan);

        if let Some(net) = &self.net {
            let specs_clone = specs;
            self.pending_sources = Some(spawn_resolve_sources(
                specs_clone,
                Arc::clone(net),
                self.base_url.clone(),
                rt.clone(),
            ));
        } else {
            let sources: Vec<ScriptSource> = specs
                .iter()
                .map(|s| ScriptSource::Inline(s.inline.clone().unwrap_or_default()))
                .collect();
            self.start_from_sources(&sources);
        }
    }

    fn start_from_sources(&mut self, sources: &[ScriptSource]) {
        let Some((classic, defer, async_scripts)) = self.pending_indices.take() else {
            return;
        };
        let worker = match &self.worker {
            Some(w) => w,
            None => return,
        };
        let specs = self.script_specs.clone();
        let pick = |idxs: &[usize]| -> Vec<(String, Option<u64>)> {
            idxs.iter()
                .filter_map(|&i| {
                    source_body(&sources[i])
                        .map(|body| (body, specs.get(i).and_then(|s| s.element)))
                })
                .collect()
        };
        let classic_bodies = pick(&classic);
        if !classic_bodies.is_empty() {
            worker.run_scripts(classic_bodies);
        }
        let defer_bodies = pick(&defer);
        if !defer_bodies.is_empty() {
            worker.run_scripts(defer_bodies);
        }
        worker.fire_dom_content_loaded();
        for idx in async_scripts {
            if let Some(body) = source_body(&sources[idx]) {
                let script_id = specs.get(idx).and_then(|s| s.element);
                worker.run_scripts(vec![(body, script_id)]);
            }
        }
    }

    fn try_finish_pending_sources(&mut self) {
        let Some(rx) = &self.pending_sources else {
            return;
        };
        if let Ok(sources) = rx.try_recv() {
            self.pending_sources = None;
            self.start_from_sources(&sources);
        }
    }

    pub fn pump(&mut self, chrome: &mut BaseDocument, _frame: Instant) -> bool {
        if !self.is_enabled() {
            return false;
        }
        self.try_finish_pending_sources();

        let mut redraw = self.drain_dom_rpc(chrome);

        if let Some(worker) = &self.worker {
            worker.pump(self.now_ms());
            worker.flush_raf();
        }

        for evt in self.tap.drain() {
            dispatch_tap_event(chrome, &self.worker, &evt, &mut redraw);
        }
        redraw
    }

    pub fn flush_dom_rpc(&mut self, chrome: &mut BaseDocument, budget: Duration) -> bool {
        if !self.is_enabled() {
            return false;
        }
        let deadline = Instant::now() + budget;
        let mut redraw = false;
        while Instant::now() < deadline {
            self.try_finish_pending_sources();
            match self.dom_rx.recv_timeout(Duration::from_millis(25)) {
                Ok(rpc) => {
                    let response = self.apply_dom_rpc(chrome, rpc.request);
                    let _ = rpc.reply.send(response);
                    redraw = true;
                }
                Err(RecvTimeoutError::Timeout) => {
                    if let Some(worker) = &self.worker {
                        worker.pump(self.now_ms());
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        redraw | self.drain_dom_rpc(chrome)
    }

    fn drain_dom_rpc(&mut self, chrome: &mut BaseDocument) -> bool {
        let mut redraw = false;
        while let Ok(rpc) = self.dom_rx.try_recv() {
            let response = self.apply_dom_rpc(chrome, rpc.request);
            let _ = rpc.reply.send(response);
            redraw = true;
        }
        redraw
    }

    fn apply_dom_rpc(&self, chrome: &mut BaseDocument, request: DomRequest) -> DomResponse {
        if let Some(page) = page_subdoc_mut(chrome) {
            apply_request(page, self.net.as_ref(), &self.base_url, request)
        } else {
            apply_request(chrome, self.net.as_ref(), &self.base_url, request)
        }
    }

    pub fn on_navigation(&mut self, chrome: &BaseDocument, _url: &str) {
        self.mark_stale_page(chrome);
        self.reset_page_js_state();
    }

    #[cfg(test)]
    pub(crate) fn started_for_doc(&self) -> Option<(usize, String)> {
        self.started_for.clone()
    }

    /// Headless/tests: resolve script sources on a worker thread and run when ready.
    #[cfg(test)]
    pub fn finish_script_resolution_sync(
        &mut self,
        rt: &tokio::runtime::Handle,
        specs: &[super::scripts::ScriptSpec],
    ) {
        if self.pending_sources.is_none()
            && self.pending_indices.is_some()
            && let Some(net) = &self.net
        {
            let sources = super::scripts::resolve_sources(specs, net, &self.base_url, rt);
            self.start_from_sources(&sources);
        }
    }
}

fn dispatch_tap_event(
    chrome: &BaseDocument,
    worker: &Option<JsWorker>,
    evt: &UiEvent,
    redraw: &mut bool,
) {
    let Some(worker) = worker else {
        return;
    };
    if let Some(page_id) = chrome.get_element_by_id("page")
        && let Some(sub) = chrome.get_node(page_id).and_then(|n| n.subdoc())
    {
        let inner = sub.inner();
        if let Some((target, ty)) = ui_event_target(evt, &inner) {
            worker.dispatch_dom_bubble(bubble_handles(&inner, target), ty);
            *redraw = true;
        }
        return;
    }
    if let Some((target, ty)) = ui_event_target(evt, chrome) {
        worker.dispatch_dom_bubble(bubble_handles(chrome, target), ty);
        *redraw = true;
    }
}

fn ui_event_target(
    evt: &UiEvent,
    doc: &BaseDocument,
) -> Option<(blitz_traits::node_id::NodeId, &'static str)> {
    match evt {
        UiEvent::PointerUp(ptr) => {
            let hit = doc.hit(ptr.coords.client_x, ptr.coords.client_y)?;
            Some((hit.node_id, "click"))
        }
        UiEvent::Ime(BlitzImeEvent::Commit(_)) => {
            let focus = doc.get_focussed_node_id()?;
            Some((focus, "input"))
        }
        UiEvent::KeyDown(_) => {
            let focus = doc.get_focussed_node_id()?;
            if doc
                .get_node(focus)
                .and_then(|n| n.element_data())
                .is_some_and(|el| el.text_input_data().is_some())
            {
                Some((focus, "keydown"))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Whether the JS worker may start for this page sub-document and URL.
pub(crate) fn should_start_page(
    stale_page_doc_id: Option<usize>,
    started_for: Option<&(usize, String)>,
    current_doc_id: usize,
    page_url: &str,
) -> bool {
    if stale_page_doc_id == Some(current_doc_id) {
        return false;
    }
    !matches!(started_for, Some((id, url)) if *id == current_doc_id && url == page_url)
}

fn page_document_id(chrome: &BaseDocument) -> usize {
    with_page_subdoc(chrome, |doc| doc.id()).unwrap_or_else(|| chrome.id())
}

fn with_page_subdoc<R>(chrome: &BaseDocument, f: impl FnOnce(&BaseDocument) -> R) -> Option<R> {
    let page_id = chrome.get_element_by_id("page")?;
    let sub = chrome.get_node(page_id)?.subdoc()?;
    match sub.inner() {
        blitz_dom::DocGuard::Ref(doc) => Some(f(doc)),
        blitz_dom::DocGuard::RefCell(_)
        | blitz_dom::DocGuard::RwLock(_)
        | blitz_dom::DocGuard::Mutex(_) => None,
    }
}

fn page_subdoc_mut(chrome: &mut BaseDocument) -> Option<&mut BaseDocument> {
    let page_id = chrome.get_element_by_id("page")?;
    let sub = chrome.get_node_mut(page_id)?.subdoc_mut()?;
    match sub.inner_mut() {
        blitz_dom::DocGuardMut::Ref(doc) => Some(doc),
        blitz_dom::DocGuardMut::RefCell(_)
        | blitz_dom::DocGuardMut::RwLock(_)
        | blitz_dom::DocGuardMut::Mutex(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blitz_dom::DocumentConfig;

    use crate::parse;

    const PAGE: &str = r#"<!DOCTYPE html><html><body>
    <script>document.title = 'x';</script></body></html>"#;

    fn page_doc(scripting: bool) -> BaseDocument {
        let config = DocumentConfig {
            base_url: Some("https://example.test/page".into()),
            ..Default::default()
        };
        parse::document_from_html(PAGE, config, scripting)
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime")
    }

    #[test]
    fn should_start_rejects_stale_and_duplicate() {
        assert!(!should_start_page(Some(1), None, 1, "https://a/"));
        assert!(!should_start_page(
            None,
            Some(&(1, "https://a/".into())),
            1,
            "https://a/",
        ));
        assert!(should_start_page(
            Some(1),
            Some(&(1, "https://a/".into())),
            2,
            "https://a/",
        ));
        assert!(should_start_page(None, None, 5, "https://b/"));
    }

    #[test]
    fn restart_requires_new_subdocument_after_reload() {
        let doc_a = page_doc(true);
        let doc_b = page_doc(true);
        assert_ne!(doc_a.id(), doc_b.id());
        let url = "https://example.test/page";
        let rt = rt();
        let handle = rt.handle();
        let tap = JsTap::new_enabled();
        let mut host = JsHost::new(true, tap, None);

        host.maybe_start_page(&doc_a, url, handle);
        assert_eq!(host.started_for_doc().map(|(id, _)| id), Some(doc_a.id()));

        host.on_navigation(&doc_a, url);
        assert!(host.started_for_doc().is_none());

        host.maybe_start_page(&doc_a, url, handle);
        assert!(host.started_for_doc().is_none());

        host.maybe_start_page(&doc_b, url, handle);
        assert_eq!(host.started_for_doc().map(|(id, _)| id), Some(doc_b.id()));
    }

    #[test]
    fn set_enabled_off_then_on_restarts_for_new_document() {
        let doc_a = page_doc(true);
        let doc_b = page_doc(true);
        let url = "https://example.test/page";
        let rt = rt();
        let handle = rt.handle();
        let tap = JsTap::new_enabled();
        let mut host = JsHost::new(true, tap, None);

        host.maybe_start_page(&doc_a, url, handle);
        assert_eq!(host.started_for_doc().map(|(id, _)| id), Some(doc_a.id()));

        host.set_enabled(false, &doc_a);
        assert!(host.started_for_doc().is_none());

        host.maybe_start_page(&doc_a, url, handle);
        assert!(host.started_for_doc().is_none());

        host.set_enabled(true, &doc_a);
        host.maybe_start_page(&doc_a, url, handle);
        assert!(host.started_for_doc().is_none());

        host.maybe_start_page(&doc_b, url, handle);
        assert_eq!(host.started_for_doc().map(|(id, _)| id), Some(doc_b.id()));
    }
}
