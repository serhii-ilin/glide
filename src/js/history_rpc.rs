//! DOM RPC handlers for the unified session history stack.

use std::sync::{Arc, Mutex};

use serde::Deserialize;
use url::Url;

use super::bridge::{DomResponse, DomValue};
use crate::history::{History, HistoryRpcSnapshot, HistoryStep};

#[derive(Debug, Deserialize)]
struct PushReplacePayload {
    state: Option<serde_json::Value>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    base_url: String,
}

pub fn apply_history_snapshot(history: &Arc<Mutex<History>>, reply: u64) -> DomResponse {
    let guard = match history.lock() {
        Ok(g) => g,
        Err(_) => return DomResponse::err(reply, "history lock poisoned"),
    };
    json_response(reply, guard.snapshot())
}

pub fn apply_history_push(history: &Arc<Mutex<History>>, body: &str, reply: u64) -> DomResponse {
    let payload: PushReplacePayload = match serde_json::from_str(body) {
        Ok(p) => p,
        Err(e) => return DomResponse::err(reply, format!("invalid history push payload: {e}")),
    };
    let resolved = match resolve_session_url(payload.url.as_deref(), &payload.base_url) {
        Ok(url) => url,
        Err(e) => return DomResponse::err(reply, e),
    };
    let state = payload.state.unwrap_or(serde_json::Value::Null);
    let mut guard = match history.lock() {
        Ok(g) => g,
        Err(_) => return DomResponse::err(reply, "history lock poisoned"),
    };
    if let Err(e) = guard.push_state(resolved, state) {
        return DomResponse::err(reply, e.message().to_string());
    }
    json_response(reply, guard.snapshot())
}

pub fn apply_history_replace(history: &Arc<Mutex<History>>, body: &str, reply: u64) -> DomResponse {
    let payload: PushReplacePayload = match serde_json::from_str(body) {
        Ok(p) => p,
        Err(e) => return DomResponse::err(reply, format!("invalid history replace payload: {e}")),
    };
    let resolved = match payload.url.as_deref() {
        Some(raw) if !raw.is_empty() => match resolve_session_url(Some(raw), &payload.base_url) {
            Ok(url) => Some(url),
            Err(e) => return DomResponse::err(reply, e),
        },
        _ => None,
    };
    let state = payload.state.unwrap_or(serde_json::Value::Null);
    let mut guard = match history.lock() {
        Ok(g) => g,
        Err(_) => return DomResponse::err(reply, "history lock poisoned"),
    };
    if let Err(e) = guard.replace_state(resolved, state) {
        return DomResponse::err(reply, e.message().to_string());
    }
    json_response(reply, guard.snapshot())
}

pub fn apply_history_go(history: &Arc<Mutex<History>>, delta: i32, reply: u64) -> DomResponse {
    let mut guard = match history.lock() {
        Ok(g) => g,
        Err(_) => return DomResponse::err(reply, "history lock poisoned"),
    };
    let step = if delta == 0 { None } else { guard.go(delta) };
    let mut snap = guard.snapshot();
    if let Some(step) = step {
        apply_step_to_snapshot(&step, &mut snap);
    }
    json_response(reply, snap)
}

pub fn apply_history_go_to_index(
    history: &Arc<Mutex<History>>,
    target: usize,
    reply: u64,
) -> DomResponse {
    let mut guard = match history.lock() {
        Ok(g) => g,
        Err(_) => return DomResponse::err(reply, "history lock poisoned"),
    };
    if target >= guard.entries().len() {
        return json_response(reply, guard.snapshot());
    }
    let delta = target as i32 - guard.index() as i32;
    let step = guard.go(delta);
    let mut snap = guard.snapshot();
    if let Some(step) = step {
        apply_step_to_snapshot(&step, &mut snap);
    }
    json_response(reply, snap)
}

fn apply_step_to_snapshot(step: &HistoryStep, snap: &mut HistoryRpcSnapshot) {
    match step {
        HistoryStep::SameDocument {
            url,
            state,
            hash_changed,
        } => {
            snap.url = url.clone();
            snap.state = state.clone();
            snap.hash_changed = *hash_changed;
            snap.navigate = None;
        }
        HistoryStep::Document { url } => {
            snap.url = url.clone();
            snap.state = guard_state_for_document(snap.state.take());
            snap.navigate = Some(url.clone());
        }
    }
}

fn guard_state_for_document(_prev: Option<serde_json::Value>) -> Option<serde_json::Value> {
    None
}

fn json_response(reply: u64, snap: HistoryRpcSnapshot) -> DomResponse {
    match serde_json::to_value(snap) {
        Ok(v) => DomResponse::ok(reply, DomValue::Json(v)),
        Err(e) => DomResponse::err(reply, format!("history response encode failed: {e}")),
    }
}

fn resolve_session_url(raw: Option<&str>, base_url: &str) -> Result<String, String> {
    let Some(raw) = raw.filter(|s| !s.is_empty()) else {
        return Ok(base_url.to_string());
    };
    let base = Url::parse(base_url).map_err(|_| "The operation is insecure.".to_string())?;
    let resolved = base
        .join(raw)
        .or_else(|_| Url::parse(raw))
        .map_err(|_| "The string did not match the expected pattern.".to_string())?;
    if !same_session_origin(&base, &resolved) {
        return Err("The operation is insecure.".to_string());
    }
    Ok(resolved.to_string())
}

fn same_session_origin(base: &Url, resolved: &Url) -> bool {
    if base.origin() == resolved.origin() {
        return true;
    }
    base.scheme() == "file" && resolved.scheme() == "file"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_origin_push_is_rejected() {
        let err = resolve_session_url(Some("https://evil.test/x"), "https://example.test/")
            .expect_err("origin");
        assert!(err.contains("insecure"));
    }

    #[test]
    fn push_rpc_updates_length() {
        let history = Arc::new(Mutex::new(History::new("https://example.test/".into())));
        let resp = apply_history_push(
            &history,
            r#"{"state":{"n":1},"url":"/one","base_url":"https://example.test/"}"#,
            1,
        );
        match resp.value {
            DomValue::Json(v) => {
                assert_eq!(v.get("length").and_then(|l| l.as_u64()), Some(2));
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
