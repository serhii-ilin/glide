//! V8 isolate on a dedicated thread.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use once_cell::sync::OnceCell;

use super::bridge::{DomRequest, DomValue, FetchRequest};
use super::diagnostics;
use super::host::{DomRpc, HostWake};
use super::import_map::{self, ImportMapRegistry};
use super::modules::{self, ResolveError};
use crate::logging::{self, Level};
use url::Url;

static V8_INIT: OnceCell<()> = OnceCell::new();

/// Sentinel: no pending timer or rAF work.
pub const WAKE_NONE: u64 = u64::MAX;

const MODULE_CACHE_LIMIT: usize = 512;

struct PendingDynamicImport {
    resolver: v8::Global<v8::PromiseResolver>,
    specifier: String,
    referrer: String,
}

thread_local! {
    static MODULE_CACHE: RefCell<HashMap<String, v8::Global<v8::Module>>> = RefCell::new(HashMap::new());
    static PENDING_DYNAMIC_IMPORTS: RefCell<Vec<PendingDynamicImport>> =
        const { RefCell::new(Vec::new()) };
    static MODULE_RESOURCE_NAMES: RefCell<HashMap<i32, String>> = RefCell::new(HashMap::new());
    static DOM_FETCH_REPLY: std::cell::Cell<u64> = const { std::cell::Cell::new(16_384) };
    static DOM_TX: std::cell::RefCell<Option<Sender<DomRpc>>> = const { std::cell::RefCell::new(None) };
    static DOM_GENERATION: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static WORKER_TAB_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static OPAQUE_DOCUMENT_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static WORKER_RETIRED: std::cell::RefCell<Option<Arc<AtomicBool>>> =
        const { std::cell::RefCell::new(None) };
    static HOST_WAKE: std::cell::RefCell<Option<Arc<HostWake>>> = const { std::cell::RefCell::new(None) };
    static LAST_NOW_MS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static WAKE_PUBLISH: std::cell::RefCell<Option<(Arc<AtomicU64>, Arc<AtomicU64>)>> =
        const { std::cell::RefCell::new(None) };
    static PAGE_IMPORT_MAP: RefCell<ImportMapRegistry> =
        const { RefCell::new(ImportMapRegistry::empty()) };
}

/// Reply channel for one eval request; the worker sends the preview or the error text.
pub type EvalReply = crossbeam_channel::Sender<Result<String, String>>;

/// Commands delivered to the glide-js thread (page control and async socket I/O).
#[derive(Debug)]
pub enum WorkerCommand {
    InitPage {
        snapshot_json: String,
        page_url: String,
        storage_json: String,
        tab_id: u64,
        page_epoch: u64,
        opaque_document_id: u64,
    },
    RunScripts {
        sources: Vec<(String, Option<u64>, bool, String)>,
        kind: String,
    },
    FireDomContentLoaded,
    FirePageLoad,
    PageSettled,
    NotifyScriptElements(Vec<(u64, bool)>),
    RegisterImportMap {
        json: String,
        base_url: String,
        page_url: String,
        script_name: String,
        element: Option<u64>,
    },
    Pump {
        now_ms: u64,
    },
    FlushRaf,
    DispatchBubble {
        path: Vec<u64>,
        event_type: String,
    },
    SetConsoleRoute {
        tab_id: u64,
        page_epoch: u64,
    },
    SetNetworkRoute {
        tab_id: u64,
        network_epoch: u64,
    },
    Eval {
        source: String,
        tab_id: u64,
        page_epoch: u64,
        reply: Option<EvalReply>,
    },
    WebSocketEvent {
        json: String,
    },
    StorageEvent {
        json: String,
    },
    ApplyHistoryStep {
        json: String,
    },
    ViewportScrolled,
    Shutdown,
}

pub struct JsWorker {
    tx: Sender<WorkerCommand>,
    retired: Arc<AtomicBool>,
    worker_stopped: Arc<AtomicBool>,
    wake_deadline_ms: Arc<AtomicU64>,
    /// Report counter: lets tests tell "the worker has published a deadline"
    /// apart from the initial `WAKE_NONE` state. Read only by tests.
    #[cfg_attr(not(test), allow(dead_code))]
    wake_epoch: Arc<AtomicU64>,
    _join: Option<thread::JoinHandle<()>>,
}

impl JsWorker {
    pub fn wake_deadline_ms(&self) -> u64 {
        self.wake_deadline_ms.load(Ordering::Acquire)
    }

    /// Report counter (see the field comment); tests only.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn wake_epoch(&self) -> u64 {
        self.wake_epoch.load(Ordering::Acquire)
    }

    pub fn is_stopped(&self) -> bool {
        self.worker_stopped.load(Ordering::Acquire)
    }
}

impl JsWorker {
    pub fn spawn(dom_tx: Sender<DomRpc>, generation: u64, host_wake: Arc<HostWake>) -> Self {
        init_v8();
        let (tx, rx) = unbounded();
        super::websocket::register_command_sender(generation, tx.clone());
        super::storage_events::register_command_sender(generation, tx.clone());
        let retired = Arc::new(AtomicBool::new(false));
        let worker_stopped = Arc::new(AtomicBool::new(false));
        let wake_deadline_ms = Arc::new(AtomicU64::new(WAKE_NONE));
        let wake_epoch = Arc::new(AtomicU64::new(0));
        let retired_for_thread = Arc::clone(&retired);
        let stopped_for_thread = Arc::clone(&worker_stopped);
        let wake_deadline_for_thread = Arc::clone(&wake_deadline_ms);
        let wake_epoch_for_thread = Arc::clone(&wake_epoch);
        let host_wake_for_thread = Arc::clone(&host_wake);
        let join = thread::Builder::new()
            .name("glide-js".into())
            .spawn(move || {
                worker_main(WorkerThreadState {
                    dom_tx,
                    generation,
                    cmd_rx: rx,
                    retired: retired_for_thread,
                    worker_stopped: stopped_for_thread,
                    wake_deadline_ms: wake_deadline_for_thread,
                    wake_epoch: wake_epoch_for_thread,
                    host_wake: host_wake_for_thread,
                })
            })
            .expect("spawn js worker");
        Self {
            tx,
            retired,
            worker_stopped,
            wake_deadline_ms,
            wake_epoch,
            _join: Some(join),
        }
    }

    pub fn init_page(
        &self,
        snapshot_json: String,
        page_url: String,
        storage_json: String,
        tab_id: u64,
        page_epoch: u64,
        opaque_document_id: u64,
    ) {
        let _ = self.tx.send(WorkerCommand::InitPage {
            snapshot_json,
            page_url,
            storage_json,
            tab_id,
            page_epoch,
            opaque_document_id,
        });
    }

    pub fn register_import_map(
        &self,
        json: String,
        base_url: String,
        page_url: String,
        script_name: String,
        element: Option<u64>,
    ) {
        let _ = self.tx.send(WorkerCommand::RegisterImportMap {
            json,
            base_url,
            page_url,
            script_name,
            element,
        });
    }

    pub fn run_scripts(&self, sources: Vec<(String, Option<u64>, bool, String)>, kind: &str) {
        let _ = self.tx.send(WorkerCommand::RunScripts {
            sources,
            kind: kind.to_string(),
        });
    }

    pub fn fire_dom_content_loaded(&self) {
        let _ = self.tx.send(WorkerCommand::FireDomContentLoaded);
    }

    pub fn fire_page_load(&self) {
        let _ = self.tx.send(WorkerCommand::FirePageLoad);
    }

    pub fn apply_history_step(&self, json: String) {
        let _ = self.tx.send(WorkerCommand::ApplyHistoryStep { json });
    }

    /// Tell the page runtime the viewport offset changed outside page script (agent or wheel).
    pub fn viewport_scrolled(&self) {
        let _ = self.tx.send(WorkerCommand::ViewportScrolled);
    }

    pub fn page_settled(&self) {
        let _ = self.tx.send(WorkerCommand::PageSettled);
    }

    pub fn notify_script_elements(&self, elements: Vec<(u64, bool)>) {
        let _ = self.tx.send(WorkerCommand::NotifyScriptElements(elements));
    }

    pub fn pump(&self, now_ms: u64) {
        let _ = self.tx.send(WorkerCommand::Pump { now_ms });
    }

    pub fn flush_raf(&self) {
        let _ = self.tx.send(WorkerCommand::FlushRaf);
    }

