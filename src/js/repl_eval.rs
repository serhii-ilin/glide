//! Console REPL evaluation and DevTools-style value preview formatting.

use std::fmt::Write as _;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use super::console_feed;

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

pub fn eval_console_expression<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    source: &str,
    tab_id: u64,
    page_epoch: u64,
) {
    let (route_tab, route_epoch) = console_feed::worker_route();
    if tab_id != route_tab || page_epoch != route_epoch {
        console_feed::publish_repl_error(
            "Expression ignored: active tab or page changed before evaluation.",
        );
        return;
    }
    match eval_expression_value(scope, source) {
        Ok(preview) => console_feed::publish_repl_result(&preview),
        Err(msg) => console_feed::publish_repl_error(&msg),
    }
}

fn eval_expression_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    source: &str,
) -> Result<String, String> {
    let quoted = serde_json::to_string(source).map_err(|e| e.to_string())?;
    let code = format!("(function(){{'use strict'; return eval({quoted});}})()");
    super::diagnostics::set_script(Some("<console>"), None, Some("repl"));
    let value = run_expression_with_timeout(scope, &code, "<repl>")?;
    super::diagnostics::clear_script();
    Ok(format_v8_preview(scope, value, 0))
}

fn run_expression_with_timeout<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    source: &str,
    name: &str,
) -> Result<v8::Local<'s, v8::Value>, String> {
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
    let tc = pin!(v8::TryCatch::new(scope));
    let tc = &mut tc.init();
    let Some(code) = v8::String::new(tc, source) else {
        done.store(true, Ordering::SeqCst);
        return Err("Failed to allocate eval source".to_string());
    };
    let Some(name_v) = v8::String::new(tc, name) else {
        done.store(true, Ordering::SeqCst);
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
        done.store(true, Ordering::SeqCst);
        tc.cancel_terminate_execution();
        return Err("REPL expression failed to compile".to_string());
    };
    let out = script.run(tc);
    done.store(true, Ordering::SeqCst);
    if tc.has_terminated() {
        tc.cancel_terminate_execution();
        return Err("EvalError: script execution timed out after five seconds".to_string());
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

const PREVIEW_MAX_LEN: usize = 240;
const PREVIEW_MAX_DEPTH: usize = 2;
const PREVIEW_MAX_PROPS: usize = 6;

pub fn format_v8_preview<'s>(
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
    if value.is_boolean() {
        return value.boolean_value(scope).to_string();
    }
    if value.is_number() {
        return value.to_rust_string_lossy(scope);
    }
    if value.is_string() {
        return quote_js_string(&value.to_rust_string_lossy(scope));
    }
    if value.is_function() {
        let label = value
            .to_object(scope)
            .and_then(|obj| {
                let key = v8::String::new(scope, "name")?;
                let name = obj.get(scope, key.into())?;
                if name.is_string() {
                    Some(name.to_rust_string_lossy(scope))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "anonymous".to_string());
        return format!("f {label}()");
    }
    if value.is_array() {
        return format_array(scope, value, depth);
    }
    if value.is_object() {
        return format_object(scope, value, depth);
    }
    value.to_rust_string_lossy(scope)
}

fn format_array<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
    depth: usize,
) -> String {
    let Some(array) = value.to_object(scope) else {
        return value.to_rust_string_lossy(scope);
    };
    let len = v8::String::new(scope, "length")
        .and_then(|key| array.get(scope, key.into()))
        .and_then(|v| v.to_int32(scope))
        .map(|i| i.value().max(0) as u32)
        .unwrap_or(0);
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
        let key = v8::Integer::new(scope, i as i32);
        let elem = array
            .get(scope, key.into())
            .unwrap_or_else(|| v8::undefined(scope).into());
        out.push_str(&format_v8_preview(scope, elem, depth + 1));
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
        return value.to_rust_string_lossy(scope);
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
        let key_val = keys
            .get(scope, v8::Integer::new(scope, i as i32).into())
            .unwrap_or_else(|| v8::undefined(scope).into());
        let key = key_val.to_rust_string_lossy(scope);
        let prop = object
            .get(scope, key_val)
            .unwrap_or_else(|| v8::undefined(scope).into());
        let _ = write!(
            out,
            "{}: {}",
            key,
            format_v8_preview(scope, prop, depth + 1)
        );
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
}
