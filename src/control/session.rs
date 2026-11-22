//! Long-lived headless browsing session on a dedicated thread.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
use crate::history::History;
use crate::html::{GlideHtmlProvider, HtmlPageDocument};
use crate::js::console_feed;
use crate::js::{JsHost, JsTap};
use crate::location;
use crate::logging;
use crate::parse;
use crate::tabs::TabId;

const SESSION_TAB_BASE: u64 = 1_u64 << 32;
const SESSION_LOG_PAGE_URL_MAX: usize = 120;

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
                        let result =
                            catch_unwind(AssertUnwindSafe(|| session.handle(envelope.request)))
                                .unwrap_or_else(|payload| {
                                    let detail = if let Some(s) = payload.downcast_ref::<&str>() {
                                        (*s).to_string()
                                    } else if let Some(s) = payload.downcast_ref::<String>() {
                                        s.clone()
                                    } else {
                                        "panic".to_string()
                                    };
                                    logging::error(
                                "control.session",
                                "request panicked",
                                &[
                                    ("method", method.clone()),
                                    (
                                        "hint",
                                        "Blitz or JS worker fault; file a bug if this reproduces"
                                            .into(),
                                    ),
                                    ("detail", detail.clone()),
                                ],
                            );
                                    Err(format!("internal error: {detail}"))
                                });
                        let fields = session_request_log_fields(
                            &method,
                            if result.is_ok() { "ok" } else { "error" },
                            session.page_url_for_log().as_deref().unwrap_or("none"),
                        );
                        logging::debug("control.session", "handled request", &fields);
                        let _ = envelope.reply.send(result);
                    }
                    Err(RecvTimeoutError::Timeout) => session.tick(),
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .map_err(|err| err.to_string())?;
    Ok(LocalSessionBackend { tx })
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
    }
    .to_string()
}

/// Synchronous headless browser state for one agent control session.
pub struct HeadlessSession {
    opts: SessionOptions,
    document: Option<HtmlPageDocument>,
    js_host: Option<JsHost>,
    history: Option<History>,
    console: ConsoleJournal,
    tab_id: TabId,
    epoch: u64,
    last_status: LoadStatus,
    current_url: String,
}

impl HeadlessSession {
    pub fn new(opts: SessionOptions) -> Self {
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
        let history = self.history.as_mut().ok_or_else(no_page_loaded)?;
        if history.index() == 0 {
            return Err("no previous page".to_string());
        }
        let target = history.entries()[history.index() - 1].clone();
        self.load_url(target, timeout, LoadHistory::Traverse)?;
        let history = self.history.as_mut().ok_or_else(no_page_loaded)?;
        let _ = history.back();
        self.page_info()
    }

    pub fn forward(&mut self, timeout: Duration) -> Result<PageInfo, String> {
        let history = self.history.as_mut().ok_or_else(no_page_loaded)?;
        if history.index() + 1 >= history.entries().len() {
            return Err("no next page".to_string());
        }
        let target = history.entries()[history.index() + 1].clone();
        self.load_url(target, timeout, LoadHistory::Traverse)?;
        let history = self.history.as_mut().ok_or_else(no_page_loaded)?;
        let _ = history.forward();
        self.page_info()
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
        let load_started = Instant::now();
        let handle = tokio::runtime::Handle::current();
        let net = Arc::new(Provider::new(None));
        let (final_url, bytes) = headless::fetch_page(&url, self.opts.js_enabled, &handle, &net)?;
        let html = String::from_utf8_lossy(&bytes);

        self.js_host = None;

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
        let mut js_host = JsHost::new(self.opts.js_enabled, tap, Some(net.clone()));

        let config = DocumentConfig {
            base_url: Some(final_url.clone()),
            net_provider: Some(net_provider),
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

        self.epoch = self.epoch.saturating_add(1);
        js_host.set_console_route(self.tab_id, self.epoch);
        js_host.maybe_start_page(&document.0, &final_url, &handle);

        let settle_opts = SettleOptions {
            max: timeout,
            ..SettleOptions::default()
        };
        let tab_id = self.tab_id;
        let epoch = self.epoch;
        let journal = &mut self.console;
        let outcome = headless::settle(
            &mut document.0,
            &mut js_host,
            &net,
            self.opts.js_enabled,
            settle_opts,
            || {
                for ev in console_feed::drain_for_tab(tab_id) {
                    if ev.page_epoch == epoch {
                        journal.push(ConsoleEntry::from_wire(ev));
                    }
                }
            },
        );
        self.last_status = if outcome.timed_out {
            LoadStatus::Timeout
        } else {
            LoadStatus::Settled
        };

        if self.opts.js_enabled {
            self.drain_console_wire();
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
            self.drain_console_wire();
        }

        self.document = Some(document);
        self.js_host = Some(js_host);
        self.current_url = final_url.clone();

        match history {
            LoadHistory::Push => match &mut self.history {
                Some(h) => h.push(final_url),
                None => self.history = Some(History::new(final_url)),
            },
            LoadHistory::ReloadCurrent => {
                if let Some(h) = self.history.as_mut()
                    && h.current() != final_url
                {
                    h.replace_current(final_url);
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
    if let Ok(url) = url::Url::parse(trimmed)
        && !url.scheme().is_empty()
    {
        return url.to_string();
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
    fn resolve_nav_url_passes_data_and_about() {
        assert!(resolve_nav_url("data:text/html,hi").starts_with("data:"));
        assert_eq!(resolve_nav_url("about:blank"), "about:blank");
    }
}