    pub fn dispatch_dom_bubble(&self, path: Vec<u64>, event_type: impl Into<String>) {
        let _ = self.tx.send(WorkerCommand::DispatchBubble {
            path,
            event_type: event_type.into(),
        });
    }

    pub fn set_network_route(&self, tab_id: u64, network_epoch: u64) {
        let _ = self.tx.send(WorkerCommand::SetNetworkRoute {
            tab_id,
            network_epoch,
        });
    }

    /// Clone of the glide-js command channel (WebSocket I/O and page control).
    #[allow(dead_code)] // F-077: host/tests may enqueue `WebSocketEvent` commands.
    pub fn command_sender(&self) -> Sender<WorkerCommand> {
        self.tx.clone()
    }

    pub fn set_console_route(&self, tab_id: u64, page_epoch: u64) {
        let _ = self
            .tx
            .send(WorkerCommand::SetConsoleRoute { tab_id, page_epoch });
    }

    pub fn eval_console(
        &self,
        source: String,
        tab_id: u64,
        page_epoch: u64,
        reply: Option<EvalReply>,
    ) {
        let _ = self.tx.send(WorkerCommand::Eval {
            source,
            tab_id,
            page_epoch,
            reply,
        });
    }
}

impl Drop for JsWorker {
    fn drop(&mut self) {
        self.retired.store(true, Ordering::SeqCst);
        self.wake_deadline_ms.store(WAKE_NONE, Ordering::Release);
        self.worker_stopped.store(true, Ordering::Release);
        let _ = self.tx.send(WorkerCommand::Shutdown);
        // Detach the worker; do not join from the UI thread.
        self._join = None;
    }
}

fn init_v8() {
    V8_INIT.get_or_init(|| {
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
    });
}

/// Marks the worker stopped on every exit path, including a panic while running a
/// page script, so the UI thread cannot keep re-arming a dead worker's deadline.
struct WorkerExitGuard {
    wake_deadline_ms: Arc<AtomicU64>,
    worker_stopped: Arc<AtomicBool>,
}

impl Drop for WorkerExitGuard {
    fn drop(&mut self) {
        self.wake_deadline_ms.store(WAKE_NONE, Ordering::Release);
        self.worker_stopped.store(true, Ordering::Release);
    }
}

struct WorkerThreadState {
    dom_tx: Sender<DomRpc>,
    generation: u64,
    cmd_rx: Receiver<WorkerCommand>,
    retired: Arc<AtomicBool>,
    worker_stopped: Arc<AtomicBool>,
    wake_deadline_ms: Arc<AtomicU64>,
    wake_epoch: Arc<AtomicU64>,
    host_wake: Arc<HostWake>,
}

fn worker_main(state: WorkerThreadState) {
    let WorkerThreadState {
        dom_tx,
        generation,
        cmd_rx,
        retired,
        worker_stopped,
        wake_deadline_ms,
        wake_epoch,
        host_wake,
    } = state;
    let _exit = WorkerExitGuard {
        wake_deadline_ms: Arc::clone(&wake_deadline_ms),
        worker_stopped: Arc::clone(&worker_stopped),
    };
    DOM_TX.with(|slot| *slot.borrow_mut() = Some(dom_tx));
    DOM_GENERATION.with(|g| g.set(generation));
    WORKER_RETIRED.with(|slot| *slot.borrow_mut() = Some(Arc::clone(&retired)));
    HOST_WAKE.with(|slot| *slot.borrow_mut() = Some(Arc::clone(&host_wake)));
    WAKE_PUBLISH.with(|slot| {
        *slot.borrow_mut() = Some((Arc::clone(&wake_deadline_ms), Arc::clone(&wake_epoch)));
    });
    LAST_NOW_MS.with(|c| c.set(0));

    let isolate = &mut v8::Isolate::new(v8::CreateParams::default());
    isolate.set_capture_stack_trace_for_uncaught_exceptions(true, 16);
    isolate.add_message_listener(v8_message_listener);
    isolate.set_promise_reject_callback(promise_reject_callback);
    isolate.set_host_initialize_import_meta_object_callback(init_import_meta_object);
    isolate.set_host_import_module_dynamically_callback(host_import_module_dynamically);
    let global_context = {
        v8::scope!(let hs, isolate);
        let context = v8::Context::new(hs, Default::default());
        {
            let scope = &mut v8::ContextScope::new(hs, context);
            install_bindings(scope, context);
            let vendor = load_embedded_script_in_scope(scope, "vendor");
            run_script(scope, vendor, "<domino-vendor>");
            let glue = load_embedded_script_in_scope(scope, "dom-glue");
            run_script(scope, glue, "<dom-glue>");
            let bootstrap = load_embedded_script_in_scope(scope, "bootstrap");
            run_script(scope, bootstrap, "<bootstrap>");
        }
        v8::Global::new(hs, context)
    };

    let mut scripts_run = 0u64;
    while let Ok(msg) = cmd_rx.recv() {
        // Messages queued before retirement still carry the old page's scripts
        // and diagnostics; stop before running any of them.
        if worker_retired() {
            break;
        }
        match msg {
            WorkerCommand::InitPage {
                snapshot_json,
                page_url,
                storage_json,
                tab_id,
                page_epoch,
                opaque_document_id,
            } => {
                diagnostics::set_page_url(page_url.clone());
                super::console_feed::set_worker_route(tab_id, page_epoch);
                WORKER_TAB_ID.with(|t| t.set(tab_id));
                OPAQUE_DOCUMENT_ID.with(|d| d.set(opaque_document_id));
                super::promise_console::clear_promise_rows();
                clear_module_graph();
                PAGE_IMPORT_MAP.with(|map| *map.borrow_mut() = ImportMapRegistry::empty());
                PENDING_DYNAMIC_IMPORTS.with(|p| p.borrow_mut().clear());
                scripts_run = 0;
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                let snap = v8::String::new(scope, &snapshot_json).unwrap();
                let url = v8::String::new(scope, &page_url).unwrap();
                let storage = v8::String::new(scope, &storage_json).unwrap();
                call_global_fn(
                    scope,
                    "__glide_init_page",
                    &[snap.into(), url.into(), storage.into()],
                );
                settle_task(scope);
                publish_next_wake(scope, &wake_deadline_ms, &wake_epoch);
            }
            WorkerCommand::RunScripts { sources, kind } => {
                scripts_run = scripts_run.saturating_add(sources.len() as u64);
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                for (src, script_id, module, resource_name) in sources.into_iter() {
                    let script_kind = if module { "module" } else { kind.as_str() };
                    let kind_s = v8::String::new(scope, script_kind).unwrap();
                    call_global_fn(scope, "__glide_set_script_kind", &[kind_s.into()]);
                    let script_name = resource_name;
                    diagnostics::set_script(Some(&script_name), script_id, Some(script_kind));
                    if !module {
                        if let Some(id) = script_id {
                            let sid = v8::String::new(scope, &id.to_string()).unwrap();
                            call_global_fn(scope, "__glide_set_current_script", &[sid.into()]);
                        } else {
                            call_global_fn(scope, "__glide_set_current_script", &[]);
                        }
                    } else {
                        call_global_fn(scope, "__glide_set_current_script", &[]);
                    }
                    let ok = if module {
                        run_module_with_timeout(scope, &src, &script_name)
                    } else {
                        run_script_with_timeout(scope, &src, &script_name)
                    };
                    if let Some(id) = script_id {
                        notify_parser_script(scope, id, ok);
                    }
                    call_global_fn(scope, "__glide_set_current_script", &[]);
                    diagnostics::clear_script();
                    settle_task(scope);
                }
                let cleared = v8::String::new(scope, "").unwrap();
                call_global_fn(scope, "__glide_set_script_kind", &[cleared.into()]);
                publish_next_wake(scope, &wake_deadline_ms, &wake_epoch);
            }
            WorkerCommand::FireDomContentLoaded => {
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                call_global_fn(scope, "__glide_fire_dom_content_loaded", &[]);
                settle_task(scope);
                publish_next_wake(scope, &wake_deadline_ms, &wake_epoch);
            }
            WorkerCommand::FirePageLoad => {
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                call_global_fn(scope, "__glide_fire_page_load", &[]);
                settle_task(scope);
                publish_next_wake(scope, &wake_deadline_ms, &wake_epoch);
            }
            WorkerCommand::PageSettled => {
                if !worker_retired() {
                    diagnostics::log_page_summary(scripts_run);
                }
            }
            WorkerCommand::NotifyScriptElements(elements) => {
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                for (id, ok) in elements {
                    notify_parser_script(scope, id, ok);
                }
                settle_task(scope);
            }
            WorkerCommand::RegisterImportMap {
                json,
                base_url,
                page_url,
                script_name,
                element,
            } => {
                let ok = register_import_map_command(&page_url, &script_name, &json, &base_url);
                if let Some(id) = element {
                    v8::scope!(let hs, isolate);
                    let ctx = v8::Local::new(hs, &global_context);
                    let scope = &mut v8::ContextScope::new(hs, ctx);
                    notify_parser_script(scope, id, ok);
                    settle_task(scope);
                }
            }
            WorkerCommand::Pump { now_ms } => {
                LAST_NOW_MS.with(|c| c.set(now_ms));
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                let n = v8::Number::new(scope, now_ms as f64);
                call_global_fn(scope, "__glide_pump_timers", &[n.into()]);
                drain_pending_dynamic_imports(scope);
                settle_task(scope);
                publish_next_wake(scope, &wake_deadline_ms, &wake_epoch);
            }
            WorkerCommand::FlushRaf => {
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                call_global_fn(scope, "__glide_flush_raf", &[]);
                settle_task(scope);
                publish_next_wake(scope, &wake_deadline_ms, &wake_epoch);
            }
            WorkerCommand::SetConsoleRoute { tab_id, page_epoch } => {
                super::console_feed::set_worker_route(tab_id, page_epoch);
            }
            WorkerCommand::SetNetworkRoute {
                tab_id,
                network_epoch,
            } => {
                crate::network_feed::set_worker_route(tab_id, network_epoch);
            }
            WorkerCommand::Eval {
                source,
                tab_id,
                page_epoch,
                reply,
            } => {
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                let outcome =
                    super::repl_eval::eval_console_expression(scope, &source, tab_id, page_epoch);
                if let Some(reply) = reply {
                    let _ = reply.send(outcome);
                }
                publish_next_wake(scope, &wake_deadline_ms, &wake_epoch);
            }
            WorkerCommand::DispatchBubble { path, event_type } => {
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                let ids: Vec<v8::Local<v8::Value>> = path
                    .iter()
                    .map(|id| v8::String::new(scope, &id.to_string()).unwrap().into())
                    .collect();
                let array = v8::Array::new_with_elements(scope, &ids);
                let ty = v8::String::new(scope, &event_type).unwrap();
                call_global_fn(scope, "__glide_dispatch_bubble", &[array.into(), ty.into()]);
                settle_task(scope);
                publish_next_wake(scope, &wake_deadline_ms, &wake_epoch);
            }
            WorkerCommand::WebSocketEvent { json } => {
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                let payload = v8::String::new(scope, &json).unwrap();
                call_global_fn(scope, "__glide_ws_deliver", &[payload.into()]);
                settle_task(scope);
                request_host_wake();
                publish_next_wake(scope, &wake_deadline_ms, &wake_epoch);
            }
            WorkerCommand::StorageEvent { json } => {
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                let payload = v8::String::new(scope, &json).unwrap();
                call_global_fn(scope, "__glide_on_storage_event", &[payload.into()]);
                settle_task(scope);
                publish_next_wake(scope, &wake_deadline_ms, &wake_epoch);
            }
            WorkerCommand::ApplyHistoryStep { json } => {
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                let payload = v8::String::new(scope, &json).unwrap();
                call_global_fn(scope, "__glide_apply_history_step", &[payload.into()]);
                settle_task(scope);
                request_host_wake();
                publish_next_wake(scope, &wake_deadline_ms, &wake_epoch);
            }
            WorkerCommand::ViewportScrolled => {
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                call_global_fn(scope, "__glide_viewport_scrolled", &[]);
                settle_task(scope);
                request_host_wake();
                publish_next_wake(scope, &wake_deadline_ms, &wake_epoch);
            }
            WorkerCommand::Shutdown => break,
        }
    }
    // The exit guard also covers a panic; this bump records a clean shutdown for tests.
    wake_epoch.fetch_add(1, Ordering::Release);
    DOM_TX.with(|slot| *slot.borrow_mut() = None);
    WORKER_RETIRED.with(|slot| *slot.borrow_mut() = None);
    HOST_WAKE.with(|slot| *slot.borrow_mut() = None);
    WAKE_PUBLISH.with(|slot| *slot.borrow_mut() = None);
}

