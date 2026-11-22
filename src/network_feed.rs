//! Completed network requests from net/JS threads to the UI thread (DevTools Network).

use std::collections::VecDeque;
use std::sync::Mutex;

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

fn worker_route() -> (TabId, u64) {
    WORKER_ROUTE.with(|r| *r.borrow())
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
    pub started_ms: u64,
}

pub fn publish_completed_from_js(completed: NetworkCompleted) {
    let (tab_id, network_epoch) = worker_route();
    enqueue(NetworkWireEvent {
        tab_id,
        network_epoch,
        method: completed.method,
        url: completed.url,
        initiator: completed.initiator,
        status: completed.status,
        error: completed.error,
        duration_ms: completed.duration_ms,
        size_bytes: completed.size_bytes,
        response_headers: completed.response_headers,
        started_ms: completed.started_ms,
    });
}

pub fn publish_completed_from_net(route: &NetworkRoute, completed: NetworkCompleted) {
    let (tab_id, network_epoch) = route.pair();
    if tab_id == 0 {
        return;
    }
    enqueue(NetworkWireEvent {
        tab_id,
        network_epoch,
        method: completed.method,
        url: completed.url,
        initiator: completed.initiator,
        status: completed.status,
        error: completed.error,
        duration_ms: completed.duration_ms,
        size_bytes: completed.size_bytes,
        response_headers: completed.response_headers,
        started_ms: completed.started_ms,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discard_pending_for_tab_drops_matching() {
        enqueue(NetworkWireEvent {
            tab_id: 1,
            network_epoch: 1,
            method: "GET".into(),
            url: "file://a".into(),
            initiator: NetworkInitiator::Fetch,
            status: Some(200),
            error: None,
            duration_ms: 1,
            size_bytes: Some(3),
            response_headers: Vec::new(),
            started_ms: 0,
        });
        discard_pending_for_tab(1);
        assert!(drain_pending().is_empty());
    }
}
