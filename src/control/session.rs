//! Long-lived headless browsing session on a dedicated thread.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use blitz_dom::DocumentConfig;
use blitz_net::Provider;
use blitz_traits::net::NetProvider;
use blitz_traits::shell::{ColorScheme, Viewport};
use crossbeam_channel::{RecvTimeoutError, Sender, bounded, unbounded};

use super::ControlBackend;
use super::page_ops;
use super::protocol::{
    ControlRequest, ControlResult, DEFAULT_MAX_CHARS, DEFAULT_NAV_TIMEOUT_MS, DEFAULT_QUERY_LIMIT,
    LoadStatus, PageInfo,
};
use crate::devtools::console::{ConsoleEntry, ConsoleJournal};
use crate::headless::{self, SettleOptions, VIEWPORT_HEIGHT, VIEWPORT_WIDTH};
use crate::history::{History, HistoryEntry, HistoryRpcSnapshot, HistoryStep};
use crate::html::{GlideHtmlProvider, HtmlPageDocument};
use crate::js::console_feed;
use crate::js::{JsHost, JsTap};
use crate::location;
use crate::logging;
use crate::parse;
use crate::tabs::TabId;

const SESSION_TAB_BASE: u64 = 1_u64 << 32;
const SESSION_LOG_PAGE_URL_MAX: usize = 120;
const CONTROL_SESSION_TARGET: &str = "control.session";
const SESSION_PANIC_DETAIL_MAX: usize = 512;

static NEXT_SESSION_TAB: AtomicU64 = AtomicU64::new(SESSION_TAB_BASE);

fn next_session_tab_id() -> TabId {
    NEXT_SESSION_TAB.fetch_add(1, Ordering::Relaxed)
}

enum LoadHistory {
    Push,
    ReloadCurrent,
    Traverse,
}

/// Options for a new headless control session.
#[derive(Debug, Clone)]
pub struct SessionOptions {
    pub js_enabled: bool,
}

struct ControlEnvelope {
    request: ControlRequest,
    reply: Sender<Result<ControlResult, String>>,
}

/// Client handle that forwards [`ControlRequest`]s to the session worker thread.
pub struct LocalSessionBackend {
    tx: crossbeam_channel::Sender<ControlEnvelope>,
}

impl ControlBackend for LocalSessionBackend {
    fn call(&self, request: ControlRequest, timeout: Duration) -> Result<ControlResult, String> {
        let (reply_tx, reply_rx) = bounded(1);
        self.tx
            .send(ControlEnvelope {
                request,
                reply: reply_tx,
            })
            .map_err(|_| "glide session thread exited".to_string())?;
        match reply_rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Disconnected) => Err("glide session thread exited".to_string()),
            Err(RecvTimeoutError::Timeout) => {
                Err(format!("glide session did not answer within {:?}", timeout))
            }
        }
    }
}

