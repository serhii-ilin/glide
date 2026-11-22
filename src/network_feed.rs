//! Completed network requests from net/JS threads to the UI thread (DevTools Network).

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::devtools::network::{NetworkInitiator, NetworkWireEvent};
use crate::tabs::TabId;

const QUEUE_CAPACITY: usize = 400;

static QUEUE: Mutex<VecDeque<NetworkWireEvent>> = Mutex::new(VecDeque::new());

thread_local! {
    static WORKER_ROUTE: std::cell::RefCell<(TabId, u64)> = const { std::cell::RefCell::new((0, 0)) };
}

#[derive(Debug, Default)]
pub struct NetworkRoute {
    tab_id: std::sync::atomic::AtomicU64,
    epoch: std::sync::atomic::AtomicU64,
}

/// Tab, epoch and start timestamp captured when a request begins (not when it completes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkStamp {
    pub tab_id: TabId,
    pub network_epoch: u64,
    pub started_ms: u64,
}

impl NetworkRoute {
    pub fn set(&self, tab_id: TabId, epoch: u64) {
        self.tab_id
            .store(tab_id, std::sync::atomic::Ordering::Release);
        self.epoch
            .store(epoch, std::sync::atomic::Ordering::Release);
    }

    pub fn pair(&self) -> (TabId, u64) {
        (
            self.tab_id.load(std::sync::atomic::Ordering::Acquire),
            self.epoch.load(std::sync::atomic::Ordering::Acquire),
        )
    }

    pub fn capture_stamp(&self) -> NetworkStamp {
        let (tab_id, network_epoch) = self.pair();
        NetworkStamp {
            tab_id,
            network_epoch,
            started_ms: unix_now_ms(),
        }
    }

    /// Bump the shared epoch when a new top-level document fetch starts.
    pub fn begin_document_load(&self) -> NetworkStamp {
        let tab_id = self.tab_id.load(std::sync::atomic::Ordering::Acquire);
        let network_epoch = self
            .epoch
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            .saturating_add(1);
        NetworkStamp {
            tab_id,
            network_epoch,
            started_ms: unix_now_ms(),
        }
    }
}

pub fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn lock_queue_mut() -> std::sync::MutexGuard<'static, VecDeque<NetworkWireEvent>> {
    QUEUE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn enqueue(event: NetworkWireEvent) {
    let mut q = lock_queue_mut();
    if q.len() >= QUEUE_CAPACITY {
        q.pop_front();
    }
    q.push_back(event);
}

pub fn set_worker_route(tab_id: TabId, network_epoch: u64) {
    WORKER_ROUTE.with(|r| *r.borrow_mut() = (tab_id, network_epoch));
}

pub fn capture_worker_stamp() -> NetworkStamp {
    let (tab_id, network_epoch) = WORKER_ROUTE.with(|r| *r.borrow());
    NetworkStamp {
        tab_id,
        network_epoch,
        started_ms: unix_now_ms(),
    }
}

/// Payload for one completed request (net thread or JS worker → UI).
#[derive(Debug, Clone)]
pub struct NetworkCompleted {
    pub method: String,
    pub url: String,
    pub initiator: NetworkInitiator,
    pub status: Option<u16>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub size_bytes: Option<u64>,
    pub response_headers: Vec<(String, String)>,
}

pub fn publish_completed(stamp: NetworkStamp, completed: NetworkCompleted) {
    enqueue(NetworkWireEvent {
        tab_id: stamp.tab_id,
        network_epoch: stamp.network_epoch,
        method: completed.method,
        url: completed.url,
        initiator: completed.initiator,
        status: completed.status,
        error: completed.error,
        duration_ms: completed.duration_ms,
        size_bytes: completed.size_bytes,
        response_headers: completed.response_headers,
        started_ms: stamp.started_ms,
    });
}

pub fn drain_pending() -> Vec<NetworkWireEvent> {
    let mut q = lock_queue_mut();
    q.drain(..).collect()
}

pub fn discard_pending_for_tab(tab_id: TabId) {
    let mut q = lock_queue_mut();
    q.retain(|ev| ev.tab_id != tab_id);
}

pub fn discard_pending_below_epoch(tab_id: TabId, min_epoch: u64) {
    let mut q = lock_queue_mut();
    q.retain(|ev| ev.tab_id != tab_id || ev.network_epoch >= min_epoch);
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

    fn drain_all() {
        let _ = drain_pending();
    }

    #[test]
    fn publish_completed_allows_tab_zero() {
        drain_all();
        let route = NetworkRoute::default();
        route.set(0, 1);
        let stamp = route.capture_stamp();
        publish_completed(
            stamp,
            NetworkCompleted {
                method: "GET".into(),
                url: "file://doc".into(),
                initiator: NetworkInitiator::Document,
                status: Some(200),
                error: None,
                duration_ms: 1,
                size_bytes: Some(4),
                response_headers: Vec::new(),
            },
        );
        let batch = drain_pending();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].tab_id, 0);
        assert_eq!(batch[0].network_epoch, 1);
    }

    #[test]
    fn publish_uses_captured_stamp_not_live_route() {
        drain_all();
        let route = NetworkRoute::default();
        route.set(1, 1);
        let stamp = route.capture_stamp();
        route.set(2, 2);
        thread::spawn(move || {
            publish_completed(
                stamp,
                NetworkCompleted {
                    method: "GET".into(),
                    url: "file://a".into(),
                    initiator: NetworkInitiator::Fetch,
                    status: Some(200),
                    error: None,
                    duration_ms: 0,
                    size_bytes: None,
                    response_headers: Vec::new(),
                },
            );
        })
        .join()
        .expect("join");
        let batch = drain_pending();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].tab_id, 1);
        assert_eq!(batch[0].network_epoch, 1);
    }

    #[test]
    fn discard_pending_for_tab_drops_matching() {
        drain_all();
        publish_completed(
            NetworkStamp {
                tab_id: 1,
                network_epoch: 1,
                started_ms: 0,
            },
            NetworkCompleted {
                method: "GET".into(),
                url: "file://a".into(),
                initiator: NetworkInitiator::Fetch,
                status: Some(200),
                error: None,
                duration_ms: 1,
                size_bytes: Some(3),
                response_headers: Vec::new(),
            },
        );
        publish_completed(
            NetworkStamp {
                tab_id: 2,
                network_epoch: 1,
                started_ms: 0,
            },
            NetworkCompleted {
                method: "GET".into(),
                url: "file://b".into(),
                initiator: NetworkInitiator::Fetch,
                status: Some(200),
                error: None,
                duration_ms: 1,
                size_bytes: Some(3),
                response_headers: Vec::new(),
            },
        );
        discard_pending_for_tab(1);
        let remaining = drain_pending();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].tab_id, 2);
    }
}
