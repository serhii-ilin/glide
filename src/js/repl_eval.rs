//! Console REPL evaluation and DevTools-style value preview formatting.

use std::fmt::Write as _;
use std::pin::pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use super::console_feed;

const DEFAULT_WATCHDOG_MS: u64 = 5_000;
const WATCHDOG_TICK_MS: u64 = 100;

static WATCHDOG_MS: AtomicU64 = AtomicU64::new(DEFAULT_WATCHDOG_MS);

/// Override the REPL watchdog duration; [`super::harness::WatchdogOverride`] restores it.
#[cfg(test)]
pub fn set_watchdog_duration_ms(ms: u64) {
    WATCHDOG_MS.store(ms, Ordering::Relaxed);
}

pub fn watchdog_duration_ms() -> u64 {
    WATCHDOG_MS.load(Ordering::Relaxed)
}

fn lock_done(done: &Mutex<bool>) -> MutexGuard<'_, bool> {
    done.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Arms a background thread that terminates the isolate after the watchdog duration.
///
/// `finish` is atomic with the final check-then-terminate, so once `finish` returns no
/// new terminate can land; the caller only has to clear one requested in that window.
struct EvalWatchdog {
    done: Arc<Mutex<bool>>,
}

impl EvalWatchdog {
    fn start(scope: &v8::PinScope<'_, '_>) -> Self {
        let done = Arc::new(Mutex::new(false));
        let done_flag = Arc::clone(&done);
        let handle = scope.thread_safe_handle();
        let total_ms = watchdog_duration_ms().max(WATCHDOG_TICK_MS);
        let ticks = total_ms.div_ceil(WATCHDOG_TICK_MS);
        thread::spawn(move || {
            for _ in 0..ticks {
                if *lock_done(&done_flag) {
                    return;
                }
                thread::sleep(Duration::from_millis(WATCHDOG_TICK_MS));
            }
            let done = lock_done(&done_flag);
            if !*done {
                handle.terminate_execution();
            }
        });
        Self { done }
    }

    fn finish(&self) {
        *lock_done(&self.done) = true;
    }
}

impl Drop for EvalWatchdog {
    fn drop(&mut self) {
        self.finish();
    }
}

/// Clears the `<console>` script label when evaluation returns, on success or error.
struct ScriptLabelGuard;

impl Drop for ScriptLabelGuard {
    fn drop(&mut self) {
        super::diagnostics::clear_script();
    }
}

pub fn quote_js_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Wrap an expression so indirect eval returns its JSON serialization.
///
/// The reply channel reports the DevTools preview of the resulting string, so a
/// caller parses that string literal to recover the JSON text.
pub fn json_eval_source(expression: &str) -> String {
    format!("JSON.stringify((0,eval)({}))", quote_js_string(expression))
}

pub fn eval_console_expression<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    source: &str,
    tab_id: u64,
    page_epoch: u64,
) -> Result<String, String> {
    let (route_tab, route_epoch) = console_feed::worker_route();
    if tab_id != route_tab || page_epoch != route_epoch {
        let message = "Expression ignored: active tab or page changed before evaluation.";
        console_feed::publish_repl_error(message);
        return Err(message.to_string());
    }
    match eval_expression_value(scope, source) {
        Ok(preview) => {
            console_feed::publish_repl_result(&preview);
            Ok(preview)
        }
        Err(msg) => {
            console_feed::publish_repl_error(&msg);
            Err(msg)
        }
    }
}

fn eval_expression_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    source: &str,
) -> Result<String, String> {
    let watchdog = EvalWatchdog::start(scope);
    let outcome = eval_source_value(scope, source, &watchdog);
    // Microtasks stay under the watchdog so an endless promise chain is bounded, but
    // the DOM flush runs after it: a terminate between emptying `pendingBatch` and the
    // `batchMutate` RPC would lose the batch or leave new nodes untagged.
    let timed_out = drain_microtasks_under_watchdog(scope);
    watchdog.finish();
    // Clear a terminate requested between the watchdog's last check and `finish`.
    scope.cancel_terminate_execution();
    super::worker::flush_dom(scope);
    if timed_out {
        return Err(timeout_message());
    }
    outcome
}

fn eval_source_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    source: &str,
    watchdog: &EvalWatchdog,
) -> Result<String, String> {
    let quoted = serde_json::to_string(source).map_err(|e| e.to_string())?;
    let code = format!("(0,eval)({quoted})");
    super::diagnostics::set_script(Some("<console>"), None, Some("repl"));
    let _label = ScriptLabelGuard;
    let value = run_expression_with_watchdog(scope, &code, "<repl>", watchdog)?;
    format_v8_preview_under_watchdog(scope, value, 0)
}

/// Drain microtasks, returning true when the watchdog terminated the drain.
fn drain_microtasks_under_watchdog<'s>(scope: &mut v8::PinScope<'s, '_>) -> bool {
    let tc = pin!(v8::TryCatch::new(scope));
    let tc = &mut tc.init();
    super::worker::drain_microtasks(tc);
    tc.has_terminated()
}

