//! V8 isolate on a dedicated thread.

use std::fmt::Write as _;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use once_cell::sync::OnceCell;

use super::bridge::{DomRequest, DomValue};
use super::diagnostics;
use super::host::DomRpc;
use crate::logging::{self, Level};
use url::Url;

static V8_INIT: OnceCell<()> = OnceCell::new();

thread_local! {
    static DOM_TX: std::cell::RefCell<Option<Sender<DomRpc>>> = const { std::cell::RefCell::new(None) };
}

pub struct JsWorker {
    tx: Sender<WorkerMessage>,
    _join: Option<thread::JoinHandle<()>>,
}

#[derive(Debug)]
enum WorkerMessage {
    InitPage {
        snapshot_json: String,
        page_url: String,
    },
    RunScripts {
        sources: Vec<(String, Option<u64>)>,
        /// `classic`, `defer`, or `async`. Empty means not a parser script.
        kind: String,
    },
    FireDomContentLoaded,
    /// All scripts queued for this load have run; emit the per-page summary.
    PageSettled,
    Pump {
        now_ms: u64,
    },
    FlushRaf,
    DispatchBubble {
        path: Vec<u64>,
        event_type: String,
    },
    Shutdown,
}

impl JsWorker {
    pub fn spawn(dom_tx: Sender<DomRpc>) -> Self {
        init_v8();
        let (tx, rx) = unbounded();
        let join = thread::Builder::new()
            .name("glide-js".into())
            .spawn(move || worker_main(dom_tx, rx))
            .expect("spawn js worker");
        Self {
            tx,
            _join: Some(join),
        }
    }

    pub fn init_page(&self, snapshot_json: String, page_url: String) {
        let _ = self.tx.send(WorkerMessage::InitPage {
            snapshot_json,
            page_url,
        });
    }

    pub fn run_scripts(&self, sources: Vec<(String, Option<u64>)>, kind: &str) {
        let _ = self.tx.send(WorkerMessage::RunScripts {
            sources,
            kind: kind.to_string(),
        });
    }

    pub fn fire_dom_content_loaded(&self) {
        let _ = self.tx.send(WorkerMessage::FireDomContentLoaded);
    }

    pub fn page_settled(&self) {
        let _ = self.tx.send(WorkerMessage::PageSettled);
    }

    pub fn pump(&self, now_ms: u64) {
        let _ = self.tx.send(WorkerMessage::Pump { now_ms });
    }

    pub fn flush_raf(&self) {
        let _ = self.tx.send(WorkerMessage::FlushRaf);
    }

    pub fn dispatch_dom_bubble(&self, path: Vec<u64>, event_type: impl Into<String>) {
        let _ = self.tx.send(WorkerMessage::DispatchBubble {
            path,
            event_type: event_type.into(),
        });
    }
}

impl Drop for JsWorker {
    fn drop(&mut self) {
        let _ = self.tx.send(WorkerMessage::Shutdown);
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

fn worker_main(dom_tx: Sender<DomRpc>, cmd_rx: Receiver<WorkerMessage>) {
    DOM_TX.with(|slot| *slot.borrow_mut() = Some(dom_tx));

    let isolate = &mut v8::Isolate::new(v8::CreateParams::default());
    isolate.set_capture_stack_trace_for_uncaught_exceptions(true, 16);
    isolate.add_message_listener(v8_message_listener);
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
        match msg {
            WorkerMessage::InitPage {
                snapshot_json,
                page_url,
            } => {
                diagnostics::set_page_url(page_url.clone());
                scripts_run = 0;
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                let snap = v8::String::new(scope, &snapshot_json).unwrap();
                let url = v8::String::new(scope, &page_url).unwrap();
                call_global_fn(scope, "__glide_init_page", &[snap.into(), url.into()]);
                settle_task(scope);
            }
            WorkerMessage::RunScripts { sources, kind } => {
                scripts_run = scripts_run.saturating_add(sources.len() as u64);
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                let kind_s = v8::String::new(scope, &kind).unwrap();
                call_global_fn(scope, "__glide_set_script_kind", &[kind_s.into()]);
                for (i, (src, script_id)) in sources.into_iter().enumerate() {
                    let script_name = format!("script-{i}");
                    diagnostics::set_script(Some(&script_name), script_id, Some(&kind));
                    if let Some(id) = script_id {
                        let sid = v8::String::new(scope, &id.to_string()).unwrap();
                        call_global_fn(scope, "__glide_set_current_script", &[sid.into()]);
                    } else {
                        call_global_fn(scope, "__glide_set_current_script", &[]);
                    }
                    run_script_with_timeout(scope, &src, &script_name);
                    call_global_fn(scope, "__glide_set_current_script", &[]);
                    diagnostics::clear_script();
                    settle_task(scope);
                }
                let cleared = v8::String::new(scope, "").unwrap();
                call_global_fn(scope, "__glide_set_script_kind", &[cleared.into()]);
            }
            WorkerMessage::FireDomContentLoaded => {
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                call_global_fn(scope, "__glide_fire_dom_content_loaded", &[]);
                settle_task(scope);
            }
            WorkerMessage::PageSettled => diagnostics::log_page_summary(scripts_run),
            WorkerMessage::Pump { now_ms } => {
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                let n = v8::Number::new(scope, now_ms as f64);
                call_global_fn(scope, "__glide_pump_timers", &[n.into()]);
                settle_task(scope);
            }
            WorkerMessage::FlushRaf => {
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                call_global_fn(scope, "__glide_flush_raf", &[]);
                settle_task(scope);
            }
            WorkerMessage::DispatchBubble { path, event_type } => {
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
            }
            WorkerMessage::Shutdown => break,
        }
    }
    DOM_TX.with(|slot| *slot.borrow_mut() = None);
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

    let (resp_tx, resp_rx) = bounded(1);
    let sent = DOM_TX.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|tx| {
                tx.send(DomRpc {
                    request: dom_req,
                    reply: resp_tx,
                })
            })
            .unwrap_or(Ok(()))
    });
    if sent.is_err() {
        rv.set(v8_error_object(scope, "dom channel closed"));
        return;
    }
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
    let feature = args.get(0).to_rust_string_lossy(scope);
    let detail = args.get(1).to_rust_string_lossy(scope);
    diagnostics::record_unsupported(&feature, &detail);
}

