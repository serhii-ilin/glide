//! Long-lived headless browsing session on a dedicated thread.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use blitz_dom::{Document, DocumentConfig, local_name};
use blitz_net::Provider;
use blitz_traits::events::{
    BlitzImeEvent, BlitzKeyEvent, BlitzPointerEvent, BlitzPointerId, KeyState, MouseEventButton,
    MouseEventButtons, Point, PointerCoords, PointerDetails, UiEvent,
};
use blitz_traits::navigation::{NavigationOptions, NavigationProvider};
use blitz_traits::net::NetProvider;
use blitz_traits::node_id::NodeId;
use blitz_traits::shell::{ColorScheme, Viewport};
use crossbeam_channel::{RecvTimeoutError, Sender, bounded, unbounded};
use keyboard_types::{Code, Key, Location, Modifiers, NamedKey};

use super::ControlBackend;
use super::page_ops;
use super::protocol::{
    ClickParams, ControlRequest, ControlResult, DEFAULT_MAX_CHARS, DEFAULT_NAV_TIMEOUT_MS,
    DEFAULT_QUERY_LIMIT, ElementInfo, InputOutcome, LoadStatus, PageInfo, TypeParams,
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
const CONTROL_SESSION_TARGET: &str = "control.session";
const SESSION_PANIC_DETAIL_MAX: usize = 512;
const DEFAULT_INPUT_TIMEOUT_MS: u64 = 2_000;

static NEXT_SESSION_TAB: AtomicU64 = AtomicU64::new(SESSION_TAB_BASE);

/// Collects navigations requested by Blitz default actions (link clicks, form submits)
/// so the session thread can load them after the input event has been processed.
struct SessionNavigation {
    queue: Arc<Mutex<Vec<String>>>,
}

impl NavigationProvider for SessionNavigation {
    fn navigate_to(&self, options: NavigationOptions) {
        if let Ok(mut queue) = self.queue.lock() {
            queue.push(options.url.to_string());
        }
    }
}

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
        ControlRequest::Click(_) => "click",
        ControlRequest::Type(_) => "type",
    }
    .to_string()
}

/// Synchronous headless browser state for one agent control session.
pub struct HeadlessSession {
    opts: SessionOptions,
    document: Option<HtmlPageDocument>,
    js_host: Option<JsHost>,
    tap: JsTap,
    pending_nav: Arc<Mutex<Vec<String>>>,
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
            tap: JsTap::new_disabled(),
            pending_nav: Arc::new(Mutex::new(Vec::new())),
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
            ControlRequest::Click(p) => {
                let outcome = self.click(p)?;
                Ok(ControlResult::Input(outcome))
            }
            ControlRequest::Type(p) => {
                let outcome = self.type_text(p)?;
                Ok(ControlResult::Input(outcome))
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
        self.load_url(target.clone(), timeout, LoadHistory::Traverse)?;
        let history = self.history.as_mut().ok_or_else(no_page_loaded)?;
        let _ = history.back();
        if self.current_url != target {
            history.replace_current(self.current_url.clone());
        }
        self.page_info()
    }

