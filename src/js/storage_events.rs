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
    tab_id: u64,
    _page_url: String,
}

fn lock_listeners() -> std::sync::MutexGuard<'static, HashMap<u64, Listener>> {
    static LISTENERS: OnceLock<Mutex<HashMap<u64, Listener>>> = OnceLock::new();
    let mux = LISTENERS.get_or_init(|| Mutex::new(HashMap::new()));
    mux.lock().unwrap_or_else(|p| p.into_inner())
}

/// Attach this worker generation to its page origin for storage notifications.
pub fn attach(dom_generation: u64, origin: String, page_url: String, tab_id: u64) {
    lock_listeners().insert(
        dom_generation,
        Listener {
            dom_generation,
            origin,
            tab_id,
            _page_url: page_url,
        },
    );
}

pub fn detach(dom_generation: u64) {
    lock_listeners().remove(&dom_generation);
}

pub fn notify_other_listeners(
    origin: &str,
    tab_id: u64,
    source_generation: u64,
    change: &StorageChange,
) {
    let storage_area = match change.area {
        StorageArea::Local => "localStorage",
        StorageArea::Session => "sessionStorage",
    };
    let json = serde_json::json!({
        "key": change.key,
        "oldValue": change.old_value,
        "newValue": change.new_value,
        "url": change.page_url,
        "storageArea": storage_area,
    });
    let payload = json.to_string();
    let listeners: Vec<u64> = lock_listeners()
        .values()
        .filter(|l| {
            l.origin == origin
                && l.dom_generation != source_generation
                && (change.area == StorageArea::Local || l.tab_id == tab_id)
        })
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
            5,
        );
        attach(
            2,
            "http://example.test".into(),
            "http://example.test/b".into(),
            5,
        );
        let change = StorageChange {
            area: StorageArea::Local,
            key: Some("k".into()),
            old_value: None,
            new_value: Some("v".into()),
            page_url: "http://example.test/a".into(),
            origin: "http://example.test".into(),
        };
        notify_other_listeners("http://example.test", 5, 1, &change);
        assert!(rx2.try_recv().is_ok());
        assert!(rx1.try_recv().is_err());
        unregister_command_sender(1);
        unregister_command_sender(2);
        detach(1);
        detach(2);
    }

    #[test]
    fn clear_event_payload_uses_null_key() {
        let (tx1, rx1) = unbounded();
        let (tx2, rx2) = unbounded();
        register_command_sender(1, tx1);
        register_command_sender(2, tx2);
        attach(
            1,
            "http://example.test".into(),
            "http://example.test/a".into(),
            9,
        );
        attach(
            2,
            "http://example.test".into(),
            "http://example.test/b".into(),
            9,
        );
        let change = StorageChange {
            area: StorageArea::Local,
            key: None,
            old_value: None,
            new_value: None,
            page_url: "http://example.test/a".into(),
            origin: "http://example.test".into(),
        };
        notify_other_listeners("http://example.test", 9, 1, &change);
        let json = match rx2.try_recv().expect("one event") {
            WorkerCommand::StorageEvent { json } => json,
            other => panic!("unexpected worker command: {other:?}"),
        };
        assert!(rx1.try_recv().is_err());
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("json");
        assert!(parsed.get("key").unwrap().is_null());
        assert!(parsed.get("oldValue").unwrap().is_null());
        assert!(parsed.get("newValue").unwrap().is_null());
        unregister_command_sender(1);
        unregister_command_sender(2);
        detach(1);
        detach(2);
    }

    #[test]
    fn session_storage_notifies_same_tab_peer() {
        let (tx1, rx1) = unbounded();
        let (tx2, rx2) = unbounded();
        register_command_sender(10, tx1);
        register_command_sender(11, tx2);
        attach(
            10,
            "http://example.test".into(),
            "http://example.test/a".into(),
            7,
        );
        attach(
            11,
            "http://example.test".into(),
            "http://example.test/b".into(),
            7,
        );
        attach(
            12,
            "http://example.test".into(),
            "http://example.test/c".into(),
            8,
        );
        register_command_sender(12, unbounded().0);
        let change = StorageChange {
            area: StorageArea::Session,
            key: Some("s".into()),
            old_value: None,
            new_value: Some("1".into()),
            page_url: "http://example.test/a".into(),
            origin: "http://example.test".into(),
        };
        notify_other_listeners("http://example.test", 7, 10, &change);
        assert!(rx2.try_recv().is_ok());
        assert!(rx1.try_recv().is_err());
        unregister_command_sender(10);
        unregister_command_sender(11);
        unregister_command_sender(12);
        detach(10);
        detach(11);
        detach(12);
    }
}