fn caps_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let missing = args.get(0).to_rust_string_lossy(scope);
    diagnostics::record_capabilities(&missing);
}

fn console_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let level = args.get(0).to_rust_string_lossy(scope);
    let msg = args.get(1).to_rust_string_lossy(scope);
    let level = match level.as_str() {
        "error" => Level::Error,
        "warn" | "warning" => Level::Warn,
        "debug" | "trace" => Level::Debug,
        _ => Level::Info,
    };
    diagnostics::record_console(level, &msg);
}

/// Capture uncaught V8 exceptions instead of letting V8 write raw `<unknown>:N:` lines.
extern "C" fn v8_message_listener(
    message: v8::Local<v8::Message>,
    _exception: v8::Local<v8::Value>,
) {
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
        "fetch" => DomRequest::Fetch {
            url: arg_string(scope, args, 2)?,
            reply,
        },
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
        #[cfg(debug_assertions)]
        "nodeCount" => DomRequest::NodeCount { reply },
        #[cfg(debug_assertions)]
        "testResize" => DomRequest::TestResize {
            width: arg_u32(scope, args, 2)?,
            height: arg_u32(scope, args, 3)?,
            reply,
        },
        other => return Err(format!("unknown dom op {other}")),
    })
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

fn v8_error_object<'s>(scope: &mut v8::PinScope<'s, '_>, msg: &str) -> v8::Local<'s, v8::Value> {
    let obj = v8::Object::new(scope);
    let key = v8::String::new(scope, "__error").unwrap();
    let val = v8::String::new(scope, msg).unwrap();
    let _ = obj.set(scope, key.into(), val.into());
    obj.into()
}

fn run_script_with_timeout(scope: &mut v8::PinScope<'_, '_>, source: &str, name: &str) {
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
    let Some(code) = v8::String::new(scope, source) else {
        logging::error(
            "js.script",
            "script source allocation failed",
            &script_fields(name),
        );
        done.store(true, Ordering::SeqCst);
        return;
    };
    run_script(scope, code, name);
    done.store(true, Ordering::SeqCst);
    scope.cancel_terminate_execution();
    let mut fields = script_fields(name);
    fields.push(("bytes", source.len().to_string()));
    fields.push(("seconds", format!("{:.3}", started.elapsed().as_secs_f64())));
    logging::debug("js.script", "script executed", &fields);
    if source.len() > 100_000 && std::env::var_os("GLIDE_MEASURE_SCRIPTS").is_some() {
        logging::info("js.script", "script execution finished", &fields);
    }
}

fn run_script(scope: &mut v8::PinScope<'_, '_>, source: v8::Local<v8::String>, name: &str) {
    let Some(script) = v8::Script::compile(scope, source, script_origin(scope, name).as_ref())
    else {
        logging::error("js.script", "script compile failed", &script_fields(name));
        return;
    };
    if script.run(scope).is_none() {
        logging::debug(
            "js.script",
            "script execution aborted",
            &script_fields(name),
        );
    }
}

fn script_origin<'s>(scope: &v8::PinScope<'s, '_>, name: &str) -> Option<v8::ScriptOrigin<'s>> {
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
        false,
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
    for _ in 0..32 {
        scope.perform_microtask_checkpoint();
        let ran = call_global_fn(scope, "__glide_drain_microtasks", &[])
            .is_some_and(|value| value.is_true());
        if !ran {
            break;
        }
    }
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