fn worker_retired() -> bool {
    WORKER_RETIRED.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Acquire))
    })
}

fn request_host_wake() {
    if worker_retired() {
        return;
    }
    HOST_WAKE.with(|slot| {
        if let Some(wake) = slot.borrow().as_ref() {
            wake.request();
        }
    });
}

fn publish_next_wake(
    scope: &mut v8::PinScope<'_, '_>,
    wake_deadline_ms: &AtomicU64,
    wake_epoch: &AtomicU64,
) {
    let last_now_ms = LAST_NOW_MS.with(|c| c.get());
    let n = v8::Number::new(scope, last_now_ms as f64);
    let deadline = call_global_fn(scope, "__glide_next_wake", &[n.into()])
        .and_then(|value| v8_wake_ms(scope, value))
        .unwrap_or(WAKE_NONE);
    let prev = wake_deadline_ms.load(Ordering::Acquire);
    wake_deadline_ms.store(deadline, Ordering::Release);
    wake_epoch.fetch_add(1, Ordering::Release);
    if !worker_retired() && deadline != WAKE_NONE && (prev == WAKE_NONE || deadline < prev) {
        request_host_wake();
    }
}

fn v8_wake_ms(scope: &mut v8::PinScope<'_, '_>, value: v8::Local<v8::Value>) -> Option<u64> {
    if !value.is_number() {
        return Some(WAKE_NONE);
    }
    let n = value.number_value(scope)?;
    if !n.is_finite() || n < 0.0 {
        return Some(WAKE_NONE);
    }
    Some(n.ceil().max(0.0) as u64)
}

fn load_embedded_script_in_scope<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    which: &str,
) -> v8::Local<'s, v8::String> {
    let text = match which {
        "vendor" => include_str!("vendor/domino.js"),
        "dom-glue" => include_str!("dom-glue.js"),
        _ => include_str!("bootstrap.js"),
    };
    v8::String::new(scope, text).expect("embed source")
}

fn install_bindings(scope: &mut v8::PinScope<'_, '_>, context: v8::Local<v8::Context>) {
    let global = context.global(scope);
    let dom_fn = v8::Function::new(scope, dom_sync_callback).expect("dom fn");
    let dom_key = v8::String::new(scope, "__glide_dom_sync").unwrap();
    global.set(scope, dom_key.into(), dom_fn.into());

    let console_fn = v8::Function::new(scope, console_callback).expect("console fn");
    let console_key = v8::String::new(scope, "__glide_console").unwrap();
    global.set(scope, console_key.into(), console_fn.into());

    let url_fn = v8::Function::new(scope, url_parse_callback).expect("url fn");
    let url_key = v8::String::new(scope, "__glide_url_parse").unwrap();
    global.set(scope, url_key.into(), url_fn.into());

    let diag_fn = v8::Function::new(scope, diag_callback).expect("diag fn");
    let diag_key = v8::String::new(scope, "__glide_diag").unwrap();
    global.set(scope, diag_key.into(), diag_fn.into());

    let caps_fn = v8::Function::new(scope, caps_callback).expect("caps fn");
    let caps_key = v8::String::new(scope, "__glide_caps").unwrap();
    global.set(scope, caps_key.into(), caps_fn.into());

    let exec_fn = v8::Function::new(scope, execute_script_callback).expect("exec fn");
    let exec_key = v8::String::new(scope, "__glide_execute_script").unwrap();
    global.set(scope, exec_key.into(), exec_fn.into());

    let platform = if cfg!(target_os = "macos") {
        "MacIntel"
    } else if cfg!(target_os = "windows") {
        "Win32"
    } else {
        "Linux x86_64"
    };
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let host_nav = format!(
        "globalThis.__glide_platform='{}';globalThis.__glide_hardware_concurrency={};",
        platform.replace('\\', "\\\\").replace('\'', "\\'"),
        cores
    );
    let host_nav_src = v8::String::new(scope, &host_nav).expect("host nav script");
    run_script(scope, host_nav_src, "<host-nav>");

    // Debug builds only. Release `map_dom_op` rejects `testResize` and `nodeCount`.
    #[cfg(debug_assertions)]
    {
        let hooks = v8::String::new(
            scope,
            "globalThis.__glide_debug_hooks=true;\
             globalThis.__glide_test_resize=function(w,h){\
               __glide_dom_sync('testResize',0,String(w),String(h));};\
             globalThis.__glide_node_count=function(){\
               if(typeof globalThis.__glide_flush_dom==='function')globalThis.__glide_flush_dom();\
               return __glide_dom_sync('nodeCount',0);};",
        )
        .expect("debug hooks");
        run_script(scope, hooks, "<debug-hooks>");
    }
}

