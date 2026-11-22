//! JS host on the UI thread: DOM RPC drain, script scheduling, frame pump.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use blitz_dom::BaseDocument;
use blitz_net::Provider;
use blitz_traits::events::{BlitzImeEvent, UiEvent};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TryRecvError, bounded, unbounded};

use super::apply::apply_request;
use super::bridge::{DomRequest, DomResponse, bubble_handles};
use super::console_feed::{self, ConsoleWireEvent};
use super::paint_prune::prune_stale_paint_children;
use super::scripts::{
    ScriptSource, ScriptSpec, collect_scripts_for_page, execution_plan, source_body,
    spawn_resolve_sources,
};
use super::snapshot;
use super::tap::JsTap;
use super::worker::{EvalReply, JsWorker, WAKE_NONE};
use crate::logging;
use crate::tabs::TabId;

pub type JsWaker = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub(crate) struct HostWake {
    waker: Arc<std::sync::Mutex<Option<JsWaker>>>,
    requested: Arc<AtomicBool>,
}

impl HostWake {
    fn new() -> Self {
        Self {
            waker: Arc::new(std::sync::Mutex::new(None)),
            requested: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn request(&self) {
        // Coalesce: `sync` clears the flag, so only the first request per batch
        // has to cross the thread boundary.
        if self.requested.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Ok(guard) = self.waker.lock()
            && let Some(waker) = guard.as_ref()
        {
            waker();
        }
    }
}

pub struct DomRpc {
    pub generation: u64,
    pub request: DomRequest,
    pub reply: Sender<DomResponse>,
}

pub struct JsHost {
    enabled: Arc<AtomicBool>,
    worker: Option<JsWorker>,
    dom_tx: Sender<DomRpc>,
    dom_rx: Receiver<DomRpc>,
    dom_generation: u64,
    tap: JsTap,
    base_url: String,
    net: Option<Arc<Provider>>,
    started_for: Option<(usize, String)>,
    stale_page_doc_id: Option<usize>,
    clock: Instant,
    pending_sources: Option<Receiver<Vec<ScriptSource>>>,
    pending_indices: Option<(Vec<usize>, Vec<usize>, Vec<usize>)>,
    script_specs: Vec<ScriptSpec>,
    console_tab_id: TabId,
    console_page_epoch: u64,
    host_wake: Arc<HostWake>,
}

impl JsHost {
    pub fn new(enabled: bool, tap: JsTap, net: Option<Arc<Provider>>) -> Self {
        let (dom_tx, dom_rx) = unbounded();
        Self {
            enabled: Arc::new(AtomicBool::new(enabled)),
            worker: None,
            dom_tx,
            dom_rx,
            dom_generation: 1,
            tap,
            base_url: String::new(),
            net,
            started_for: None,
            stale_page_doc_id: None,
            clock: Instant::now(),
            pending_sources: None,
            pending_indices: None,
            script_specs: Vec::new(),
            console_tab_id: 1,
            console_page_epoch: 1,
            host_wake: Arc::new(HostWake::new()),
        }
    }

    pub fn set_waker(&self, waker: JsWaker) {
        if let Ok(mut slot) = self.host_wake.waker.lock() {
            *slot = Some(waker);
        }
    }

    pub fn take_wake_request(&self) -> bool {
        self.host_wake.requested.swap(false, Ordering::SeqCst)
    }

    pub fn set_console_route(&mut self, tab_id: TabId, page_epoch: u64) {
        let changed = self.console_tab_id != tab_id || self.console_page_epoch != page_epoch;
        self.console_tab_id = tab_id;
        self.console_page_epoch = page_epoch;
        if changed && let Some(worker) = &self.worker {
            worker.set_console_route(tab_id, page_epoch);
        }
    }

    /// Console messages and exceptions from the worker, not yet merged into a tab journal.
    pub fn drain_console_wire(&mut self) -> Vec<ConsoleWireEvent> {
        console_feed::drain_pending()
    }

    pub fn page_js_active(&self) -> bool {
        self.is_enabled() && self.worker.is_some()
    }

    /// When the page has a pending timer or animation frame, time until the earliest deadline.
    pub fn next_wake_in(&self) -> Option<Duration> {
        if !self.page_js_active() {
            return None;
        }
        let worker = self.worker.as_ref()?;
        if worker.is_stopped() {
            return None;
        }
        let deadline = worker.wake_deadline_ms();
        if deadline == WAKE_NONE {
            return None;
        }
        let now = self.now_ms();
        // A page can schedule a very distant deadline; cap what we hand to winit
        // (the loop re-arms on every event, so a later wake is harmless).
        const MAX_WAIT_MS: u64 = 24 * 60 * 60 * 1000;
        let ms = deadline.saturating_sub(now).min(MAX_WAIT_MS);
        Some(Duration::from_millis(ms).max(Duration::from_millis(1)))
    }

    pub fn submit_console_eval(&self, source: &str) -> Result<(), String> {
        self.submit_eval(source, None)
    }

    /// Submit an eval, optionally asking the worker to reply on `reply`.
    pub fn submit_eval(&self, source: &str, reply: Option<EvalReply>) -> Result<(), String> {
        self.submit_eval_at(source, self.console_tab_id, self.console_page_epoch, reply)
    }

    /// Submit an eval tagged with an explicit console route (tests exercise stale routes).
    pub(crate) fn submit_eval_at(
        &self,
        source: &str,
        tab_id: u64,
        page_epoch: u64,
        reply: Option<EvalReply>,
    ) -> Result<(), String> {
        if !self.is_enabled() {
            return Err("Console REPL unavailable: JavaScript is disabled.".to_string());
        }
        let Some(worker) = &self.worker else {
            return Err("Console REPL unavailable: no active page is loaded.".to_string());
        };
        worker.eval_console(source.to_string(), tab_id, page_epoch, reply);
        Ok(())
    }

    /// Default timeout for [`JsHost::eval_blocking`]: the REPL watchdog plus two seconds.
    pub fn default_eval_timeout() -> Duration {
        Duration::from_millis(super::repl_eval::watchdog_duration_ms().saturating_add(2_000))
    }

    /// Evaluate on the worker and pump the host until the reply arrives or `timeout` passes.
    ///
    /// Pumping is mandatory: an eval that touches the DOM blocks in the worker's DOM
    /// sync RPC until this host drains it. The timeout only stops waiting; it does not
    /// cancel the eval, which may still finish on the worker and publish to the console.
    pub fn eval_blocking(
        &mut self,
        doc: &mut BaseDocument,
        source: &str,
        timeout: Duration,
    ) -> Result<String, String> {
        let (tx, rx) = bounded(1);
        self.submit_eval(source, Some(tx))?;
        let deadline = Instant::now() + timeout;
        loop {
            match rx.try_recv() {
                Ok(outcome) => return outcome,
                Err(TryRecvError::Disconnected) => {
                    return Err("eval reply channel closed before the worker answered.".to_string());
                }
                Err(TryRecvError::Empty) => {}
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "eval timed out after {:.1}s waiting for the reply",
                    timeout.as_secs_f64()
                ));
            }
            self.pump(doc, Instant::now());
            doc.resolve(0.0);
            std::thread::sleep(Duration::from_millis(10));
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
        self.dom_generation = self.dom_generation.wrapping_add(1);
        self.worker = None;
        self.started_for = None;
        self.pending_sources = None;
        self.pending_indices = None;
        self.script_specs.clear();
        // Answer anything the retired worker already queued so it is not left
        // blocked in `recv_timeout`; the next drain would reject it anyway.
        while let Ok(rpc) = self.dom_rx.try_recv() {
            let reply = super::apply::reply_id(&rpc.request);
            let _ = rpc.reply.send(DomResponse::err(reply, "page detached"));
        }
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
        if self.worker.is_some() {
            self.reset_page_js_state();
        }
        let specs = with_page_subdoc(chrome, |doc| collect_scripts_for_page(doc, Some(page_url)))
            .unwrap_or_else(|| collect_scripts_for_page(chrome, Some(page_url)));
        log_discovered_scripts(page_url, &specs);
        self.script_specs = specs.clone();
        self.base_url = page_url.to_string();
        self.started_for = Some((current_doc_id, page_url.to_string()));
        self.stale_page_doc_id = None;
        self.clock = Instant::now();

        let worker = JsWorker::spawn(
            self.dom_tx.clone(),
            self.dom_generation,
            Arc::clone(&self.host_wake),
        );
        let snapshot_json = serde_json::to_string(
            &with_page_subdoc(chrome, snapshot::snapshot_tree)
                .unwrap_or_else(|| snapshot::snapshot_tree(chrome)),
        )
        .unwrap_or_else(|_| "[]".into());
        worker.init_page(
            snapshot_json,
            page_url.to_string(),
            self.console_tab_id,
            self.console_page_epoch,
        );
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
        let pick = |idxs: &[usize]| -> Vec<(String, Option<u64>, bool)> {
            idxs.iter()
                .filter_map(|&i| {
                    let spec = specs.get(i);
                    source_body(&sources[i]).map(|body| {
                        (
                            body,
                            spec.and_then(|s| s.element),
                            spec.is_some_and(|s| s.module),
                        )
                    })
                })
                .collect()
        };
        let classic_bodies = pick(&classic);
        if !classic_bodies.is_empty() {
            worker.run_scripts(classic_bodies, "classic");
        }
        let defer_bodies = pick(&defer);
        if !defer_bodies.is_empty() {
            worker.run_scripts(defer_bodies, "defer");
        }
        worker.fire_dom_content_loaded();
        for idx in async_scripts {
            if let Some(body) = source_body(&sources[idx]) {
                let spec = specs.get(idx);
                let script_id = spec.and_then(|s| s.element);
                let module = spec.is_some_and(|s| s.module);
                worker.run_scripts(vec![(body, script_id, module)], "async");
            }
        }
        worker.page_settled();
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
                    redraw |= self.handle_dom_rpc(chrome, rpc);
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

    /// Drain DOM RPC until `idle` passes with no messages or `max` elapses (headless session).
    pub fn flush_dom_rpc_until_idle(
        &mut self,
        chrome: &mut BaseDocument,
        max: Duration,
        idle: Duration,
    ) -> bool {
        if !self.is_enabled() {
            return false;
        }
        let deadline = Instant::now() + max;
        let mut last_rpc = Instant::now();
        let mut redraw = false;
        while Instant::now() < deadline {
            self.try_finish_pending_sources();
            match self.dom_rx.recv_timeout(Duration::from_millis(25)) {
                Ok(rpc) => {
                    redraw |= self.handle_dom_rpc(chrome, rpc);
                    last_rpc = Instant::now();
                }
                Err(RecvTimeoutError::Timeout) => {
                    if last_rpc.elapsed() >= idle {
                        break;
                    }
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
            redraw |= self.handle_dom_rpc(chrome, rpc);
        }
        redraw
    }

    fn apply_dom_rpc(&self, chrome: &mut BaseDocument, request: DomRequest) -> DomResponse {
        if let Some(page) = page_subdoc_mut(chrome) {
            apply_request(page, &self.base_url, request)
        } else {
            apply_request(chrome, &self.base_url, request)
        }
    }

    /// Dispatch one DOM request. `Fetch` goes to tokio and replies later, so a
    /// slow page request never stops the UI thread (or the winit event loop).
    fn handle_dom_rpc(&self, chrome: &mut BaseDocument, rpc: DomRpc) -> bool {
        if rpc.generation != self.dom_generation {
            let reply_id = super::apply::reply_id(&rpc.request);
            let _ = rpc.reply.send(DomResponse::err(reply_id, "page detached"));
            return false;
        }
        match rpc.request {
            DomRequest::Fetch(fetch) => {
                super::apply::start_fetch(self.net.clone(), &self.base_url, fetch, rpc.reply);
                false
            }
            request => {
                let response = self.apply_dom_rpc(chrome, request);
                let _ = rpc.reply.send(response);
                true
            }
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
    chrome: &mut BaseDocument,
    worker: &Option<JsWorker>,
    evt: &UiEvent,
    redraw: &mut bool,
) {
    let Some(worker) = worker else {
        return;
    };
    if let Some(inner) = page_subdoc_mut(chrome) {
        if let Some((target, ty)) = ui_event_target(evt, inner) {
            worker.dispatch_dom_bubble(bubble_handles(inner, target), ty);
            *redraw = true;
        }
        return;
    }
    if let Some((target, ty)) = ui_event_target(evt, chrome) {
        worker.dispatch_dom_bubble(bubble_handles(chrome, target), ty);
        *redraw = true;
    }
}

/// Prune stale paint/stacking entries, resolve layout, then hit test. JS mutations
/// drain asynchronously; without the prune, a click can hit a list that still names a
/// removed node and panic.
fn hit_test(doc: &mut BaseDocument, x: f32, y: f32) -> Option<blitz_traits::events::HitResult> {
    let pruned = prune_stale_paint_children(doc);
    if pruned > 0 {
        logging::warn(
            "dom.hit",
            "pruned stale paint children before hit test",
            &[
                ("doc_id", doc.id().to_string()),
                ("count", pruned.to_string()),
                (
                    "hint",
                    "A removed node stayed in a paint or stacking list; Blitz would panic while hit testing it."
                        .into(),
                ),
            ],
        );
    }
    doc.resolve(0.0);
    doc.hit(x, y)
}

fn ui_event_target(
    evt: &UiEvent,
    doc: &mut BaseDocument,
) -> Option<(blitz_traits::node_id::NodeId, &'static str)> {
    match evt {
        UiEvent::PointerUp(ptr) => {
            let hit = hit_test(doc, ptr.coords.client_x, ptr.coords.client_y)?;
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

/// One record of what a page's markup contained, before any script runs.
fn log_discovered_scripts(page_url: &str, specs: &[ScriptSpec]) {
    if specs.is_empty() {
        return;
    }
    let inline = specs.iter().filter(|s| s.inline.is_some()).count();
    let external = specs.len() - inline;
    let deferred = specs.iter().filter(|s| s.defer).count();
    let async_count = specs.iter().filter(|s| s.r#async).count();
    logging::debug(
        "js.page",
        "page scripts discovered",
        &[
            ("page_url", page_url.to_string()),
            ("scripts", specs.len().to_string()),
            ("inline", inline.to_string()),
            ("external", external.to_string()),
            ("defer", deferred.to_string()),
            ("async", async_count.to_string()),
        ],
    );
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
    use std::sync::Arc;
    use std::sync::Mutex;

    use super::*;

    fn worker_test_lock() -> (
        std::sync::MutexGuard<'static, ()>,
        std::sync::MutexGuard<'static, ()>,
    ) {
        static WORKER: Mutex<()> = Mutex::new(());
        (
            WORKER
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            crate::js::console_feed::test_console_serial_lock(),
        )
    }

    fn poll_until(polls: usize, sleep: Duration, mut done: impl FnMut() -> bool) -> bool {
        for _ in 0..polls {
            if done() {
                return true;
            }
            std::thread::sleep(sleep);
        }
        false
    }
    use crate::js::bridge::DomValue;
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
    fn stale_paint_child_is_pruned_before_hit_test() {
        let html = concat!(
            "<!DOCTYPE html><html><body>",
            "<div id=\"p\"><div id=\"c\">child</div></div>",
            "</body></html>"
        );
        let mut doc = parse::document_from_html(html, DocumentConfig::default(), true);
        doc.resolve(0.0);
        let parent = doc.get_element_by_id("p").expect("parent");
        let child = doc.get_element_by_id("c").expect("child");
        assert!(
            doc.get_node(parent)
                .expect("parent node")
                .paint_children
                .borrow()
                .as_ref()
                .is_some_and(|children| children.contains(&child))
        );

        doc.mutate().remove_and_drop_node(child);
        assert!(doc.get_node(child).is_none());
        // The child is listed in both the layout and paint lists, so prune reports
        // both; the assertions below pin down the paint entry specifically.
        assert!(prune_stale_paint_children(&mut doc) >= 1);
        assert!(
            !doc.get_node(parent)
                .expect("parent node")
                .paint_children
                .borrow()
                .as_ref()
                .is_some_and(|children| children.contains(&child))
        );
        let _ = hit_test(&mut doc, 1.0, 1.0);
    }

    #[test]
    fn paint_after_removed_positioned_node_does_not_panic() {
        let html = concat!(
            "<!DOCTYPE html><html><head><style>",
            "#p { position: relative; width: 100px; height: 100px; }",
            "#c { position: absolute; z-index: 1; width: 50px; height: 50px; }",
            "</style></head><body>",
            "<div id=\"p\"><div id=\"c\">child</div></div>",
            "</body></html>"
        );
        let mut doc = parse::document_from_html(html, DocumentConfig::default(), true);
        doc.resolve(0.0);
        let child = doc.get_element_by_id("c").expect("child");
        doc.mutate().remove_and_drop_node(child);
        assert!(doc.get_node(child).is_none());
        assert!(
            prune_stale_paint_children(&mut doc) >= 1,
            "expected a stale hoisted stacking entry"
        );
        // Hit before the next resolve: this is the frame that panicked without the
        // prune, because the stale entry is only replaced when layout is rebuilt.
        let _ = doc.hit(10.0, 10.0);
        doc.resolve(0.0);
        let _ = doc.hit(10.0, 10.0);
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
        let (_worker, _console) = worker_test_lock();
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
    fn maybe_start_without_navigation_bumps_generation() {
        let (_worker, _console) = worker_test_lock();
        let doc_a = page_doc(true);
        let mut doc_b = page_doc(true);
        let url_a = "https://example.test/a";
        let url_b = "https://example.test/b";
        let rt = rt();
        let handle = rt.handle();
        let tap = JsTap::new_enabled();
        let mut host = JsHost::new(true, tap, None);
        host.maybe_start_page(&doc_a, url_a, handle);
        assert!(
            poll_until(100, Duration::from_millis(2), || host.worker.is_some()),
            "worker should start for document A"
        );
        let stale_generation = host.dom_generation;

        host.maybe_start_page(&doc_b, url_b, handle);
        assert!(
            host.dom_generation > stale_generation,
            "expected a new generation for document B"
        );

        let (reply_tx, reply_rx) = bounded(1);
        let rpc = DomRpc {
            generation: stale_generation,
            request: DomRequest::Navigate {
                url: "https://stale.example/".into(),
                reply: 1,
            },
            reply: reply_tx,
        };
        assert!(host.dom_tx.send(rpc).is_ok());
        assert!(!host.drain_dom_rpc(&mut doc_b));
        let response = reply_rx.try_recv().expect("stale rpc reply");
        assert!(matches!(response.value, DomValue::Error(_)));
    }

    #[test]
    fn stale_dom_rpc_is_rejected() {
        let (_worker, _console) = worker_test_lock();
        let mut doc = page_doc(true);
        let url = "https://example.test/page";
        let rt = rt();
        let handle = rt.handle();
        let tap = JsTap::new_enabled();
        let mut host = JsHost::new(true, tap, None);
        host.maybe_start_page(&doc, url, handle);
        let stale_generation = host.dom_generation;
        host.on_navigation(&doc, url);
        assert!(host.worker.is_none());

        let (reply_tx, reply_rx) = bounded(1);
        let rpc = DomRpc {
            generation: stale_generation,
            request: DomRequest::Navigate {
                url: "https://stale.example/".into(),
                reply: 1,
            },
            reply: reply_tx,
        };
        assert!(host.dom_tx.send(rpc).is_ok());
        assert!(!host.drain_dom_rpc(&mut doc));
        let response = reply_rx.try_recv().expect("stale rpc reply");
        assert!(matches!(response.value, DomValue::Error(_)));
    }

    #[test]
    fn current_dom_rpc_is_applied() {
        let (_worker, _console) = worker_test_lock();
        let mut doc = page_doc(true);
        let url = "https://example.test/page";
        let rt = rt();
        let handle = rt.handle();
        let tap = JsTap::new_enabled();
        let mut host = JsHost::new(true, tap, None);
        host.maybe_start_page(&doc, url, handle);
        host.pump(&mut doc, Instant::now());

        let (reply_tx, reply_rx) = bounded(1);
        let rpc = DomRpc {
            generation: host.dom_generation,
            request: DomRequest::Navigate {
                url: "https://current.example/".into(),
                reply: 1,
            },
            reply: reply_tx,
        };
        assert!(host.dom_tx.send(rpc).is_ok());
        host.drain_dom_rpc(&mut doc);
        let response = reply_rx.try_recv().expect("current rpc reply");
        assert!(!matches!(response.value, DomValue::Error(_)));
    }

    #[test]
    fn page_without_timers_reports_no_wake() {
        let (_worker, _console) = worker_test_lock();
        let html = concat!(
            "<!DOCTYPE html><html><body>",
            "<p id=\"t\">ok</p><script>document.getElementById('t').textContent='still';</script>",
            "</body></html>"
        );
        let mut doc = parse::document_from_html(html, DocumentConfig::default(), true);
        let rt = rt();
        let handle = rt.handle();
        let tap = JsTap::new_enabled();
        let mut host = JsHost::new(true, tap, None);
        host.maybe_start_page(&doc, "https://example.test/", handle);
        let mut settled = false;
        for _ in 0..100 {
            host.pump(&mut doc, Instant::now());
            if host
                .worker
                .as_ref()
                .is_some_and(|w| w.wake_epoch() > 0 && w.wake_deadline_ms() == WAKE_NONE)
            {
                settled = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(settled, "settled page should report no pending wake");
        assert!(host.next_wake_in().is_none());
    }

    #[test]
    fn page_with_timer_reports_next_wake() {
        let (_worker, _console) = worker_test_lock();
        let html = concat!(
            "<!DOCTYPE html><html><body>",
            "<script>setTimeout(function(){}, 500);</script>",
            "</body></html>"
        );
        let mut doc = parse::document_from_html(html, DocumentConfig::default(), true);
        let rt = rt();
        let handle = rt.handle();
        let tap = JsTap::new_enabled();
        let mut host = JsHost::new(true, tap, None);
        host.maybe_start_page(&doc, "https://example.test/", handle);
        let mut saw_timer = false;
        for _ in 0..100 {
            host.pump(&mut doc, Instant::now());
            if host
                .worker
                .as_ref()
                .is_some_and(|w| w.wake_deadline_ms() != WAKE_NONE)
                && host.next_wake_in().is_some()
            {
                saw_timer = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(saw_timer, "500ms timer should publish a pending wake");
        let wake = host.next_wake_in().expect("pending timer");
        assert!(wake >= Duration::from_millis(100));
    }

    #[test]
    fn settimeout_zero_reports_immediate_wake() {
        let (_worker, _console) = worker_test_lock();
        let html = concat!(
            "<!DOCTYPE html><html><body>",
            "<script>setTimeout(function(){}, 0);</script>",
            "</body></html>"
        );
        let doc = parse::document_from_html(html, DocumentConfig::default(), true);
        let rt = rt();
        let handle = rt.handle();
        let tap = JsTap::new_enabled();
        let mut host = JsHost::new(true, tap, None);
        host.maybe_start_page(&doc, "https://example.test/", handle);
        assert!(
            poll_until(100, Duration::from_millis(2), || {
                host.worker
                    .as_ref()
                    .is_some_and(|w| w.wake_deadline_ms() == 0)
            }),
            "setTimeout(0) should publish a zero-ms wake deadline"
        );
        assert!(host.next_wake_in().is_some());
    }

    #[test]
    fn settimeout_negative_delay_reports_immediate_wake() {
        let (_worker, _console) = worker_test_lock();
        let html = concat!(
            "<!DOCTYPE html><html><body>",
            "<script>setTimeout(function(){}, -5);</script>",
            "</body></html>"
        );
        let doc = parse::document_from_html(html, DocumentConfig::default(), true);
        let rt = rt();
        let handle = rt.handle();
        let tap = JsTap::new_enabled();
        let mut host = JsHost::new(true, tap, None);
        host.maybe_start_page(&doc, "https://example.test/", handle);
        assert!(
            poll_until(100, Duration::from_millis(2), || {
                host.worker
                    .as_ref()
                    .is_some_and(|w| w.wake_deadline_ms() == 0)
            }),
            "a negative setTimeout delay is due now, not idle"
        );
        assert!(host.next_wake_in().is_some());
    }

    #[test]
    fn worker_wake_on_dom_rpc_without_host_pump() {
        use std::sync::atomic::Ordering;

        let (_worker, _console) = worker_test_lock();
        let html = concat!(
            "<!DOCTYPE html><html><head><title>t</title></head><body>",
            "<p id=\"t\">idle</p></body></html>"
        );
        let doc = parse::document_from_html(html, DocumentConfig::default(), true);
        let rt = rt();
        let handle = rt.handle();
        let tap = JsTap::new_enabled();
        let mut host = JsHost::new(true, tap, None);
        let wake_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let wake_probe = Arc::clone(&wake_count);
        host.set_waker(Arc::new(move || {
            wake_probe.fetch_add(1, Ordering::SeqCst);
        }));
        host.maybe_start_page(&doc, "https://example.test/", handle);
        assert!(
            poll_until(100, Duration::from_millis(2), || host.worker.is_some()),
            "worker should start"
        );
        host.submit_eval("document.title = 'wake';", None)
            .expect("eval");
        assert!(
            poll_until(100, Duration::from_millis(2), || {
                wake_count.load(Ordering::SeqCst) > 0
            }),
            "expected the injected waker to fire for a queued DOM RPC"
        );
    }

    #[test]
    fn earlier_timer_shortens_next_wake_and_wakes_the_host() {
        use std::sync::atomic::Ordering;

        let (_worker, _console) = worker_test_lock();
        let html = "<!DOCTYPE html><html><body><p id=\"t\">x</p></body></html>";
        let mut doc = parse::document_from_html(html, DocumentConfig::default(), true);
        let rt = rt();
        let handle = rt.handle();
        let tap = JsTap::new_enabled();
        let mut host = JsHost::new(true, tap, None);
        host.maybe_start_page(&doc, "https://example.test/", handle);
        assert!(
            poll_until(100, Duration::from_millis(2), || host.worker.is_some()),
            "worker should start"
        );
        host.pump(&mut doc, Instant::now());
        host.submit_eval("setTimeout(function(){}, 5000);", None)
            .expect("eval");
        assert!(
            poll_until(200, Duration::from_millis(2), || {
                host.next_wake_in()
                    .is_some_and(|w| w > Duration::from_secs(1))
            }),
            "expected the long timer to be pending"
        );

        // Clear the coalesced request from the long timer, then prove a shorter
        // timer wakes the host through the injected waker.
        let _ = host.take_wake_request();
        let wake_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let wake_probe = Arc::clone(&wake_count);
        host.set_waker(Arc::new(move || {
            wake_probe.fetch_add(1, Ordering::SeqCst);
        }));
        host.submit_eval("setTimeout(function(){}, 80);", None)
            .expect("eval");
        assert!(
            poll_until(200, Duration::from_millis(2), || {
                wake_count.load(Ordering::SeqCst) > 0
            }),
            "an earlier timer must wake the host"
        );
        assert!(
            host.next_wake_in()
                .is_some_and(|w| w < Duration::from_secs(2)),
            "expected the shorter timer to publish an earlier wake"
        );
    }

    #[test]
    fn set_enabled_off_then_on_restarts_for_new_document() {
        let (_worker, _console) = worker_test_lock();
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