    pub fn forward(&mut self, timeout: Duration) -> Result<PageInfo, String> {
        let history = self.history.as_mut().ok_or_else(no_page_loaded)?;
        if history.index() + 1 >= history.entries().len() {
            return Err("no next page".to_string());
        }
        let target = history.entries()[history.index() + 1].clone();
        self.load_url(target.clone(), timeout, LoadHistory::Traverse)?;
        let history = self.history.as_mut().ok_or_else(no_page_loaded)?;
        let _ = history.forward();
        if self.current_url != target {
            history.replace_current(self.current_url.clone());
        }
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

    /// Click at a selector's centre or at explicit page coordinates.
    ///
    /// `x`/`y` in [`ClickParams`] are viewport coordinates as seen in a screenshot; the
    /// session adds `viewport_scroll()` before dispatching pointer events. Worst case waits
    /// `timeout_ms` (default 2 s) for the input to settle, plus the navigation timeout
    /// (default 12 s) when the click starts a load.
    pub fn click(&mut self, params: ClickParams) -> Result<InputOutcome, String> {
        params.validate()?;
        let timeout = input_timeout(params.timeout_ms);
        let nav_timeout = nav_timeout(params.timeout_ms);
        let selector = params
            .selector
            .as_deref()
            .filter(|selector| !selector.trim().is_empty());
        self.prepare_input();

        let (x, y, viewport_x, viewport_y, target) = if let Some(selector) = selector {
            let (x, y, info) = {
                let doc = self.document.as_ref().ok_or_else(no_page_loaded)?;
                page_ops::element_center(&doc.0, selector)?
            };
            let scroll = self
                .document
                .as_ref()
                .ok_or_else(no_page_loaded)?
                .0
                .viewport_scroll();
            (x, y, x - scroll.x as f32, y - scroll.y as f32, Some(info))
        } else {
            let (viewport_x, viewport_y) = match (params.x, params.y) {
                (Some(x), Some(y)) => (x, y),
                _ => return Err("give a selector or both x and y".to_string()),
            };
            let scroll = self
                .document
                .as_ref()
                .ok_or_else(no_page_loaded)?
                .0
                .viewport_scroll();
            let target = {
                let doc = self.document.as_ref().ok_or_else(no_page_loaded)?;
                page_ops::element_at(&doc.0, viewport_x, viewport_y)
            };
            (
                viewport_x + scroll.x as f32,
                viewport_y + scroll.y as f32,
                viewport_x,
                viewport_y,
                target,
            )
        };

        self.send_pointer(x, y, viewport_x, viewport_y);
        self.settle_after_input(timeout);
        let navigated = self.follow_pending_navigation(nav_timeout, "clicked")?;
        let page = self.page_info()?;
        Ok(InputOutcome {
            x,
            y,
            target,
            navigated,
            page,
        })
    }

    /// Type text, optionally into a selector, and optionally press Enter.
    ///
    /// The selector is clicked first to focus it; `type` then moves the caret to the end
    /// so the text appends. Worst case waits `timeout_ms` (default 2 s) for the input to
    /// settle, plus the navigation timeout (default 12 s) when Enter submits a form.
    /// `setTimeout` follow-ups scheduled by page handlers are not awaited.
    pub fn type_text(&mut self, params: TypeParams) -> Result<InputOutcome, String> {
        let timeout = input_timeout(params.timeout_ms);
        let nav_timeout = nav_timeout(params.timeout_ms);
        let selector = params
            .selector
            .as_deref()
            .filter(|selector| !selector.trim().is_empty());
        self.prepare_input();

        let (x, y, target) = if let Some(selector) = selector {
            let (page_x, page_y, info) = {
                let doc = self.document.as_ref().ok_or_else(no_page_loaded)?;
                page_ops::element_center(&doc.0, selector)?
            };
            let scroll = self
                .document
                .as_ref()
                .ok_or_else(no_page_loaded)?
                .0
                .viewport_scroll();
            self.send_pointer(
                page_x,
                page_y,
                page_x - scroll.x as f32,
                page_y - scroll.y as f32,
            );
            if self.focused_text_field().is_none() {
                return Err(format!("element `{selector}` is not a text field"));
            }
            (page_x, page_y, Some(info))
        } else {
            let (_, info) = self
                .focused_text_field()
                .ok_or_else(|| "no focused text field; pass a selector".to_string())?;
            (
                info.rect.x + info.rect.width / 2.0,
                info.rect.y + info.rect.height / 2.0,
                Some(info),
            )
        };

        self.dispatch_ui_event(&UiEvent::KeyDown(end_key_event(KeyState::Pressed)));
        self.dispatch_ui_event(&UiEvent::Ime(BlitzImeEvent::Commit(params.text)));
        if params.submit {
            self.dispatch_ui_event(&UiEvent::KeyDown(enter_key_event(KeyState::Pressed)));
            self.dispatch_ui_event(&UiEvent::KeyUp(enter_key_event(KeyState::Released)));
        }

        self.settle_after_input(timeout);
        let navigated = self.follow_pending_navigation(nav_timeout, "typed")?;
        let page = self.page_info()?;
        Ok(InputOutcome {
            x,
            y,
            target,
            navigated,
            page,
        })
    }

    fn focused_text_field(&self) -> Option<(NodeId, ElementInfo)> {
        let doc = self.document.as_ref()?;
        let id = doc.0.get_focussed_node_id()?;
        if !is_editable(&doc.0, id) {
            return None;
        }
        Some((id, page_ops::element_info_for(&doc.0, id)?))
    }

    fn prepare_input(&mut self) {
        if let Some(doc) = self.document.as_mut() {
            let _ = crate::js::prune_stale_paint_children(&mut doc.0);
            doc.0.resolve(0.0);
        }
    }

    fn dispatch_ui_event(&mut self, event: &UiEvent) {
        if let Some(doc) = self.document.as_mut() {
            doc.0.handle_ui_event(event.clone());
        }
        self.tap.record_ui_event(event);
    }

    fn send_pointer(&mut self, page_x: f32, page_y: f32, client_x: f32, client_y: f32) {
        let move_event = UiEvent::PointerMove(pointer_event(
            page_x,
            page_y,
            client_x,
            client_y,
            MouseEventButtons::None,
        ));
        self.dispatch_ui_event(&move_event);
        let down_event = UiEvent::PointerDown(pointer_event(
            page_x,
            page_y,
            client_x,
            client_y,
            MouseEventButtons::Primary,
        ));
        self.dispatch_ui_event(&down_event);
        let up_event = UiEvent::PointerUp(pointer_event(
            page_x,
            page_y,
            client_x,
            client_y,
            MouseEventButtons::None,
        ));
        self.dispatch_ui_event(&up_event);
    }

    /// Flush recorded tap events, wait for the page worker to reach them, then flush DOM RPC.
    ///
    /// The worker processes messages in order, so an `eval_blocking` round trip after the
    /// dispatch barrier proves the page handlers ran. Timer follow-ups are not awaited.
    fn settle_after_input(&mut self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        if let (Some(doc), Some(js_host)) = (self.document.as_mut(), self.js_host.as_mut()) {
            doc.0.resolve(0.0);
            js_host.pump(&mut doc.0, Instant::now());
            let barrier = timeout
                .min(JsHost::default_eval_timeout())
                .max(Duration::from_millis(100));
            let _ = js_host.eval_blocking(&mut doc.0, "0", barrier);
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining > Duration::ZERO {
                js_host.flush_dom_rpc_until_idle(
                    &mut doc.0,
                    remaining.min(Duration::from_secs(1)),
                    Duration::from_millis(50),
                );
            }
            doc.0.resolve(0.0);
        }
        self.drain_console_wire();
    }

    fn follow_pending_navigation(
        &mut self,
        timeout: Duration,
        action: &str,
    ) -> Result<bool, String> {
        let Some(url) = self.pending_nav.lock().ok().and_then(|mut q| q.pop()) else {
            return Ok(false);
        };
        let scheme = url::Url::parse(&url)
            .map(|parsed| parsed.scheme().to_string())
            .unwrap_or_default();
        if !matches!(scheme.as_str(), "http" | "https" | "file" | "data") {
            logging::warn(
                CONTROL_SESSION_TARGET,
                "ignored unsupported navigation",
                &[
                    (
                        "page_url",
                        console_feed::truncate_field(
                            self.page_url_for_log().as_deref().unwrap_or("none"),
                            SESSION_LOG_PAGE_URL_MAX,
                        ),
                    ),
                    ("error_kind", "unsupported".into()),
                    ("feature", "F-068".into()),
                    ("detail", scheme),
                    (
                        "hint",
                        "The headless session loads only http, https, file and data URLs; the input itself was delivered."
                            .into(),
                    ),
                ],
            );
            return Ok(false);
        }
        self.navigate(&url, timeout)
            .map_err(|err| format!("{action}; navigation to {url} failed: {err}"))?;
        Ok(true)
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
        if let Ok(mut queue) = self.pending_nav.lock() {
            queue.clear();
        }
        let handle = tokio::runtime::Handle::current();
        let net = Arc::new(Provider::new(None));
        let (final_url, bytes) = headless::fetch_page(&url, self.opts.js_enabled, &handle, &net)?;
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
        let mut js_host = JsHost::new(self.opts.js_enabled, tap.clone(), Some(net.clone()));

        let config = DocumentConfig {
            base_url: Some(final_url.clone()),
            net_provider: Some(net_provider),
            navigation_provider: Some(Arc::new(SessionNavigation {
                queue: self.pending_nav.clone(),
            })),
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

        self.document = Some(document);
        self.js_host = Some(js_host);
        self.tap = tap;
        self.epoch = new_epoch;
        self.current_url = final_url.clone();
        self.last_status = last_status;

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

fn input_timeout(timeout_ms: Option<u64>) -> Duration {
    Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_INPUT_TIMEOUT_MS))
}

fn pointer_event(
    page_x: f32,
    page_y: f32,
    client_x: f32,
    client_y: f32,
    buttons: MouseEventButtons,
) -> BlitzPointerEvent {
    // Blitz's default actions hit-test with page coordinates while the JS tap and
    // screenshot-facing coordinates use client coordinates, so keep both fields honest.
    let coords = PointerCoords {
        page_x,
        page_y,
        screen_x: client_x,
        screen_y: client_y,
        client_x,
        client_y,
    };
    BlitzPointerEvent {
        id: BlitzPointerId::Mouse,
        is_primary: true,
        coords,
        button: MouseEventButton::Main,
        buttons,
        mods: Modifiers::default(),
        details: PointerDetails::default(),
        element: Point { x: 0.0, y: 0.0 },
        active_pointers: Arc::new(Default::default()),
    }
}

fn enter_key_event(state: KeyState) -> BlitzKeyEvent {
    BlitzKeyEvent {
        key: Key::Named(NamedKey::Enter),
        code: Code::Enter,
        modifiers: Modifiers::default(),
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state,
        text: None,
    }
}

fn end_key_event(state: KeyState) -> BlitzKeyEvent {
    BlitzKeyEvent {
        key: Key::Named(NamedKey::End),
        code: Code::End,
        modifiers: Modifiers::default(),
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state,
        text: None,
    }
}

fn is_editable(doc: &blitz_dom::BaseDocument, id: NodeId) -> bool {
    let Some(element) = doc.get_node(id).and_then(|node| node.element_data()) else {
        return false;
    };
    element.text_input_data().is_some()
        || matches!(
            element.attr(local_name!("contenteditable")),
            Some("") | Some("true") | Some("plaintext-only")
        )
}

pub(crate) fn resolve_nav_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Ok(url) = url::Url::parse(trimmed) {
        match url.scheme() {
            "file" => return trimmed.to_string(),
            "http" | "https" | "data" => return url.to_string(),
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
    fn type_text_writes_into_blitz_editor() {
        let rt = crate::tokio_runtime();
        let _g = rt.enter();
        let mut session = HeadlessSession::new(SessionOptions { js_enabled: true });
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/mcp-form.html");
        let url = url::Url::from_file_path(&path)
            .expect("file url")
            .to_string();
        session
            .navigate(&url, Duration::from_secs(12))
            .expect("navigate");
        let outcome = session
            .type_text(TypeParams {
                text: "Alice".into(),
                selector: Some("#name".into()),
                submit: false,
                timeout_ms: None,
            })
            .expect("type");
        assert!(!outcome.navigated);
        let doc = session.document.as_ref().expect("document");
        let id = doc.0.get_element_by_id("name").expect("input");
        let editor_text = |doc: &blitz_dom::BaseDocument| {
            doc.get_node(id)
                .and_then(|node| node.element_data())
                .and_then(|el| el.text_input_data())
                .map(|data| data.editor.text().to_string())
                .expect("editor text")
        };
        assert_eq!(editor_text(&doc.0), "Alice");
        session
            .type_text(TypeParams {
                text: "!".into(),
                selector: Some("#name".into()),
                submit: false,
                timeout_ms: None,
            })
            .expect("type appends");
        let doc = session.document.as_ref().expect("document");
        assert_eq!(editor_text(&doc.0), "Alice!");
    }
}