fn dom_sync_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let op = args.get(0).to_rust_string_lossy(scope);
    let reply = args.get(1).to_uint32(scope).map(|n| n.value()).unwrap_or(1) as u64;

    let dom_req = match map_dom_op(&op, reply, scope, &args) {
        Ok(req) => req,
        Err(err) => {
            rv.set(v8_error_object(scope, &err));
            return;
        }
    };

    if worker_retired() {
        throw_page_detached(scope);
        return;
    }

    let (resp_tx, resp_rx) = bounded(1);
    let generation = DOM_GENERATION.with(|g| g.get());
    let sent = DOM_TX.with(|slot| {
        slot.borrow().as_ref().is_some_and(|tx| {
            tx.send(DomRpc {
                generation,
                request: dom_req,
                reply: resp_tx,
            })
            .is_ok()
        })
    });
    if !sent {
        if worker_retired() {
            throw_page_detached(scope);
        } else {
            rv.set(v8_error_object(scope, "dom channel closed"));
        }
        return;
    }
    request_host_wake();
    let Ok(resp) = resp_rx.recv_timeout(Duration::from_secs(30)) else {
        rv.set(v8_error_object(scope, "dom rpc timeout"));
        return;
    };
    rv.set(dom_value_to_v8(scope, &resp.value));
}

fn diag_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    if worker_retired() {
        return;
    }
    let feature = args.get(0).to_rust_string_lossy(scope);
    let detail = args.get(1).to_rust_string_lossy(scope);
    diagnostics::record_unsupported(&feature, &detail);
    request_host_wake();
}

fn caps_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    if worker_retired() {
        return;
    }
    let missing = args.get(0).to_rust_string_lossy(scope);
    diagnostics::record_capabilities(&missing);
    request_host_wake();
}

fn console_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    if worker_retired() {
        return;
    }
    let level_str = args.get(0).to_rust_string_lossy(scope);
    let msg = args.get(1).to_rust_string_lossy(scope);
    let level = match level_str.as_str() {
        "error" | "rejection" => Level::Error,
        "warn" | "warning" => Level::Warn,
        "debug" | "trace" => Level::Debug,
        _ => Level::Info,
    };
    diagnostics::record_console(level, &msg);
    let caller_line = console_caller_line(scope);
    super::console_feed::publish_console_level(&level_str, &msg, caller_line);
    request_host_wake();
}

fn console_caller_line(scope: &mut v8::PinScope<'_, '_>) -> Option<usize> {
    let stack = v8::StackTrace::current_stack_trace(scope, 8)?;
    for index in 0..stack.get_frame_count() {
        let frame = stack.get_frame(scope, index)?;
        let name = frame
            .get_script_name_or_source_url(scope)
            .map(|name| name.to_rust_string_lossy(scope))
            .unwrap_or_default();
        if name.starts_with('<') {
            continue;
        }
        return Some(frame.get_line_number());
    }
    None
}

extern "C" fn promise_reject_callback(msg: v8::PromiseRejectMessage) {
    use v8::PromiseRejectEvent;
    if worker_retired() {
        return;
    }
    v8::callback_scope!(unsafe scope, &msg);
    let promise = msg.get_promise();
    match msg.get_event() {
        PromiseRejectEvent::PromiseRejectWithNoHandler => {
            v8::scope!(let scope, scope);
            let row_id = super::promise_console::register_unhandled(scope, promise);
            let Some(value) = msg.get_value() else {
                super::console_feed::publish_unhandled_rejection(
                    "Unhandled rejection: <no reason>",
                    row_id,
                );
                return;
            };
            let message = if value.is_undefined() {
                "<undefined>".to_string()
            } else if value.is_null() {
                "null".to_string()
            } else {
                value.to_rust_string_lossy(scope)
            };
            super::console_feed::publish_unhandled_rejection(
                &format!("Unhandled rejection: {message}"),
                row_id,
            );
            request_host_wake();
        }
        PromiseRejectEvent::PromiseHandlerAddedAfterReject => {
            v8::scope!(let scope, scope);
            if let Some(row_id) = super::promise_console::revoke_handled(scope, promise) {
                super::console_feed::publish_rejection_revoke(row_id);
            }
        }
        _ => {}
    }
}

/// Capture uncaught V8 exceptions instead of letting V8 write raw `<unknown>:N:` lines.
extern "C" fn v8_message_listener(
    message: v8::Local<v8::Message>,
    _exception: v8::Local<v8::Value>,
) {
    if worker_retired() {
        return;
    }
    let scope = pin!(unsafe { v8::CallbackScope::new(message) });
    let scope = &mut scope.init();
    v8::scope!(let scope, scope);

    let text = message.get(scope).to_rust_string_lossy(scope);
    let line = message.get_line_number(scope);
    let resource = message
        .get_script_resource_name(scope)
        .map(|name| name.to_rust_string_lossy(scope));
    let stack = message
        .get_stack_trace(scope)
        .map(|trace| stack_trace_text(scope, &trace))
        .unwrap_or_default();

    diagnostics::record_uncaught(&text, line, resource.as_deref(), &stack);
    super::console_feed::publish_uncaught(&text, line, resource.as_deref());
    request_host_wake();
}

fn stack_trace_text(scope: &v8::PinScope<'_, '_>, trace: &v8::Local<v8::StackTrace>) -> String {
    let mut out = String::new();
    for index in 0..trace.get_frame_count().min(8) {
        let Some(frame) = trace.get_frame(scope, index) else {
            continue;
        };
        let name = frame
            .get_script_name_or_source_url(scope)
            .map(|name| name.to_rust_string_lossy(scope))
            .unwrap_or_else(|| "<unknown>".into());
        let _ = write!(out, "{name}:{};", frame.get_line_number());
    }
    out
}

