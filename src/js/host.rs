//! JS host on the UI thread: DOM RPC drain, script scheduling, frame pump.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use blitz_dom::BaseDocument;
use blitz_net::Provider;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, unbounded};

use super::apply::apply_request;
use super::bridge::DomResponse;
use super::bridge::{DomRequest, node_to_handle};
use super::scripts::{ScriptSource, ScriptSpec, collect_scripts, execution_plan, resolve_sources};
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
    }

    pub fn tap(&self) -> JsTap {
        self.tap.clone()
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
        let specs = with_page_subdoc(chrome, collect_scripts).unwrap_or_else(|| {
            collect_scripts(chrome)
        });
        self.base_url = page_url.to_string();
        self.started_for_url = Some(page_url.to_string());

        let (dom_tx, dom_rx) = unbounded();
        self.dom_rx = dom_rx;
        let worker = JsWorker::spawn(dom_tx);
        let plan = execution_plan(&specs);
        let sources = if let Some(net) = &self.net {
            resolve_sources(&specs, net, &self.base_url, rt)
        } else {
            specs
                .iter()
                .map(|s| {
                    ScriptSource::Inline(s.inline.clone().unwrap_or_default())
                })
                .collect()
        };

        let mut bundle = Vec::new();
        for batch in plan {
            for idx in batch {
                bundle.push(match &sources[idx] {
                    ScriptSource::Inline(s) => s.clone(),
                    ScriptSource::External { body, .. } => body.clone(),
                });
            }
        }
        worker.run_scripts(bundle);
        self.worker = Some(worker);
    }

    pub fn pump(&mut self, chrome: &mut BaseDocument, frame: Instant) -> bool {
        if !self.is_enabled() {
            return false;
        }
        let mut redraw = self.drain_dom_rpc(chrome);

        if let Some(worker) = &self.worker {
            worker.pump(frame.elapsed().as_millis() as u64);
            worker.flush_raf();
        }

        for evt in self.tap.drain() {
            if let Some(page_id) = chrome.get_element_by_id("page") {
                if let Some(node_id) = click_target_from_ui(&evt, chrome, page_id) {
                    if let Some(worker) = &self.worker {
                        worker.dispatch_dom_event(node_to_handle(node_id), "click");
                        redraw = true;
                    }
                }
            }
            let _ = evt;
        }
        redraw
    }

    /// Block until pending DOM RPCs are answered or `budget` elapses (headless screenshots).
    pub fn flush_dom_rpc(&mut self, chrome: &mut BaseDocument, budget: std::time::Duration) -> bool {
        if !self.is_enabled() {
            return false;
        }
        let deadline = Instant::now() + budget;
        let mut redraw = false;
        while Instant::now() < deadline {
            match self.dom_rx.recv_timeout(Duration::from_millis(25)) {
                Ok(rpc) => {
                    let response = self.apply_dom_rpc(chrome, rpc.request);
                    let _ = rpc.reply.send(response);
                    redraw = true;
                }
                Err(RecvTimeoutError::Timeout) => {
                    if let Some(worker) = &self.worker {
                        worker.pump(deadline.duration_since(Instant::now()).as_millis() as u64);
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

    fn apply_dom_rpc(
        &self,
        chrome: &mut BaseDocument,
        request: DomRequest,
    ) -> DomResponse {
        if let Some(page) = page_subdoc_mut(chrome) {
            apply_request(page, self.net.as_ref(), &self.base_url, request)
        } else {
            apply_request(chrome, self.net.as_ref(), &self.base_url, request)
        }
    }

    pub fn on_navigation(&mut self, _url: &str) {
        self.worker = None;
        self.started_for_url = None;
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

fn click_target_from_ui(
    evt: &blitz_traits::events::UiEvent,
    chrome: &BaseDocument,
    page_iframe: blitz_traits::node_id::NodeId,
) -> Option<blitz_traits::node_id::NodeId> {
    use blitz_traits::events::UiEvent;
    if chrome.get_focussed_node_id() != Some(page_iframe) {
        return None;
    }
    match evt {
        UiEvent::PointerUp(_) | UiEvent::KeyDown(_) => {
            let sub = chrome.get_node(page_iframe)?.subdoc()?;
            sub.inner().get_focussed_node_id()
        }
        _ => None,
    }
}