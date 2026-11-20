//! V8 isolate on a dedicated thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use once_cell::sync::OnceCell;

use super::bridge::{DomRequest, DomResponse, DomValue};
use super::host::DomRpc;

static V8_INIT: OnceCell<()> = OnceCell::new();

thread_local! {
    static DOM_TX: std::cell::RefCell<Option<Sender<DomRpc>>> = const { std::cell::RefCell::new(None) };
}

pub struct JsWorker {
    tx: Sender<WorkerMessage>,
    join: Option<JoinHandle<()>>,
}

#[derive(Debug)]
enum WorkerMessage {
    RunScripts(Vec<String>),
    Pump { now_ms: u64 },
    FlushRaf,
    DispatchDom { node: u64, event_type: String },
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
            join: Some(join),
        }
    }

    pub fn run_scripts(&self, sources: Vec<String>) {
        let _ = self.tx.send(WorkerMessage::RunScripts(sources));
    }

    pub fn pump(&self, now_ms: u64) {
        let _ = self.tx.send(WorkerMessage::Pump { now_ms });
    }

    pub fn flush_raf(&self) {
        let _ = self.tx.send(WorkerMessage::FlushRaf);
    }

    pub fn dispatch_dom_event(&self, node: u64, event_type: impl Into<String>) {
        let _ = self.tx.send(WorkerMessage::DispatchDom {
            node,
            event_type: event_type.into(),
        });
    }
}

impl Drop for JsWorker {
    fn drop(&mut self) {
        let _ = self.tx.send(WorkerMessage::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
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
    let global_context = {
        v8::scope!(let hs, isolate);
        let context = v8::Context::new(hs, Default::default());
        {
            let scope = &mut v8::ContextScope::new(hs, context);
            install_bindings(scope, context);
            let source = v8::String::new(scope, include_str!("bootstrap.js")).unwrap();
            run_script(scope, source, "<bootstrap>");
        }
        v8::Global::new(hs, context)
    };

    while let Ok(msg) = cmd_rx.recv() {
        match msg {
            WorkerMessage::RunScripts(sources) => {
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                for (i, src) in sources.into_iter().enumerate() {
                    run_script_with_timeout(scope, &src, &format!("script-{i}"));
                    scope.perform_microtask_checkpoint();
                }
                call_global_fn(scope, "__glide_fire_dom_content_loaded", &[]);
                scope.perform_microtask_checkpoint();
            }
            WorkerMessage::Pump { now_ms } => {
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                let n = v8::Number::new(scope, now_ms as f64);
                call_global_fn(scope, "__glide_pump_timers", &[n.into()]);
                scope.perform_microtask_checkpoint();
            }
            WorkerMessage::FlushRaf => {
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                call_global_fn(scope, "__glide_flush_raf", &[]);
                scope.perform_microtask_checkpoint();
            }
            WorkerMessage::DispatchDom { node, event_type } => {
                v8::scope!(let hs, isolate);
                let ctx = v8::Local::new(hs, &global_context);
                let scope = &mut v8::ContextScope::new(hs, ctx);
                let n = v8::Number::new(scope, node as f64);
                let ty = v8::String::new(scope, &event_type).unwrap();
                call_global_fn(scope, "__glide_dispatch_event", &[n.into(), ty.into()]);
                scope.perform_microtask_checkpoint();
            }
            WorkerMessage::Shutdown => break,
        }
    }
    DOM_TX.with(|slot| *slot.borrow_mut() = None);
}

fn install_bindings(scope: &mut v8::PinScope<'_, '_>, context: v8::Local<v8::Context>) {
    let global = context.global(scope);
    let dom_fn = v8::Function::new(scope, dom_sync_callback).expect("dom fn");
    let dom_key = v8::String::new(scope, "__glide_dom_sync").unwrap();
    global.set(scope, dom_key.into(), dom_fn.into());

    let console_fn = v8::Function::new(scope, console_callback).expect("console fn");
    let console_key = v8::String::new(scope, "__glide_console").unwrap();
    global.set(scope, console_key.into(), console_fn.into());
}

fn dom_sync_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let op = args.get(0).to_rust_string_lossy(scope);
    let reply = args
        .get(1)
        .to_uint32(scope)
        .map(|n| n.value())
        .unwrap_or(1) as u64;

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

fn console_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let level = args.get(0).to_rust_string_lossy(scope);
    let msg = args.get(1).to_rust_string_lossy(scope);
    eprintln!("[console.{level}] {msg}");
}

fn map_dom_op(
    op: &str,
    reply: u64,
    scope: &v8::PinScope,
    args: &v8::FunctionCallbackArguments,
) -> Result<DomRequest, String> {
    Ok(match op {
        "getElementById" => DomRequest::GetElementById {
            id: arg_string(scope, args, 2)?,
            reply,
        },
        "querySelector" => DomRequest::QuerySelector {
            selector: arg_string(scope, args, 2)?,
            reply,
        },
        "querySelectorAll" => DomRequest::QuerySelectorAll {
            selector: arg_string(scope, args, 2)?,
            reply,
        },
        "createElement" => DomRequest::CreateElement {
            tag: arg_string(scope, args, 2)?,
            reply,
        },
        "createTextNode" => DomRequest::CreateTextNode {
            text: arg_string(scope, args, 2)?,
            reply,
        },
        "appendChild" => DomRequest::AppendChild {
            parent: arg_u64(scope, args, 2)?,
            child: arg_u64(scope, args, 3)?,
            reply,
        },
        "removeChild" => DomRequest::RemoveChild {
            parent: arg_u64(scope, args, 2)?,
            child: arg_u64(scope, args, 3)?,
            reply,
        },
        "insertBefore" => DomRequest::InsertBefore {
            parent: arg_u64(scope, args, 2)?,
            child: arg_u64(scope, args, 3)?,
            before: arg_u64(scope, args, 4)?,
            reply,
        },
        "setAttribute" => DomRequest::SetAttribute {
            node: arg_u64(scope, args, 2)?,
            name: arg_string(scope, args, 3)?,
            value: arg_string(scope, args, 4)?,
            reply,
        },
        "getAttribute" => DomRequest::GetAttribute {
            node: arg_u64(scope, args, 2)?,
            name: arg_string(scope, args, 3)?,
            reply,
        },
        "removeAttribute" => DomRequest::RemoveAttribute {
            node: arg_u64(scope, args, 2)?,
            name: arg_string(scope, args, 3)?,
            reply,
        },
        "setInnerHtml" => DomRequest::SetInnerHtml {
            node: arg_u64(scope, args, 2)?,
            html: arg_string(scope, args, 3)?,
            reply,
        },
        "getTextContent" => DomRequest::GetTextContent {
            node: arg_u64(scope, args, 2)?,
            reply,
        },
        "setTextContent" => DomRequest::SetTextContent {
            node: arg_u64(scope, args, 2)?,
            text: arg_string(scope, args, 3)?,
            reply,
        },
        "classAdd" => DomRequest::ClassListAdd {
            node: arg_u64(scope, args, 2)?,
            token: arg_string(scope, args, 3)?,
            reply,
        },
        "classRemove" => DomRequest::ClassListRemove {
            node: arg_u64(scope, args, 2)?,
            token: arg_string(scope, args, 3)?,
            reply,
        },
        "classContains" => DomRequest::ClassListContains {
            node: arg_u64(scope, args, 2)?,
            token: arg_string(scope, args, 3)?,
            reply,
        },
        "styleSet" => DomRequest::StyleSet {
            node: arg_u64(scope, args, 2)?,
            name: arg_string(scope, args, 3)?,
            value: arg_string(scope, args, 4)?,
            reply,
        },
        "fetch" => DomRequest::Fetch {
            url: arg_string(scope, args, 2)?,
            reply,
        },
        "listen" => DomRequest::DocumentUrl { reply },
        other => return Err(format!("unknown dom op {other}")),
    })
}

fn arg_string(
    scope: &v8::PinScope,
    args: &v8::FunctionCallbackArguments,
    idx: i32,
) -> Result<String, String> {
    Ok(args.get(idx).to_rust_string_lossy(scope))
}

fn arg_u64(
    scope: &v8::PinScope,
    args: &v8::FunctionCallbackArguments,
    idx: i32,
) -> Result<u64, String> {
    let n = args
        .get(idx)
        .to_uint32(scope)
        .ok_or_else(|| format!("arg {idx} not a number"))?;
    Ok(n.value() as u64)
}

fn dom_value_to_v8<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: &DomValue,
) -> v8::Local<'s, v8::Value> {
    match value {
        DomValue::Null => v8::null(scope).into(),
        DomValue::Bool(b) => v8::Boolean::new(scope, *b).into(),
        DomValue::Number(n) => v8::Number::new(scope, *n).into(),
        DomValue::String(s) => v8::String::new(scope, s).unwrap().into(),
        DomValue::NodeId(id) => v8::Number::new(scope, *id as f64).into(),
        DomValue::NodeList(ids) => {
            let elements: Vec<v8::Local<v8::Value>> = ids
                .iter()
                .map(|id| v8::Number::new(scope, *id as f64).into())
                .collect();
            v8::Array::new_with_elements(scope, &elements).into()
        }
        DomValue::Error(err) => v8_error_object(scope, err),
    }
}