fn map_dom_op(
    op: &str,
    reply: u64,
    scope: &v8::PinScope,
    args: &v8::FunctionCallbackArguments,
) -> Result<DomRequest, String> {
    Ok(match op {
        "fetch" => {
            let url = arg_string(scope, args, 2)?;
            let ParsedFetchOptions {
                method,
                headers,
                body,
                credentials,
            } = fetch_options(scope, args, 3)?;
            DomRequest::Fetch(FetchRequest {
                url,
                method,
                headers,
                body,
                credentials,
                reply,
                initiator: crate::devtools::network::NetworkInitiator::Fetch,
            })
        }
        "xhr" => {
            let url = arg_string(scope, args, 2)?;
            let ParsedFetchOptions {
                method,
                headers,
                body,
                credentials,
            } = fetch_options(scope, args, 3)?;
            DomRequest::Fetch(FetchRequest {
                url,
                method,
                headers,
                body,
                credentials,
                reply,
                initiator: crate::devtools::network::NetworkInitiator::Xhr,
            })
        }
        "listen" => {
            let node = arg_handle(scope, args, 2)?;
            let event_type = arg_string(scope, args, 3)?;
            DomRequest::ListenTarget {
                node,
                event_type,
                reply,
            }
        }
        "snapshotTree" => DomRequest::SnapshotTree { reply },
        "batchMutate" => DomRequest::BatchMutate {
            json: arg_string(scope, args, 2)?,
            reply,
        },
        "boundingRect" => DomRequest::BoundingRect {
            node: arg_handle(scope, args, 2)?,
            reply,
        },
        "loadImage" => DomRequest::LoadImage {
            node: arg_handle(scope, args, 2)?,
            reply,
        },
        "scrollViewport" => {
            let x = arg_string(scope, args, 2)?
                .parse::<f64>()
                .map_err(|_| "scrollViewport x must be a number".to_string())?;
            let y = arg_string(scope, args, 3)?
                .parse::<f64>()
                .map_err(|_| "scrollViewport y must be a number".to_string())?;
            DomRequest::ScrollViewport { x, y, reply }
        }
        "viewportInfo" => DomRequest::ViewportInfo { reply },
        "scrollIntoView" => DomRequest::ScrollIntoView {
            node: arg_handle(scope, args, 2)?,
            block: arg_string(scope, args, 3)?,
            reply,
        },
        "computedStyle" => DomRequest::ComputedStyle {
            node: arg_handle(scope, args, 2)?,
            prop: arg_string(scope, args, 3)?,
            reply,
        },
        "layoutField" => DomRequest::LayoutField {
            node: arg_handle(scope, args, 2)?,
            field: arg_string(scope, args, 3)?,
            reply,
        },
        "pageUrl" => DomRequest::PageUrl { reply },
        "navigate" => DomRequest::Navigate {
            url: arg_string(scope, args, 2)?,
            reply,
        },
        "submitForm" => DomRequest::SubmitForm {
            form: arg_handle(scope, args, 2)?,
            submitter: arg_handle(scope, args, 3)?,
            reply,
        },
        "controlField" => DomRequest::ControlField {
            node: arg_handle(scope, args, 2)?,
            field: arg_string(scope, args, 3)?,
            reply,
        },
        "syncFormControls" => DomRequest::SyncFormControls {
            form: arg_handle(scope, args, 2)?,
            reply,
        },
        "pushFormValues" => DomRequest::PushFormValues {
            json: arg_string(scope, args, 2)?,
            reply,
        },
        "formDataEntries" => DomRequest::FormDataEntries {
            form: arg_handle(scope, args, 2)?,
            submitter: arg_handle(scope, args, 3)?,
            reply,
        },
        "randomBytes" => {
            let size = arg_string(scope, args, 2)?
                .parse::<usize>()
                .map_err(|_| "randomBytes size must be a non-negative integer".to_string())?;
            DomRequest::RandomBytes { size, reply }
        }
        "cookieGet" => DomRequest::CookieGet { reply },
        "cookieSet" => DomRequest::CookieSet {
            value: arg_string(scope, args, 2)?,
            reply,
        },
        "storageCall" => DomRequest::StorageCall {
            method: arg_string(scope, args, 2)?,
            area: arg_string(scope, args, 3)?,
            key: arg_optional_string(scope, args, 4),
            value: arg_optional_string(scope, args, 5),
            index: arg_optional_u32(scope, args, 6),
            page_url: arg_string(scope, args, 7)?,
            tab_id: WORKER_TAB_ID.with(|t| t.get()),
            opaque_document_id: OPAQUE_DOCUMENT_ID.with(|d| d.get()),
            source_generation: DOM_GENERATION.with(|g| g.get()),
            reply,
        },
        "wsConnect" => DomRequest::WebSocketConnect {
            socket_id: arg_u64(scope, args, 2)?,
            url: arg_string(scope, args, 3)?,
            protocols: arg_string(scope, args, 4).unwrap_or_else(|_| "[]".into()),
            reply,
        },
        "wsSend" => {
            let binary = arg_string(scope, args, 4)
                .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            DomRequest::WebSocketSend {
                socket_id: arg_u64(scope, args, 2)?,
                data: arg_string(scope, args, 3)?,
                binary,
                reply,
            }
        }
        "wsClose" => DomRequest::WebSocketClose {
            socket_id: arg_u64(scope, args, 2)?,
            code: arg_u16(scope, args, 3).unwrap_or(1000),
            reason: arg_string(scope, args, 4).unwrap_or_default(),
            reply,
        },
        "wsBuffered" => DomRequest::WebSocketBuffered {
            socket_id: arg_u64(scope, args, 2)?,
            reply,
        },
        #[cfg(debug_assertions)]
        "nodeCount" => DomRequest::NodeCount { reply },
        #[cfg(debug_assertions)]
        "testResize" => DomRequest::TestResize {
            width: arg_u32(scope, args, 2)?,
            height: arg_u32(scope, args, 3)?,
            reply,
        },
        "historySnapshot" => DomRequest::HistorySnapshot { reply },
        "historyPush" => DomRequest::HistoryPush {
            json: arg_string(scope, args, 2)?,
            reply,
        },
        "historyReplace" => DomRequest::HistoryReplace {
            json: arg_string(scope, args, 2)?,
            reply,
        },
        "historyGo" => DomRequest::HistoryGo {
            delta: arg_i32(scope, args, 2)?,
            reply,
        },
        "historyGoTo" => DomRequest::HistoryGoTo {
            target: arg_usize(scope, args, 2)?,
            reply,
        },
        other => return Err(format!("unknown dom op {other}")),
    })
}

fn arg_i32(
    scope: &v8::PinScope,
    args: &v8::FunctionCallbackArguments,
    idx: i32,
) -> Result<i32, String> {
    let text = arg_string(scope, args, idx)?;
    text.parse::<i32>()
        .map_err(|_| format!("arg {idx} not an integer"))
}

fn arg_usize(
    scope: &v8::PinScope,
    args: &v8::FunctionCallbackArguments,
    idx: i32,
) -> Result<usize, String> {
    let text = arg_string(scope, args, idx)?;
    text.parse::<usize>()
        .map_err(|_| format!("arg {idx} not an integer"))
}

fn arg_u64(
    scope: &v8::PinScope,
    args: &v8::FunctionCallbackArguments,
    idx: i32,
) -> Result<u64, String> {
    let text = arg_string(scope, args, idx)?;
    text.parse::<u64>()
        .map_err(|_| format!("arg {idx} not an integer"))
}

fn arg_u16(
    scope: &v8::PinScope,
    args: &v8::FunctionCallbackArguments,
    idx: i32,
) -> Result<u16, String> {
    let text = arg_string(scope, args, idx)?;
    text.parse::<u16>()
        .map_err(|_| format!("arg {idx} not an integer"))
}

#[cfg(debug_assertions)]
fn arg_u32(
    scope: &v8::PinScope,
    args: &v8::FunctionCallbackArguments,
    idx: i32,
) -> Result<u32, String> {
    let text = arg_string(scope, args, idx)?;
    text.parse::<u32>()
        .map_err(|_| format!("arg {idx} not a viewport size"))
}

fn arg_string(
    scope: &v8::PinScope,
    args: &v8::FunctionCallbackArguments,
    idx: i32,
) -> Result<String, String> {
    Ok(args.get(idx).to_rust_string_lossy(scope))
}

fn arg_optional_string(
    scope: &v8::PinScope,
    args: &v8::FunctionCallbackArguments,
    idx: i32,
) -> Option<String> {
    if args.length() <= idx {
        return None;
    }
    let raw = args.get(idx);
    if raw.is_null() || raw.is_undefined() {
        return None;
    }
    Some(raw.to_rust_string_lossy(scope))
}

fn arg_optional_u32(
    scope: &v8::PinScope,
    args: &v8::FunctionCallbackArguments,
    idx: i32,
) -> Option<u32> {
    let text = arg_optional_string(scope, args, idx)?;
    text.parse::<u32>().ok()
}

struct ParsedFetchOptions {
    method: String,
    headers: Vec<(String, String)>,
    body: String,
    credentials: String,
}

fn fetch_options(
    scope: &v8::PinScope,
    args: &v8::FunctionCallbackArguments,
    idx: i32,
) -> Result<ParsedFetchOptions, String> {
    if args.length() <= idx {
        return Ok(ParsedFetchOptions {
            method: String::from("GET"),
            headers: Vec::new(),
            body: String::new(),
            credentials: String::from("same-origin"),
        });
    }
    let raw = args.get(idx);
    if raw.is_undefined() || raw.is_null() {
        return Ok(ParsedFetchOptions {
            method: String::from("GET"),
            headers: Vec::new(),
            body: String::new(),
            credentials: String::from("same-origin"),
        });
    }
    let json = raw.to_rust_string_lossy(scope);
    if json.trim().is_empty() {
        return Ok(ParsedFetchOptions {
            method: String::from("GET"),
            headers: Vec::new(),
            body: String::new(),
            credentials: String::from("same-origin"),
        });
    }
    let opts: serde_json::Value =
        serde_json::from_str(&json).map_err(|_| "invalid fetch options json")?;
    let method = opts
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("GET")
        .to_string();
    let mut headers = Vec::new();
    if let Some(serde_json::Value::Object(map)) = opts.get("headers") {
        for (k, v) in map {
            if let Some(s) = v.as_str() {
                headers.push((k.clone(), s.to_string()));
            }
        }
    }
    let body = match opts.get("body") {
        None => String::new(),
        Some(serde_json::Value::String(s)) => {
            serde_json::json!({"kind": "text", "data": s}).to_string()
        }
        Some(v) => v.to_string(),
    };
    let credentials = opts
        .get("credentials")
        .and_then(|b| b.as_str())
        .unwrap_or("same-origin")
        .to_string();
    Ok(ParsedFetchOptions {
        method,
        headers,
        body,
        credentials,
    })
}