/// Spawn the session worker thread and return a [`ControlBackend`] handle.
///
/// The owned Tokio [`Runtime`](tokio::runtime::Runtime) that produced `handle` must stay alive
/// for the whole session: a [`Handle`](tokio::runtime::Handle) clone does not keep the runtime
/// running after the runtime is dropped.
pub fn spawn_session(
    opts: SessionOptions,
    handle: tokio::runtime::Handle,
) -> Result<LocalSessionBackend, String> {
    let (tx, rx) = unbounded::<ControlEnvelope>();
    thread::Builder::new()
        .name("glide-session".into())
        .spawn(move || {
            let _g = handle.enter();
            let mut session = HeadlessSession::new(opts);
            loop {
                match rx.recv_timeout(Duration::from_millis(30)) {
                    Ok(envelope) => {
                        let method = request_method_name(&envelope.request);
                        let result = catch_session_op(&method, "request panicked", || {
                            session.handle(envelope.request)
                        });
                        let fields = session_request_log_fields(
                            &method,
                            if result.is_ok() { "ok" } else { "error" },
                            session.page_url_for_log().as_deref().unwrap_or("none"),
                        );
                        logging::debug(CONTROL_SESSION_TARGET, "handled request", &fields);
                        let _ = envelope.reply.send(result);
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        catch_session_tick(&mut session);
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .map_err(|err| err.to_string())?;
    Ok(LocalSessionBackend { tx })
}

fn panic_payload_string(payload: Box<dyn Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "panic".to_string()
    }
}

fn log_control_session_panic(method: &str, message: &str, detail: &str) {
    logging::error(
        CONTROL_SESSION_TARGET,
        message,
        &[
            ("method", method.to_string()),
            (
                "hint",
                "Blitz or JS worker fault; file a bug if this reproduces".into(),
            ),
            (
                "detail",
                console_feed::truncate_field(detail, SESSION_PANIC_DETAIL_MAX),
            ),
        ],
    );
}

fn catch_session_op<T, F>(method: &str, panic_message: &str, op: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    catch_unwind(AssertUnwindSafe(op)).unwrap_or_else(|payload| {
        let detail = panic_payload_string(payload);
        log_control_session_panic(method, panic_message, &detail);
        Err(format!("internal error: {detail}"))
    })
}

fn catch_session_tick(session: &mut HeadlessSession) {
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| session.tick())) {
        log_control_session_panic("tick", "tick panicked", &panic_payload_string(payload));
    }
}

pub(crate) fn session_request_log_fields(
    method: &str,
    status: &str,
    page_url: &str,
) -> Vec<(&'static str, String)> {
    vec![
        ("method", method.to_string()),
        ("status", status.to_string()),
        (
            "page_url",
            console_feed::truncate_field(page_url, SESSION_LOG_PAGE_URL_MAX),
        ),
    ]
}

fn request_method_name(request: &ControlRequest) -> String {
    match request {
        ControlRequest::Navigate(_) => "navigate",
        ControlRequest::Back(_) => "back",
        ControlRequest::Forward(_) => "forward",
        ControlRequest::Reload(_) => "reload",
        ControlRequest::PageInfo => "page_info",
        ControlRequest::SnapshotText(_) => "snapshot_text",
        ControlRequest::Dom(_) => "dom",
        ControlRequest::Query(_) => "query",
        ControlRequest::Screenshot => "screenshot",
        ControlRequest::ComputedStyle(_) => "computed_style",
    }
    .to_string()
}

/// Synchronous headless browser state for one agent control session.
pub struct HeadlessSession {
    opts: SessionOptions,
    document: Option<HtmlPageDocument>,
    js_host: Option<JsHost>,
    history: Option<Arc<Mutex<History>>>,
    console: ConsoleJournal,
    tab_id: TabId,
    epoch: u64,
    last_status: LoadStatus,
    current_url: String,
}

impl HeadlessSession {
    pub fn new(opts: SessionOptions) -> Self {
        crate::cookies::CookieJar::ensure_process_global(None);
        crate::web_storage::WebStorage::ensure_process_global(None);
        Self {
            opts,
            document: None,
            js_host: None,
            history: None,
            console: ConsoleJournal::default(),
            tab_id: next_session_tab_id(),
            epoch: 1,
            last_status: LoadStatus::Settled,
            current_url: String::new(),
        }
    }

    pub fn handle(&mut self, request: ControlRequest) -> Result<ControlResult, String> {
        match request {
            ControlRequest::Navigate(p) => {
                let info = self.navigate(&p.url, nav_timeout(p.timeout_ms))?;
                Ok(ControlResult::Page(info))
            }
            ControlRequest::Back(p) => {
                let info = self.back(nav_timeout(p.timeout_ms))?;
                Ok(ControlResult::Page(info))
            }
            ControlRequest::Forward(p) => {
                let info = self.forward(nav_timeout(p.timeout_ms))?;
                Ok(ControlResult::Page(info))
            }
            ControlRequest::Reload(p) => {
                let info = self.reload(nav_timeout(p.timeout_ms))?;
                Ok(ControlResult::Page(info))
            }
            ControlRequest::PageInfo => {
                let info = self.page_info()?;
                Ok(ControlResult::Page(info))
            }
            ControlRequest::SnapshotText(p) => {
                let doc = self.document.as_ref().ok_or_else(no_page_loaded)?;
                let max = p.max_chars.unwrap_or(DEFAULT_MAX_CHARS);
                let (text, truncated) = page_ops::snapshot_text(&doc.0, self.opts.js_enabled, max);
                Ok(ControlResult::Text { text, truncated })
            }
            ControlRequest::Dom(p) => {
                let doc = self.document.as_ref().ok_or_else(no_page_loaded)?;
                let max = p.max_chars.unwrap_or(DEFAULT_MAX_CHARS);
                let (html, matched, truncated) =
                    page_ops::dom_html(&doc.0, p.selector.as_deref(), p.all, max)?;
                Ok(ControlResult::Html {
                    html,
                    matched,
                    truncated,
                })
            }
            ControlRequest::Query(p) => {
                let doc = self.document.as_ref().ok_or_else(no_page_loaded)?;
                let limit = p.limit.unwrap_or(DEFAULT_QUERY_LIMIT);
                let elements = page_ops::query(&doc.0, &p.selector, limit, self.opts.js_enabled)?;
                Ok(ControlResult::Elements(elements))
            }
            ControlRequest::Screenshot => {
                let doc = self.document.as_mut().ok_or_else(no_page_loaded)?;
                doc.0.resolve(0.0);
                let png = page_ops::paint_png(&mut doc.0, VIEWPORT_WIDTH, VIEWPORT_HEIGHT)?;
                Ok(ControlResult::Png(png))
            }
            ControlRequest::ComputedStyle(p) => {
                let doc = self.document.as_mut().ok_or_else(no_page_loaded)?;
                doc.0.resolve(0.0);
                let map = crate::devtools::computed::computed_style_map_for_selector(
                    &doc.0,
                    &p.selector,
                )?;
                Ok(ControlResult::ComputedStyle(map))
            }
        }
    }

    pub fn tick(&mut self) {
        let Some(doc) = self.document.as_mut() else {
            return;
        };
        let Some(js_host) = self.js_host.as_mut() else {
            return;
        };
        doc.0.resolve(0.0);
        let _ = js_host.pump(&mut doc.0, Instant::now());
        self.drain_console_wire();
    }

    pub fn navigate(&mut self, raw_url: &str, timeout: Duration) -> Result<PageInfo, String> {
        let resolved = resolve_nav_url(raw_url);
        self.load_url(resolved, timeout, LoadHistory::Push)?;
        self.page_info()
    }

    pub fn back(&mut self, timeout: Duration) -> Result<PageInfo, String> {
        let history = self.history.as_ref().ok_or_else(no_page_loaded)?;
        let mut guard = history.lock().map_err(|_| "history lock poisoned")?;
        if guard.index() == 0 {
            return Err("no previous page".to_string());
        }
        let step = guard.back().ok_or_else(|| "no previous page".to_string())?;
        drop(guard);
        self.finish_history_step(step, timeout)
    }

    pub fn forward(&mut self, timeout: Duration) -> Result<PageInfo, String> {
        let history = self.history.as_ref().ok_or_else(no_page_loaded)?;
        let mut guard = history.lock().map_err(|_| "history lock poisoned")?;
        if guard.index() + 1 >= guard.entries().len() {
            return Err("no next page".to_string());
        }
        let step = guard.forward().ok_or_else(|| "no next page".to_string())?;
        drop(guard);
        self.finish_history_step(step, timeout)
    }

    fn finish_history_step(
        &mut self,
        step: HistoryStep,
        timeout: Duration,
    ) -> Result<PageInfo, String> {
        match step {
            HistoryStep::Document { url } => {
                match self.load_url(url, timeout, LoadHistory::Traverse) {
                    Ok(()) => self.page_info(),
                    Err(e) => {
                        if let Some(h) = self.history.as_ref()
                            && let Ok(mut guard) = h.lock()
                        {
                            let _ = guard.forward();
                        }
                        Err(e)
                    }
                }
            }
            HistoryStep::SameDocument {
                url,
                state,
                hash_changed,
            } => {
                self.current_url = url.clone();
                let snap = HistoryRpcSnapshot {
                    index: self
                        .history
                        .as_ref()
                        .and_then(|h| h.lock().ok())
                        .map(|g| g.index())
                        .unwrap_or(0),
                    length: self
                        .history
                        .as_ref()
                        .and_then(|h| h.lock().ok())
                        .map(|g| g.len())
                        .unwrap_or(1),
                    state,
                    url,
                    navigate: None,
                    hash_changed,
                };
                if let (Some(js_host), Some(doc)) = (self.js_host.as_mut(), self.document.as_mut())
                {
                    js_host.apply_history_step(&mut doc.0, &snap);
                }
                self.page_info()
            }
        }
    }

    pub fn reload(&mut self, timeout: Duration) -> Result<PageInfo, String> {
        if self.current_url.is_empty() {
            return Err(no_page_loaded());
        }
        let url = self.current_url.clone();
        self.load_url(url, timeout, LoadHistory::ReloadCurrent)?;
        self.page_info()
    }

    pub fn page_info(&self) -> Result<PageInfo, String> {
        let doc = self.document.as_ref().ok_or_else(no_page_loaded)?;
        Ok(self.build_page_info(&doc.0, &self.current_url))
    }

    fn page_url_for_log(&self) -> Option<String> {
        if self.current_url.is_empty() {
            None
        } else {
            Some(self.current_url.clone())
        }
    }

    fn load_url(
        &mut self,
        url: String,
        timeout: Duration,
        history: LoadHistory,
    ) -> Result<(), String> {
        self.load_url_at(url, timeout, history, Instant::now(), 0, None)
    }

    fn load_url_at(
        &mut self,
        url: String,
        timeout: Duration,
        history: LoadHistory,
        load_started: Instant,
        hop: u32,
        navigation_request: Option<blitz_traits::net::Request>,
    ) -> Result<(), String> {
        let handle = tokio::runtime::Handle::current();
        let net = Arc::new(Provider::new(None));
        let (final_url, bytes) = match navigation_request {
            Some(request) => {
                headless::fetch_page_with_request(request, self.opts.js_enabled, &handle, &net)?
            }
            None => headless::fetch_page(&url, self.opts.js_enabled, &handle, &net)?,
        };
        let html = String::from_utf8_lossy(&bytes);

        let tap = if self.opts.js_enabled {
            JsTap::new_enabled()
        } else {
            JsTap::new_disabled()
        };
        let html_provider = GlideHtmlProvider {
            scripting_enabled: Arc::new(AtomicBool::new(self.opts.js_enabled)),
            js_tap: tap.clone(),
        };
        let net_provider: Arc<dyn NetProvider> = net.clone();
        if self.history.is_none() {
            self.history = Some(Arc::new(Mutex::new(History::from_parts(
                vec![HistoryEntry::document_load(final_url.clone(), true)],
                0,
            ))));
        }
        let mut js_host = JsHost::new(self.opts.js_enabled, tap, Some(net.clone()));
        if let Some(stack) = &self.history {
            js_host.bind_session_history(Arc::clone(stack));
        }
        let navigation = Arc::new(headless::JsNavigation::default());

        let config = DocumentConfig {
            base_url: Some(final_url.clone()),
            net_provider: Some(net_provider),
            navigation_provider: Some(navigation.clone()),
            html_parser_provider: Some(Arc::new(html_provider)),
            viewport: Some(Viewport::new(
                VIEWPORT_WIDTH,
                VIEWPORT_HEIGHT,
                1.0,
                ColorScheme::Light,
            )),
            ..Default::default()
        };
        let mut document = HtmlPageDocument(parse::document_from_html(
            &html,
            config,
            self.opts.js_enabled,
        ));

        let new_epoch = self.epoch.saturating_add(1);
        js_host.set_console_route(self.tab_id, new_epoch);
        js_host.maybe_start_page(&document.0, &final_url, &handle);

        let mut settle_opts = SettleOptions {
            max: timeout,
            ..SettleOptions::default()
        };
        if timeout <= Duration::from_millis(1000) {
            settle_opts.quiet_ticks = 8;
        }
        let tab_id = self.tab_id;
        let journal = &mut self.console;
        let outcome = headless::settle(
            &mut document.0,
            &mut js_host,
            &net,
            self.opts.js_enabled,
            settle_opts,
            || {
                for ev in console_feed::drain_for_tab(tab_id) {
                    if ev.page_epoch == new_epoch {
                        journal.push(ConsoleEntry::from_wire(ev));
                    }
                }
            },
        );
        let last_status = if outcome.timed_out {
            LoadStatus::Timeout
        } else {
            LoadStatus::Settled
        };

        if self.opts.js_enabled {
            let remaining = timeout.saturating_sub(load_started.elapsed());
            let flush_budget = remaining.min(Duration::from_secs(4));
            if flush_budget > Duration::ZERO {
                js_host.flush_dom_rpc_until_idle(
                    &mut document.0,
                    flush_budget,
                    Duration::from_millis(100),
                );
            }
            document.0.resolve(0.0);
            for ev in console_feed::drain_for_tab(tab_id) {
                if ev.page_epoch == new_epoch {
                    journal.push(ConsoleEntry::from_wire(ev));
                }
            }
        }

        let (document, js_host) = if self.opts.js_enabled
            && !html.trim().is_empty()
            && !headless::has_visible_content(&document.0)
            && let Some((fallback, static_host)) =
                headless::static_fallback(&html, &final_url, &net, &navigation)
        {
            headless::log_static_fallback(&final_url);
            (fallback, static_host)
        } else {
            (document, js_host)
        };
        self.document = Some(document);
        self.js_host = Some(js_host);
        self.epoch = new_epoch;
        self.current_url = final_url.clone();
        self.last_status = last_status;

        // A page-script navigation (JavaScript challenge, `location.assign`)
        // replaces this page; give it the remaining budget and the caller's
        // history intent so only the final URL is recorded.
        if let Some(next) = navigation.take() {
            let remaining = timeout.saturating_sub(load_started.elapsed());
            if hop < headless::MAX_JS_NAVIGATIONS && remaining > Duration::from_millis(50) {
                let target = next.url.to_string();
                return self.load_url_at(
                    target,
                    remaining,
                    history,
                    load_started,
                    hop + 1,
                    Some(next),
                );
            }
        }

        match history {
            LoadHistory::Push => {
                if let Some(h) = self.history.as_ref() {
                    let mut guard = h.lock().map_err(|_| "history lock poisoned".to_string())?;
                    if guard.current_url() != final_url {
                        guard.push_document(final_url);
                    }
                }
            }
            LoadHistory::ReloadCurrent => {
                if let Some(h) = self.history.as_ref()
                    && let Ok(mut guard) = h.lock()
                    && guard.current_url() != final_url
                {
                    guard.replace_current_url(final_url);
                }
            }
            LoadHistory::Traverse => {}
        }

        Ok(())
    }

    fn build_page_info(&self, doc: &blitz_dom::BaseDocument, url: &str) -> PageInfo {
        let (can_go_back, can_go_forward) = self
            .history
            .as_ref()
            .and_then(|h| h.lock().ok())
            .map(|h| (h.index() > 0, h.index() + 1 < h.entries().len()))
            .unwrap_or((false, false));
        PageInfo {
            url: url.to_string(),
            title: page_ops::page_title(doc),
            loading: false,
            can_go_back,
            can_go_forward,
            js_enabled: self.opts.js_enabled,
            status: self.last_status,
        }
    }

    fn drain_console_wire(&mut self) {
        let batch = console_feed::drain_for_tab(self.tab_id);
        for ev in batch {
            if ev.page_epoch == self.epoch {
                self.console.push(ConsoleEntry::from_wire(ev));
            }
        }
    }
}

fn no_page_loaded() -> String {
    "no page loaded".to_string()
}

fn nav_timeout(timeout_ms: Option<u64>) -> Duration {
    Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_NAV_TIMEOUT_MS))
}