fn v8_error_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    msg: &str,
) -> v8::Local<'s, v8::Value> {
    let obj = v8::Object::new(scope);
    let key = v8::String::new(scope, "__error").unwrap();
    let val = v8::String::new(scope, msg).unwrap();
    let _ = obj.set(scope, key.into(), val.into());
    obj.into()
}

fn run_script_with_timeout(scope: &mut v8::PinScope<'_, '_>, source: &str, name: &str) {
    let done = Arc::new(AtomicBool::new(false));
    let done_flag = done.clone();
    let handle = scope.thread_safe_handle();
    let watchdog = thread::spawn(move || {
        thread::sleep(Duration::from_secs(5));
        if !done_flag.load(Ordering::SeqCst) {
            handle.terminate_execution();
        }
    });
    let code = v8::String::new(scope, source).unwrap();
    run_script(scope, code, name);
    done.store(true, Ordering::SeqCst);
    let _ = watchdog.join();
    scope.cancel_terminate_execution();
}

fn run_script(scope: &mut v8::PinScope<'_, '_>, source: v8::Local<v8::String>, name: &str) {
    let Some(script) = v8::Script::compile(scope, source, None) else {
        eprintln!("script compile failed in {name}");
        return;
    };
    if script.run(scope).is_none() {
        eprintln!("script run failed in {name}");
    }
}

fn call_global_fn(scope: &mut v8::PinScope<'_, '_>, name: &str, argv: &[v8::Local<v8::Value>]) {
    let context = scope.get_current_context();
    let global = context.global(scope);
    let key = v8::String::new(scope, name).unwrap();
    let Some(func_val) = global.get(scope, key.into()) else {
        return;
    };
    if !func_val.is_function() {
        return;
    }
    if !func_val.is_function() {
        return;
    }
    let func = unsafe { v8::Local::<v8::Function>::cast_unchecked(func_val) };
    let _ = func.call(scope, global.into(), argv);
}