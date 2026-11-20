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
    ScriptSource, collect_scripts, execution_plan, source_body, spawn_resolve_sources,
};
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
    started_for_url: Option<String>,
    clock: Instant,
    pending_sources: Option<Receiver<Vec<ScriptSource>>>,
    pending_indices: Option<(Vec<usize>, Vec<usize>, Vec<usize>)>,
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
            started_for_url: None,
            clock: Instant::now(),
            pending_sources: None,
            pending_indices: None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
        if !on {
            self.worker = None;
        }
        self.started_for_url = None;
        self.pending_sources = None;
        self.pending_indices = None;
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
        if self.started_for_url.as_deref() == Some(page_url) {
            return;
        }
        let specs =
            with_page_subdoc(chrome, collect_scripts).unwrap_or_else(|| collect_scripts(chrome));
        self.base_url = page_url.to_string();
        self.started_for_url = Some(page_url.to_string());
        self.clock = Instant::now();

        let (dom_tx, dom_rx) = unbounded();
        self.dom_rx = dom_rx;
        let worker = JsWorker::spawn(dom_tx);
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
        let pick = |idxs: &[usize]| -> Vec<String> {
            idxs.iter()
                .filter_map(|&i| source_body(&sources[i]))
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
                worker.run_scripts(vec![body]);
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

    pub fn on_navigation(&mut self, _url: &str) {
        self.worker = None;
        self.started_for_url = None;
        self.pending_sources = None;
        self.pending_indices = None;
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