fn arg_handle(
    scope: &v8::PinScope,
    args: &v8::FunctionCallbackArguments,
    idx: i32,
) -> Result<u64, String> {
    let v = args.get(idx);
    if v.is_string() {
        let s = v.to_rust_string_lossy(scope);
        return s
            .parse::<u64>()
            .map_err(|_| format!("arg {idx} not a valid node id"));
    }
    if let Some(n) = v.number_value(scope)
        && n >= 0.0
        && n <= u64::MAX as f64
        && n.trunc() == n
    {
        return Ok(n as u64);
    }
    Err(format!("arg {idx} not a node id"))
}

fn dom_value_to_v8<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: &DomValue,
) -> v8::Local<'s, v8::Value> {
    match value {
        DomValue::Null => v8::null(scope).into(),
        DomValue::String(s) => v8::String::new(scope, s).unwrap().into(),
        DomValue::Number(n) => v8::Number::new(scope, *n).into(),
        DomValue::Json(v) => json_value_to_v8(scope, v),
        DomValue::Error(err) => v8_error_object(scope, err),
    }
}

fn json_value_to_v8<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: &serde_json::Value,
) -> v8::Local<'s, v8::Value> {
    match value {
        serde_json::Value::Null => v8::null(scope).into(),
        serde_json::Value::Bool(b) => v8::Boolean::new(scope, *b).into(),
        serde_json::Value::Number(n) => v8::Number::new(scope, n.as_f64().unwrap_or(0.0)).into(),
        serde_json::Value::String(s) => v8::String::new(scope, s).unwrap().into(),
        serde_json::Value::Array(arr) => {
            let elems: Vec<v8::Local<v8::Value>> =
                arr.iter().map(|v| json_value_to_v8(scope, v)).collect();
            v8::Array::new_with_elements(scope, &elems).into()
        }
        serde_json::Value::Object(map) => {
            let obj = v8::Object::new(scope);
            for (k, v) in map {
                let val = json_value_to_v8(scope, v);
                let key = v8::String::new(scope, k).unwrap();
                let _ = obj.set(scope, key.into(), val);
            }
            obj.into()
        }
    }
}

fn url_parse_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let href = args.get(0).to_rust_string_lossy(scope);
    let base = if args.length() > 1 {
        Some(args.get(1).to_rust_string_lossy(scope))
    } else {
        None
    };
    let resolved = match base {
        Some(b) => Url::parse(&b).and_then(|u| u.join(&href)),
        None => Url::parse(&href),
    };
    let Ok(url) = resolved else {
        rv.set(v8::undefined(scope).into());
        return;
    };
    let obj = v8::Object::new(scope);
    let set = |name: &str, val: &str| {
        let key = v8::String::new(scope, name).unwrap();
        let v = v8::String::new(scope, val).unwrap();
        let _ = obj.set(scope, key.into(), v.into());
    };
    set("href", url.as_str());
    set("protocol", &format!("{}:", url.scheme()));
    let hostname = url.host_str().unwrap_or("");
    let port = url.port().map(|p| p.to_string()).unwrap_or_default();
    let host = if port.is_empty() {
        hostname.to_string()
    } else {
        format!("{hostname}:{port}")
    };
    set("host", &host);
    set("hostname", hostname);
    set("port", &port);
    set("pathname", url.path());
    let search = url.query().map(|q| format!("?{q}")).unwrap_or_default();
    let hash = url.fragment().map(|h| format!("#{h}")).unwrap_or_default();
    set("search", &search);
    set("hash", &hash);
    let origin = if url.scheme() == "file" || url.host_str().is_none() {
        "null".into()
    } else {
        let host = url.host_str().unwrap_or("");
        format!(
            "{}://{}{}",
            url.scheme(),
            host,
            url.port().map(|p| format!(":{}", p)).unwrap_or_default()
        )
    };
    set("origin", &origin);
    rv.set(obj.into());
}

fn throw_page_detached(scope: &mut v8::PinScope<'_, '_>) {
    let msg = v8::String::new(scope, "page detached")
        .expect("V8 string allocation cannot fail for a static ASCII message");
    let err = v8::Exception::error(scope, msg);
    scope.throw_exception(err);
}

fn v8_error_object<'s>(scope: &mut v8::PinScope<'s, '_>, msg: &str) -> v8::Local<'s, v8::Value> {
    let obj = v8::Object::new(scope);
    let key = v8::String::new(scope, "__error").unwrap();
    let val = v8::String::new(scope, msg).unwrap();
    let _ = obj.set(scope, key.into(), val.into());
    obj.into()
}

fn local_v8_string<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    text: &str,
) -> Option<v8::Local<'s, v8::String>> {
    v8::String::new(scope, text)
}

fn notify_parser_script(scope: &mut v8::PinScope<'_, '_>, script_id: u64, ok: bool) {
    let Some(id_s) = local_v8_string(scope, &script_id.to_string()) else {
        return;
    };
    let ok_v = v8::Boolean::new(scope, ok);
    let _ = call_global_fn(
        scope,
        "__glide_notify_script_element",
        &[id_s.into(), ok_v.into()],
    );
}

fn run_script_with_timeout(scope: &mut v8::PinScope<'_, '_>, source: &str, name: &str) -> bool {
    let started = std::time::Instant::now();
    let done = Arc::new(AtomicBool::new(false));
    let done_flag = done.clone();
    let handle = scope.thread_safe_handle();
    thread::spawn(move || {
        for _ in 0..50 {
            if done_flag.load(Ordering::SeqCst) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
        if !done_flag.load(Ordering::SeqCst) {
            handle.terminate_execution();
        }
    });
    let Some(code) = local_v8_string(scope, source) else {
        logging::error(
            "js.script",
            "script source allocation failed",
            &script_fields(name),
        );
        done.store(true, Ordering::SeqCst);
        return false;
    };
    let ok = run_script(scope, code, name);
    done.store(true, Ordering::SeqCst);
    scope.cancel_terminate_execution();
    let mut fields = script_fields(name);
    fields.push(("bytes", source.len().to_string()));
    fields.push(("seconds", format!("{:.3}", started.elapsed().as_secs_f64())));
    logging::debug("js.script", "script executed", &fields);
    if source.len() > 100_000 && std::env::var_os("GLIDE_MEASURE_SCRIPTS").is_some() {
        logging::info("js.script", "script execution finished", &fields);
    }
    ok
}

fn module_cache_insert<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    key: &str,
    module: v8::Local<'s, v8::Module>,
) -> bool {
    MODULE_CACHE.with(|cache| {
        let mut map = cache.borrow_mut();
        if map.len() >= MODULE_CACHE_LIMIT && !map.contains_key(key) {
            logging::warn(
                "js.loader",
                "module cache full",
                &[
                    ("limit", MODULE_CACHE_LIMIT.to_string()),
                    ("url", key.to_string()),
                    (
                        "hint",
                        "Too many distinct module URLs; drop the page or restart.".into(),
                    ),
                ],
            );
            return false;
        }
        map.insert(key.to_string(), v8::Global::new(scope, module));
        true
    })
}