fn run_expression_with_watchdog<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    source: &str,
    name: &str,
    _watchdog: &EvalWatchdog,
) -> Result<v8::Local<'s, v8::Value>, String> {
    let tc = pin!(v8::TryCatch::new(scope));
    let tc = &mut tc.init();
    let Some(code) = v8::String::new(tc, source) else {
        return Err("Failed to allocate eval source".to_string());
    };
    let Some(name_v) = v8::String::new(tc, name) else {
        return Err("Failed to allocate eval script name".to_string());
    };
    let origin = v8::ScriptOrigin::new(
        tc,
        name_v.into(),
        0,
        0,
        false,
        0,
        None,
        false,
        false,
        false,
        None,
    );
    let Some(script) = v8::Script::compile(tc, code, Some(&origin)) else {
        tc.cancel_terminate_execution();
        return Err("REPL expression failed to compile".to_string());
    };
    let out = script.run(tc);
    if tc.has_terminated() {
        tc.cancel_terminate_execution();
        return Err(timeout_message());
    }
    tc.cancel_terminate_execution();
    if tc.has_caught() {
        let message = tc
            .exception()
            .map(|exception| exception.to_rust_string_lossy(tc))
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "Uncaught exception".to_string());
        let text = if message.starts_with("Uncaught ") {
            message
        } else {
            format!("Uncaught {message}")
        };
        return Err(text);
    }
    out.ok_or_else(|| "Expression produced no value".to_string())
}

fn format_v8_preview_under_watchdog<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
    depth: usize,
) -> Result<String, String> {
    let tc = pin!(v8::TryCatch::new(scope));
    let tc = &mut tc.init();
    let text = format_v8_preview(tc, value, depth);
    if tc.has_terminated() {
        tc.cancel_terminate_execution();
        return Err(timeout_message());
    }
    if tc.has_caught() {
        tc.reset();
        return Ok("<unserializable>".to_string());
    }
    Ok(text)
}

fn timeout_message() -> String {
    let secs = watchdog_duration_ms() as f64 / 1000.0;
    format!("EvalError: script execution timed out after {secs:.1} seconds")
}

const PREVIEW_MAX_LEN: usize = 240;
const PREVIEW_MAX_DEPTH: usize = 2;
const PREVIEW_MAX_PROPS: usize = 6;

fn format_v8_preview<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
    depth: usize,
) -> String {
    if value.is_undefined() {
        return "undefined".to_string();
    }
    if value.is_null() {
        return "null".to_string();
    }
    if value.is_proxy() {
        return "Proxy {…}".to_string();
    }
    if value.is_symbol() {
        return format_symbol(scope, value);
    }
    if value.is_boolean() {
        return value.boolean_value(scope).to_string();
    }
    if value.is_number() {
        return value.to_rust_string_lossy(scope);
    }
    if value.is_string() {
        return quote_js_string(&value.to_rust_string_lossy(scope));
    }
    if value.is_big_int() {
        return value.to_rust_string_lossy(scope);
    }
    if value.is_function() {
        return format_function_name(scope, value);
    }
    if value.is_array() {
        return format_array(scope, value, depth);
    }
    if value.is_object() {
        return format_object(scope, value, depth);
    }
    "<unserializable>".to_string()
}

fn format_symbol<'s>(scope: &mut v8::PinScope<'s, '_>, value: v8::Local<'s, v8::Value>) -> String {
    let Ok(sym) = v8::Local::<v8::Symbol>::try_from(value) else {
        return "Symbol()".to_string();
    };
    let desc = sym.description(scope);
    if desc.is_undefined() {
        return "Symbol()".to_string();
    }
    if desc.is_string() {
        return format!(
            "Symbol({})",
            quote_js_string(&desc.to_rust_string_lossy(scope))
        );
    }
    "Symbol()".to_string()
}

fn format_function_name<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
) -> String {
    let Some(obj) = value.to_object(scope) else {
        return "f anonymous()".to_string();
    };
    let Some(name_key) = v8::String::new(scope, "name") else {
        return "f anonymous()".to_string();
    };
    let label = match obj.get_own_property_descriptor(scope, name_key.into()) {
        Some(desc) => descriptor_data_value(scope, desc)
            .filter(|v| v.is_string())
            .map(|v| v.to_rust_string_lossy(scope))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "anonymous".to_string()),
        None => "anonymous".to_string(),
    };
    format!("f {label}()")
}

fn format_array<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
    depth: usize,
) -> String {
    let Ok(array) = v8::Local::<v8::Array>::try_from(value) else {
        return "[…]".to_string();
    };
    let len = array.length();
    let mut out = String::from("[");
    let cap = if depth >= PREVIEW_MAX_DEPTH {
        0
    } else {
        PREVIEW_MAX_PROPS.min(len as usize)
    };
    for i in 0..cap {
        if i > 0 {
            out.push_str(", ");
        }
        if out.len() >= PREVIEW_MAX_LEN {
            out.push('…');
            break;
        }
        let preview = v8::String::new(scope, &i.to_string())
            .and_then(|index_key| array.get_own_property_descriptor(scope, index_key.into()))
            .map(|desc| preview_from_descriptor(scope, desc, depth + 1, None))
            .unwrap_or_else(|| "undefined".to_string());
        out.push_str(&preview);
    }
    if (len as usize) > cap {
        if cap > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "…+{}", len as usize - cap);
    }
    out.push(']');
    truncate_preview(out)
}

