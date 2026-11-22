//! Cross-thread console events from the V8 worker to the UI thread (DevTools Console).

use std::sync::Mutex;

/// Payload from `__glide_console` or the uncaught-exception listener on the worker thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleWireEvent {
    pub level: String,
    pub message: String,
    pub source: Option<String>,
    pub line: Option<usize>,
}

static QUEUE: Mutex<Vec<ConsoleWireEvent>> = Mutex::new(Vec::new());

fn enqueue(event: ConsoleWireEvent) {
    if let Ok(mut q) = QUEUE.lock() {
        q.push(event);
    }
}

/// Called on the `glide-js` thread from the console binding and uncaught handler.
pub fn publish(level: &str, message: &str, source: Option<&str>, line: Option<usize>) {
    enqueue(ConsoleWireEvent {
        level: level.to_string(),
        message: message.to_string(),
        source: source.map(str::to_string),
        line,
    });
}

/// Source label from the current script context when the page called `console.*`.
pub fn publish_console_level(level: &str, message: &str) {
    let ctx = super::diagnostics::current();
    let source = ctx
        .script_name
        .as_deref()
        .filter(|s| !s.is_empty())
        .or(ctx.page_url.as_deref());
    publish(level, message, source, None);
}

pub fn publish_uncaught(message: &str, line: Option<usize>, resource: Option<&str>) {
    publish("uncaught", message, resource, line);
}

pub fn publish_unhandled_rejection(message: &str) {
    publish("rejection", message, None, None);
}

/// Non-blocking drain for the UI host pump and tests.
pub fn drain_pending() -> Vec<ConsoleWireEvent> {
    match QUEUE.lock() {
        Ok(mut q) => std::mem::take(&mut q),
        Err(poisoned) => std::mem::take(&mut poisoned.into_inner()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    static UNIT_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn publish_delivers_on_receiver() {
        let _lock = UNIT_LOCK.lock().expect("console_feed unit lock");
        while !drain_pending().is_empty() {}
        publish_console_level("log", "hello feed");
        let batch = drain_pending();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].level, "log");
        assert_eq!(batch[0].message, "hello feed");
    }
}
