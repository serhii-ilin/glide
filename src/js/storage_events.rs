//! Cross-document `storage` events for tuple origins.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crossbeam_channel::Sender;

use super::worker::WorkerCommand;
use crate::web_storage::{StorageArea, StorageChange};

type SenderMap = HashMap<u64, Sender<WorkerCommand>>;

fn lock_senders() -> std::sync::MutexGuard<'static, SenderMap> {
    static SENDERS: OnceLock<Mutex<SenderMap>> = OnceLock::new();
    let mux = SENDERS.get_or_init(|| Mutex::new(HashMap::new()));
    mux.lock().unwrap_or_else(|p| p.into_inner())
}

/// Register the JS worker command channel for a DOM generation.
pub fn register_command_sender(dom_generation: u64, tx: Sender<WorkerCommand>) {
    lock_senders().insert(dom_generation, tx);
}

/// Drop the command channel when a page worker retires.
pub fn unregister_command_sender(dom_generation: u64) {
    lock_senders().remove(&dom_generation);
}

#[derive(Debug, Clone)]
struct Listener {
    dom_generation: u64,
    origin: String,
    _page_url: String,
}

fn lock_listeners() -> std::sync::MutexGuard<'static, HashMap<u64, Listener>> {
    static LISTENERS: OnceLock<Mutex<HashMap<u64, Listener>>> = OnceLock::new();
    let mux = LISTENERS.get_or_init(|| Mutex::new(HashMap::new()));
    mux.lock().unwrap_or_else(|p| p.into_inner())
}

/// Attach this worker generation to its page origin for storage notifications.
pub fn attach(dom_generation: u64, origin: String, page_url: String) {
    lock_listeners().insert(
        dom_generation,
        Listener {
            dom_generation,
            origin,
            _page_url: page_url,
        },
    );
}

pub fn detach(dom_generation: u64) {
    lock_listeners().remove(&dom_generation);
}

pub fn notify_other_listeners(origin: &str, source_generation: u64, change: &StorageChange) {
    if change.area != StorageArea::Local {
        return;
    }
    let json = serde_json::json!({
        "key": change.key,
        "oldValue": change.old_value,
        "newValue": change.new_value,
        "url": change.page_url,
        "storageArea": "localStorage",
    });
    let payload = json.to_string();
    let listeners: Vec<u64> = lock_listeners()
        .values()
        .filter(|l| l.origin == origin && l.dom_generation != source_generation)
        .map(|l| l.dom_generation)
        .collect();
    let senders = lock_senders();
    for generation in listeners {
        if let Some(tx) = senders.get(&generation) {
            let _ = tx.send(WorkerCommand::StorageEvent {
                json: payload.clone(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use crossbeam_channel::unbounded;

    use super::*;
    use crate::web_storage::StorageArea;

    #[test]
    fn notifies_peer_generation_on_same_origin() {
        let (tx1, rx1) = unbounded();
        let (tx2, rx2) = unbounded();
        register_command_sender(1, tx1);
        register_command_sender(2, tx2);
        attach(
            1,
            "http://example.test".into(),
            "http://example.test/a".into(),
        );
        attach(
            2,
            "http://example.test".into(),
            "http://example.test/b".into(),
        );
        let change = StorageChange {
            area: StorageArea::Local,
            key: "k".into(),
            old_value: None,
            new_value: Some("v".into()),
            page_url: "http://example.test/a".into(),
            origin: "http://example.test".into(),
        };
        notify_other_listeners("http://example.test", 1, &change);
        assert!(rx2.try_recv().is_ok());
        assert!(rx1.try_recv().is_err());
        unregister_command_sender(1);
        unregister_command_sender(2);
        detach(1);
        detach(2);
    }
}
