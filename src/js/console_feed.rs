//! Cross-thread console events from the V8 worker to the UI thread (DevTools Console).

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Instant;

use crate::tabs::TabId;

pub const QUEUE_CAPACITY: usize = 500;
pub const MAX_MESSAGE_LEN: usize = 8_192;
pub const MAX_SOURCE_LEN: usize = 2_048;

/// Payload from `__glide_console`, uncaught exceptions, or promise rejection bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleWireEvent {
    pub tab_id: TabId,
    pub page_epoch: u64,
    pub timestamp_ms: u64,
    pub level: String,
    pub message: String,
    pub source: Option<String>,
    pub line: Option<usize>,
    /// Stable identity for a rejection row; `rejection-revoke` removes the matching row.
    pub promise_id: Option<u64>,
}

static QUEUE: Mutex<VecDeque<ConsoleWireEvent>> = Mutex::new(VecDeque::new());

#[cfg(test)]
static TEST_CONSOLE_SERIAL: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub fn test_console_serial_lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_CONSOLE_SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

thread_local! {
    static WORKER_ROUTE: std::cell::RefCell<(TabId, u64)> = const { std::cell::RefCell::new((0, 0)) };
    static WORKER_CLOCK: std::cell::RefCell<Option<Instant>> = const { std::cell::RefCell::new(None) };
}

fn worker_timestamp_ms() -> u64 {
    WORKER_CLOCK.with(|slot| {
        let mut opt = slot.borrow_mut();
        let start = opt.get_or_insert_with(Instant::now);
        start.elapsed().as_millis() as u64
    })
}

fn truncate_field(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }
    let end = value.floor_char_boundary(max_len.saturating_sub(1));
    format!("{}…", &value[..end])
}

fn lock_queue_mut() -> std::sync::MutexGuard<'static, VecDeque<ConsoleWireEvent>> {
    match QUEUE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn enqueue(event: ConsoleWireEvent) {
    let mut q = lock_queue_mut();
    if q.len() >= QUEUE_CAPACITY {
        q.pop_front();
    }
    q.push_back(event);
}

pub fn set_worker_route(tab_id: TabId, page_epoch: u64) {
    WORKER_ROUTE.with(|r| *r.borrow_mut() = (tab_id, page_epoch));
    WORKER_CLOCK.with(|c| *c.borrow_mut() = Some(Instant::now()));
}

fn current_route() -> (TabId, u64) {
    WORKER_ROUTE.with(|r| *r.borrow())
}

fn publish_wire(
    level: &str,
    message: &str,
    source: Option<&str>,
    line: Option<usize>,
    promise_id: Option<u64>,
) {
    let (tab_id, page_epoch) = current_route();
    enqueue(ConsoleWireEvent {
        tab_id,
        page_epoch,
        timestamp_ms: worker_timestamp_ms(),
        level: level.to_string(),
        message: truncate_field(message, MAX_MESSAGE_LEN),
        source: source.map(|s| truncate_field(s, MAX_SOURCE_LEN)),
        line,
        promise_id,
    });
}

/// Called on the `glide-js` thread from the console binding and uncaught handler.
pub fn publish(level: &str, message: &str, source: Option<&str>, line: Option<usize>) {
    publish_wire(level, message, source, line, None);
}

fn console_source_from_ctx(
    ctx: &super::diagnostics::ScriptContext,
) -> (Option<String>, Option<usize>) {
    let source = ctx.page_url.clone().or_else(|| {
        ctx.script_name
            .as_ref()
            .filter(|name| !name.starts_with("script-"))
            .cloned()
    });
    (source, None)
}

/// Source label from the current script context when the page called `console.*`.
pub fn publish_console_level(level: &str, message: &str) {
    let ctx = super::diagnostics::current();
    let (source, line) = console_source_from_ctx(&ctx);
    publish(level, message, source.as_deref(), line);
}

pub fn publish_uncaught(message: &str, line: Option<usize>, resource: Option<&str>) {
    publish("uncaught", message, resource, line);
}

pub fn publish_unhandled_rejection(message: &str, promise_id: u64) {
    publish_wire("rejection", message, None, None, Some(promise_id));
}

pub fn publish_rejection_revoke(promise_id: u64) {
    let (tab_id, page_epoch) = current_route();
    enqueue(ConsoleWireEvent {
        tab_id,
        page_epoch,
        timestamp_ms: worker_timestamp_ms(),
        level: "rejection-revoke".into(),
        message: String::new(),
        source: None,
        line: None,
        promise_id: Some(promise_id),
    });
}

/// Drop queued rows for one tab (Clear / navigation).
pub fn discard_pending_for_tab(tab_id: TabId) {
    let mut q = lock_queue_mut();
    q.retain(|ev| ev.tab_id != tab_id);
}

/// Non-blocking drain for the UI host pump, headless settle, and tests.
pub fn drain_pending() -> Vec<ConsoleWireEvent> {
    lock_queue_mut().drain(..).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_delivers_on_receiver() {
        let _lock = test_console_serial_lock();
        while !drain_pending().is_empty() {}
        set_worker_route(7, 2);
        publish_console_level("log", "hello feed");
        let batch = drain_pending();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].level, "log");
        assert_eq!(batch[0].message, "hello feed");
        assert_eq!(batch[0].tab_id, 7);
        assert_eq!(batch[0].page_epoch, 2);
    }

    #[test]
    fn queue_caps_at_capacity_dropping_oldest() {
        let _lock = test_console_serial_lock();
        drain_pending();
        set_worker_route(1, 1);
        for i in 0..QUEUE_CAPACITY + 5 {
            publish("log", &format!("m{i}"), None, None);
        }
        let batch = drain_pending();
        assert_eq!(batch.len(), QUEUE_CAPACITY);
        assert_eq!(batch[0].message, "m5");
    }

    #[test]
    fn publish_truncates_long_messages() {
        let _lock = test_console_serial_lock();
        drain_pending();
        set_worker_route(1, 1);
        let long = "x".repeat(MAX_MESSAGE_LEN + 64);
        publish("log", &long, None, None);
        let batch = drain_pending();
        assert_eq!(batch.len(), 1);
        assert!(batch[0].message.len() <= MAX_MESSAGE_LEN + 4);
        assert!(batch[0].message.ends_with('…'));
    }

    #[test]
    fn discard_pending_for_tab_removes_only_that_tab() {
        let _lock = test_console_serial_lock();
        drain_pending();
        set_worker_route(1, 1);
        publish("log", "a", None, None);
        set_worker_route(2, 1);
        publish("log", "b", None, None);
        discard_pending_for_tab(1);
        let batch = drain_pending();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].tab_id, 2);
    }
}