/// Compile, instantiate, and evaluate one ES module (including its dependency graph).
fn run_module_with_timeout(scope: &mut v8::PinScope<'_, '_>, source: &str, name: &str) -> bool {
    let started = std::time::Instant::now();
    let done = Arc::new(AtomicBool::new(false));
    let done_flag = done.clone();
    let handle = scope.thread_safe_handle();
    thread::spawn(move || {
        for _ in 0..50 {
            if done_flag.load(Ordering::SeqCst) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
        if !done_flag.load(Ordering::SeqCst) {
            handle.terminate_execution();
        }
    });
    if let Some(hit) = MODULE_CACHE.with(|cache| cache.borrow().get(name).cloned()) {
        let module = v8::Local::new(scope, hit);
        if module.get_status() == v8::ModuleStatus::Evaluated {
            done.store(true, Ordering::SeqCst);
            return true;
        }
    }
    let bytes = source.len();
    let Some(module) = compile_module_source(scope, source, name) else {
        modules::log_module_fetch_failure(
            diagnostics::current().page_url.as_deref(),
            name,
            "module compile failed".into(),
        );
        logging::error("js.script", "module compile failed", &script_fields(name));
        done.store(true, Ordering::SeqCst);
        return false;
    };
    if !module_cache_insert(scope, name, module) {
        done.store(true, Ordering::SeqCst);
        return false;
    }
    if !matches!(module.instantiate_module(scope, resolve_module), Some(true)) {
        logging::error(
            "js.loader",
            "module instantiate failed",
            &script_fields(name),
        );
        done.store(true, Ordering::SeqCst);
        return false;
    }
    if module.evaluate(scope).is_none() {
        logging::debug(
            "js.script",
            "module execution aborted",
            &script_fields(name),
        );
        done.store(true, Ordering::SeqCst);
        return false;
    }
    done.store(true, Ordering::SeqCst);
    scope.cancel_terminate_execution();
    let mut fields = script_fields(name);
    fields.push(("bytes", bytes.to_string()));
    fields.push(("seconds", format!("{:.3}", started.elapsed().as_secs_f64())));
    logging::debug("js.script", "script executed", &fields);
    true
}

fn clear_module_graph() {
    MODULE_CACHE.with(|cache| cache.borrow_mut().clear());
    MODULE_RESOURCE_NAMES.with(|names| names.borrow_mut().clear());
}

fn import_map_active() -> bool {
    !PAGE_IMPORT_MAP.with(|map| map.borrow().is_empty())
}

fn register_import_map_command(
    page_url: &str,
    script_name: &str,
    json: &str,
    base_url: &str,
) -> bool {
    match import_map::parse_import_map_json(json, base_url) {
        Ok(parsed) => {
            PAGE_IMPORT_MAP.with(|registry| registry.borrow_mut().merge(parsed));
            logging::debug(
                "js.loader",
                "import map registered",
                &[
                    ("page_url", page_url.to_string()),
                    ("script", script_name.to_string()),
                    (
                        "hint",
                        "Later module loads resolve bare specifiers through this map.".into(),
                    ),
                ],
            );
            true
        }
        Err(err) => {
            import_map::log_import_map_failure(page_url, format!("{}: {}", err.kind, err.detail));
            false
        }
    }
}

fn dom_fetch_sync(url: &str) -> Result<String, String> {
    if worker_retired() {
        return Err("page detached".into());
    }
    let (resp_tx, resp_rx) = bounded(1);
    let reply = DOM_FETCH_REPLY.get();
    DOM_FETCH_REPLY.set(reply.saturating_add(1));
    let generation = DOM_GENERATION.with(|g| g.get());
    let sent = DOM_TX.with(|slot| {
        slot.borrow().as_ref().is_some_and(|tx| {
            tx.send(DomRpc {
                generation,
                request: DomRequest::Fetch(FetchRequest {
                    url: url.to_string(),
                    method: "GET".into(),
                    headers: Vec::new(),
                    body: String::new(),
                    credentials: "same-origin".into(),
                    reply,
                    initiator: crate::devtools::network::NetworkInitiator::Subresource,
                }),
                reply: resp_tx,
            })
            .is_ok()
        })
    });
    if !sent {
        return Err("dom channel closed".into());
    }
    request_host_wake();
    let Ok(resp) = resp_rx.recv_timeout(Duration::from_secs(25)) else {
        return Err("fetch timed out".into());
    };
    match resp.value {
        DomValue::Json(value) => {
            let status = value.get("status").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            if !(200..300).contains(&status) {
                return Err(format!("HTTP {status}"));
            }
            value
                .get("body")
                .and_then(|b| b.as_str())
                .map(str::to_string)
                .ok_or_else(|| "empty response body".to_string())
        }
        DomValue::Error(msg) => Err(msg),
        other => Err(format!("unexpected fetch reply: {other:?}")),
    }
}

fn register_module_resource(module: v8::Local<v8::Module>, resource_name: &str) {
    if let Some(id) = module.script_id() {
        MODULE_RESOURCE_NAMES.with(|names| {
            names.borrow_mut().insert(id, resource_name.to_string());
        });
    }
}

fn compile_module_source<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    source: &str,
    resource_name: &str,
) -> Option<v8::Local<'s, v8::Module>> {
    let code = v8::String::new(scope, source)?;
    let origin = script_origin_with_module(scope, resource_name, true)?;
    let mut compiled = v8::script_compiler::Source::new(code, Some(&origin));
    let module = v8::script_compiler::compile_module(scope, &mut compiled)?;
    register_module_resource(module, resource_name);
    Some(module)
}

fn module_referrer_url(referrer: v8::Local<v8::Module>) -> String {
    referrer
        .script_id()
        .and_then(|id| MODULE_RESOURCE_NAMES.with(|names| names.borrow().get(&id).cloned()))
        .or_else(|| diagnostics::current().page_url.clone())
        .unwrap_or_default()
}

fn resolve_specifier_to_url(referrer: &str, spec: &str) -> Option<Url> {
    let referrer_base = match Url::parse(referrer) {
        Ok(_) => referrer.to_string(),
        Err(_) => diagnostics::current().page_url.clone().unwrap_or_default(),
    };
    if referrer_base.is_empty() {
        return None;
    }
    match PAGE_IMPORT_MAP.with(|m| {
        let map = m.borrow();
        let map_ref = if map.is_empty() { None } else { Some(&*map) };
        modules::resolve_specifier_from_str(&referrer_base, spec, map_ref)
    }) {
        Ok(url) => Some(url),
        Err(ResolveError::Bare(name)) => {
            let detail = if import_map_active() {
                format!("no import map entry for bare specifier {name}")
            } else {
                format!("bare specifier {name} (import maps not supported)")
            };
            diagnostics::record_unsupported("module-import", &detail);
            None
        }
        Err(ResolveError::Invalid(detail)) => {
            modules::log_module_fetch_failure(
                diagnostics::current().page_url.as_deref(),
                spec,
                detail,
            );
            None
        }
    }
}

fn load_fetched_module<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    cache_key: &str,
) -> Option<v8::Local<'s, v8::Module>> {
    if let Some(hit) = MODULE_CACHE.with(|cache| cache.borrow().get(cache_key).cloned()) {
        return Some(v8::Local::new(scope, hit));
    }
    let source = dom_fetch_sync(cache_key)
        .map_err(|err| {
            modules::log_module_fetch_failure(
                diagnostics::current().page_url.as_deref(),
                cache_key,
                err,
            );
        })
        .ok()?;
    let Some(module) = compile_module_source(scope, &source, cache_key) else {
        modules::log_module_fetch_failure(
            diagnostics::current().page_url.as_deref(),
            cache_key,
            "module compile failed".into(),
        );
        return None;
    };
    if !module_cache_insert(scope, cache_key, module) {
        return None;
    }
    if !matches!(module.instantiate_module(scope, resolve_module), Some(true)) {
        MODULE_CACHE.with(|cache| cache.borrow_mut().remove(cache_key));
        modules::log_module_fetch_failure(
            diagnostics::current().page_url.as_deref(),
            cache_key,
            "module instantiate failed".into(),
        );
        return None;
    }
    Some(module)
}

fn evaluate_module_namespace<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    module: v8::Local<'s, v8::Module>,
) -> Option<v8::Local<'s, v8::Value>> {
    if module.get_status() == v8::ModuleStatus::Evaluated {
        return Some(module.get_module_namespace());
    }
    module.evaluate(scope)?;
    Some(module.get_module_namespace())
}

/// Resolve and compile module dependencies for V8's module loader.
///
/// V8 invokes this callback synchronously during `instantiate_module`; the loader
/// must return a compiled module (or `None`) before instantiation continues.
fn resolve_module<'s>(
    context: v8::Local<'s, v8::Context>,
    specifier: v8::Local<'s, v8::String>,
    _import_attributes: v8::Local<'s, v8::FixedArray>,
    referrer: v8::Local<'s, v8::Module>,
) -> Option<v8::Local<'s, v8::Module>> {
    v8::callback_scope!(unsafe scope, context);
    let spec = specifier.to_rust_string_lossy(scope);
    let referrer_name = module_referrer_url(referrer);
    let resolved = resolve_specifier_to_url(&referrer_name, &spec)?;
    load_fetched_module(scope, resolved.as_ref())
}