fn format_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
    depth: usize,
) -> String {
    if depth >= PREVIEW_MAX_DEPTH {
        return "{…}".to_string();
    }
    let Some(object) = value.to_object(scope) else {
        return "{…}".to_string();
    };
    let keys = object
        .get_own_property_names(scope, Default::default())
        .unwrap_or_else(|| v8::Array::new(scope, 0));
    let len = keys.length();
    let mut out = String::from("{");
    let cap = PREVIEW_MAX_PROPS.min(len as usize);
    for i in 0..cap {
        if i > 0 {
            out.push_str(", ");
        }
        if out.len() >= PREVIEW_MAX_LEN {
            out.push('…');
            break;
        }
        let key = keys
            .get_index(scope, i as u32)
            .unwrap_or_else(|| v8::undefined(scope).into());
        let key_label = property_key_label(scope, key);
        let Some(prop_name) = property_key_as_name(scope, key) else {
            continue;
        };
        let preview = match object.get_own_property_descriptor(scope, prop_name) {
            Some(desc) => preview_from_descriptor(scope, desc, depth + 1, Some(&key_label)),
            None => "undefined".to_string(),
        };
        let _ = write!(out, "{key_label}: {preview}");
    }
    if (len as usize) > cap {
        if cap > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "…+{}", len as usize - cap);
    }
    out.push('}');
    truncate_preview(out)
}

fn property_key_as_name<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    key: v8::Local<'s, v8::Value>,
) -> Option<v8::Local<'s, v8::Name>> {
    if key.is_string() {
        return v8::Local::<v8::String>::try_from(key)
            .ok()
            .map(|s| s.into());
    }
    if key.is_symbol() {
        return v8::Local::<v8::Symbol>::try_from(key)
            .ok()
            .map(|s| s.into());
    }
    if key.is_int32() {
        let index = key.int32_value(scope).unwrap_or(0);
        return v8::String::new(scope, &index.to_string()).map(|s| s.into());
    }
    if key.is_uint32() {
        let index = key.uint32_value(scope).unwrap_or(0);
        return v8::String::new(scope, &index.to_string()).map(|s| s.into());
    }
    None
}

fn property_key_label<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    key: v8::Local<'s, v8::Value>,
) -> String {
    if key.is_string() {
        return key.to_rust_string_lossy(scope);
    }
    if key.is_symbol() {
        return format_symbol(scope, key);
    }
    if key.is_int32() || key.is_uint32() {
        return key.to_rust_string_lossy(scope);
    }
    "?".to_string()
}

fn preview_from_descriptor<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    descriptor: v8::Local<'s, v8::Value>,
    depth: usize,
    key_label: Option<&str>,
) -> String {
    if descriptor_has_accessor(scope, descriptor) {
        return key_label
            .map(|k| format!("get {k}"))
            .unwrap_or_else(|| "get …".to_string());
    }
    if let Some(data) = descriptor_data_value(scope, descriptor) {
        return format_v8_preview(scope, data, depth);
    }
    "undefined".to_string()
}

/// Read a field from a property-descriptor plain object (engine-owned), not the user target.
fn descriptor_data_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    descriptor: v8::Local<'s, v8::Value>,
) -> Option<v8::Local<'s, v8::Value>> {
    let desc_obj = descriptor.to_object(scope)?;
    let key = v8::String::new(scope, "value")?;
    desc_obj.get(scope, key.into())
}

fn descriptor_has_accessor<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    descriptor: v8::Local<'s, v8::Value>,
) -> bool {
    let Some(desc_obj) = descriptor.to_object(scope) else {
        return false;
    };
    let (Some(get_key), Some(set_key)) =
        (v8::String::new(scope, "get"), v8::String::new(scope, "set"))
    else {
        return false;
    };
    let has_get = desc_obj
        .get(scope, get_key.into())
        .is_some_and(|v| v.is_function());
    let has_set = desc_obj
        .get(scope, set_key.into())
        .is_some_and(|v| v.is_function());
    has_get || has_set
}

fn truncate_preview(mut text: String) -> String {
    if text.len() <= PREVIEW_MAX_LEN {
        return text;
    }
    let end = text.floor_char_boundary(PREVIEW_MAX_LEN.saturating_sub(1));
    text.truncate(end);
    text.push('…');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_js_string_escapes_controls() {
        assert_eq!(quote_js_string("a\nb"), "\"a\\nb\"");
        assert_eq!(quote_js_string(r#"say "hi""#), r#""say \"hi\"""#);
    }

    #[test]
    fn json_eval_source_wraps_with_stringify() {
        assert_eq!(
            json_eval_source("1 + 1"),
            r#"JSON.stringify((0,eval)("1 + 1"))"#
        );
        assert_eq!(
            json_eval_source(r#"say "hi""#),
            r#"JSON.stringify((0,eval)("say \"hi\""))"#
        );
    }
}