pub(crate) fn resolve_nav_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Ok(url) = url::Url::parse(trimmed) {
        match url.scheme() {
            "file" => return trimmed.to_string(),
            "http" | "https" | "data" | "about" => return url.to_string(),
            _ => {}
        }
    }
    location::resolve_location(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_request_log_fields_use_control_session_target_schema() {
        let fields = session_request_log_fields("navigate", "ok", "https://example.com/page");
        let map: std::collections::HashMap<_, _> = fields.into_iter().collect();
        assert_eq!(map.get("method").map(String::as_str), Some("navigate"));
        assert_eq!(map.get("status").map(String::as_str), Some("ok"));
        assert!(map.contains_key("page_url"));
        assert_eq!(CONTROL_SESSION_TARGET, "control.session");
    }

    #[test]
    fn session_request_log_truncates_long_page_urls() {
        let long = "https://example.com/".to_string() + &"a".repeat(200);
        let fields = session_request_log_fields("dom", "error", &long);
        let url = fields
            .into_iter()
            .find(|(k, _)| *k == "page_url")
            .map(|(_, v)| v)
            .expect("page_url");
        assert!(url.len() <= SESSION_LOG_PAGE_URL_MAX + 4);
        assert!(url.ends_with('…'));
    }

    #[test]
    fn resolve_nav_url_uses_address_bar_rules_for_bare_hosts() {
        assert_eq!(resolve_nav_url("localhost:3000"), "https://localhost:3000/");
        assert_eq!(
            resolve_nav_url("example.com:8080"),
            "https://example.com:8080/"
        );
        assert_eq!(resolve_nav_url("data:text/html,x"), "data:text/html,x");
        assert_eq!(
            resolve_nav_url("file:///tmp/a b.html"),
            "file:///tmp/a b.html"
        );
        let search = resolve_nav_url("some phrase");
        assert!(
            search.starts_with("https://duckduckgo.com/html/?q="),
            "{search}"
        );
    }

    #[test]
    fn resolve_nav_url_keeps_about_scheme() {
        assert_eq!(resolve_nav_url("about:blank"), "about:blank");
        assert_eq!(resolve_nav_url("about:blank#frag"), "about:blank#frag");
        assert_eq!(resolve_nav_url("about:blank?x=1"), "about:blank?x=1");
    }
}