extern "C" fn init_import_meta_object(
    context: v8::Local<v8::Context>,
    module: v8::Local<v8::Module>,
    meta: v8::Local<v8::Object>,
) {
    v8::callback_scope!(unsafe scope, context);
    let url = module_referrer_url(module);
    let Some(key) = v8::String::new(scope, "url") else {
        return;
    };
    let Some(val) = v8::String::new(scope, &url) else {
        return;
    };
    let _ = meta.create_data_property(scope, key.into(), val.into());
}

fn dynamic_import_reject_message(spec: &str) -> String {
    if modules::is_bare_specifier(spec) {
        modules::bare_specifier_rejection_message(spec, import_map_active())
    } else {
        format!("module resolution failed for {spec}")
    }
}

fn fulfill_dynamic_import<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    resolver: v8::Local<'s, v8::PromiseResolver>,
    spec: &str,
    referrer: &str,
) {
    let Some(resolved) = resolve_specifier_to_url(referrer, spec) else {
        let msg = dynamic_import_reject_message(spec);
        if let Some(s) = local_v8_string(scope, &msg) {
            resolver.reject(scope, s.into());
        }
        return;
    };
    let cache_key = resolved.to_string();
    let Some(module) = load_fetched_module(scope, &cache_key) else {
        let msg = format!("module load failed for {spec}");
        if let Some(s) = local_v8_string(scope, &msg) {
            resolver.reject(scope, s.into());
        }
        return;
    };
    match evaluate_module_namespace(scope, module) {
        Some(ns) => {
            resolver.resolve(scope, ns);
        }
        None => {
            let msg = format!("module evaluation failed for {spec}");
            if let Some(s) = local_v8_string(scope, &msg) {
                resolver.reject(scope, s.into());
            }
        }
    }
}

fn drain_pending_dynamic_imports(scope: &mut v8::PinScope<'_, '_>) {
    let jobs = PENDING_DYNAMIC_IMPORTS.with(|p| std::mem::take(&mut *p.borrow_mut()));
    for job in jobs {
        let resolver = v8::Local::new(scope, job.resolver);
        fulfill_dynamic_import(scope, resolver, &job.specifier, &job.referrer);
    }
}

fn host_import_module_dynamically<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    _host_defined_options: v8::Local<'s, v8::Data>,
    resource_name: v8::Local<'s, v8::Value>,
    specifier: v8::Local<'s, v8::String>,
    _import_attributes: v8::Local<'s, v8::FixedArray>,
) -> Option<v8::Local<'s, v8::Promise>> {
    let resolver = v8::PromiseResolver::new(scope)?;
    let spec = specifier.to_rust_string_lossy(scope);
    let referrer = v8_value_to_string(scope, resource_name)
        .unwrap_or_else(|| diagnostics::current().page_url.clone().unwrap_or_default());
    PENDING_DYNAMIC_IMPORTS.with(|p| {
        p.borrow_mut().push(PendingDynamicImport {
            resolver: v8::Global::new(scope, resolver),
            specifier: spec,
            referrer,
        });
    });
    request_host_wake();
    Some(resolver.get_promise(scope))
}

fn v8_value_to_string(
    scope: &mut v8::PinScope<'_, '_>,
    value: v8::Local<v8::Value>,
) -> Option<String> {
    if value.is_string() {
        return Some(
            v8::Local::<v8::String>::try_from(value)
                .ok()?
                .to_rust_string_lossy(scope),
        );
    }
    None
}

fn execute_script_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if worker_retired() {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    }
    let source = args.get(0).to_rust_string_lossy(scope);
    let kind = args.get(1).to_rust_string_lossy(scope);
    let name = args.get(2).to_rust_string_lossy(scope);
    let module = kind == "module";
    let script_kind = if module { "module" } else { "dynamic" };
    let Some(kind_s) = local_v8_string(scope, script_kind) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    call_global_fn(scope, "__glide_set_script_kind", &[kind_s.into()]);
    diagnostics::set_script(Some(&name), None, Some(script_kind));
    let ok = if module {
        run_module_with_timeout(scope, &source, &name)
    } else {
        run_script_with_timeout(scope, &source, &name)
    };
    diagnostics::clear_script();
    if let Some(cleared) = local_v8_string(scope, "") {
        call_global_fn(scope, "__glide_set_script_kind", &[cleared.into()]);
    }
    WAKE_PUBLISH.with(|slot| {
        if let Some((deadline, epoch)) = slot.borrow().as_ref() {
            publish_next_wake(scope, deadline, epoch);
        }
    });
    rv.set(v8::Boolean::new(scope, ok).into());
}

fn run_script(scope: &mut v8::PinScope<'_, '_>, source: v8::Local<v8::String>, name: &str) -> bool {
    let Some(script) = v8::Script::compile(scope, source, script_origin(scope, name).as_ref())
    else {
        logging::error("js.script", "script compile failed", &script_fields(name));
        return false;
    };
    if script.run(scope).is_none() {
        logging::debug(
            "js.script",
            "script execution aborted",
            &script_fields(name),
        );
        return false;
    }
    true
}

fn script_origin<'s>(scope: &v8::PinScope<'s, '_>, name: &str) -> Option<v8::ScriptOrigin<'s>> {
    script_origin_with_module(scope, name, false)
}

fn script_origin_with_module<'s>(
    scope: &v8::PinScope<'s, '_>,
    name: &str,
    is_module: bool,
) -> Option<v8::ScriptOrigin<'s>> {
    let resource_name: v8::Local<v8::Value> = v8::String::new(scope, name)?.into();
    Some(v8::ScriptOrigin::new(
        scope,
        resource_name,
        0,
        0,
        false,
        0,
        None,
        false,
        false,
        is_module,
        None,
    ))
}

fn script_fields(name: &str) -> Vec<(&'static str, String)> {
    let ctx = diagnostics::current();
    let mut fields = vec![
        ("page_url", ctx.page_url.unwrap_or_else(|| "unknown".into())),
        ("script", name.to_string()),
    ];
    if let Some(kind) = ctx.script_kind {
        fields.push(("script_kind", kind));
    }
    fields
}

/// Run V8 microtasks and the Glide queue until both are idle, then flush DOM ops.
fn settle_task(scope: &mut v8::PinScope<'_, '_>) {
    drain_microtasks(scope);
    flush_dom(scope);
}

/// Run V8 microtasks and the Glide queue until both are idle.
pub(super) fn drain_microtasks(scope: &mut v8::PinScope<'_, '_>) {
    for _ in 0..32 {
        scope.perform_microtask_checkpoint();
        let ran = call_global_fn(scope, "__glide_drain_microtasks", &[])
            .is_some_and(|value| value.is_true());
        if !ran {
            break;
        }
    }
}

/// Flush queued DOM mutations to the Blitz mirror.
pub(super) fn flush_dom(scope: &mut v8::PinScope<'_, '_>) {
    call_global_fn(scope, "__glide_flush_dom", &[]);
}

fn call_global_fn<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    name: &str,
    argv: &[v8::Local<v8::Value>],
) -> Option<v8::Local<'s, v8::Value>> {
    let context = scope.get_current_context();
    let global = context.global(scope);
    let key = v8::String::new(scope, name).unwrap();
    let func_val = global.get(scope, key.into())?;
    if !func_val.is_function() {
        return None;
    }
    let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(func_val) };
    func.call(scope, global.into(), argv)
}

#[cfg(test)]
mod vendor_tests {
    use super::*;

    #[test]
    fn domino_vendor_exports_htmlelement() {
        init_v8();
        let isolate = &mut v8::Isolate::new(v8::CreateParams::default());
        v8::scope!(let hs, isolate);
        let context = v8::Context::new(hs, Default::default());
        let scope = &mut v8::ContextScope::new(hs, context);
        install_bindings(scope, context);
        let vendor = load_embedded_script_in_scope(scope, "vendor");
        run_script(scope, vendor, "<domino-vendor-test>");
        let global = context.global(scope);
        let key = v8::String::new(scope, "__glide_domino").unwrap();
        let val = global.get(scope, key.into()).expect("domino export");
        assert!(val.is_object());
        let check = v8::String::new(
            scope,
            r#"(function(d){
            return d && typeof d.impl.HTMLElement === 'function'
              && d.createDocument('<p id=x>', true).getElementById('x') != null;
          })(__glide_domino)"#,
        )
        .unwrap();
        let script = v8::Script::compile(scope, check, None).expect("compile");
        let result = script.run(scope).expect("run");
        assert!(result.is_true());
    }
}
